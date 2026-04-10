use claude_api::{AnthropicClient, ContentBlock, CreateMessageRequest, Message, Role, ToolSchema};
use claude_tools::{Tool, ToolUseContext};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tiktoken_rs::cl100k_base;

#[derive(Debug)]
pub enum EngineEvent {
    TextDelta(String),
    ToolExecutionStarted(String),
    ToolExecutionCompleted(String, String),
    ToolExecutionError(String, String),
    TurnCompleted,
    Error(String),
    PermissionRequested(String, String, oneshot::Sender<bool>),
    // Signal to UI that context was compacted to save tokens
    ContextCompacted(usize, usize), // original_tokens, new_tokens
}

const AUTOCOMPACT_THRESHOLD: usize = 180_000; // Threshold before we compress

pub struct QueryEngine<'a> {
    client: AnthropicClient,
    messages: Vec<Message>,
    tools: HashMap<String, Box<dyn Tool>>,
    model: String,
    system_prompt: Option<&'a str>,
}

impl<'a> QueryEngine<'a> {
    pub fn new(api_key: String, model: String, system_prompt: Option<&'a str>) -> Self {
        Self {
            client: AnthropicClient::new(api_key),
            messages: Vec::new(),
            tools: HashMap::new(),
            model,
            system_prompt,
        }
    }

    pub fn register_tool(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Fast and dirty local token estimator using cl100k_base.
    /// Anthropic has their own tokenizer, but cl100k_base is close enough for thresholding.
    fn estimate_tokens(&self) -> usize {
        let bpe = cl100k_base().unwrap();
        let mut total = 0;

        if let Some(sys) = self.system_prompt {
            total += bpe.encode_ordinary(sys).len();
        }

        for msg in &self.messages {
            for block in &msg.content {
                match block {
                    ContentBlock::Text { text } => total += bpe.encode_ordinary(text).len(),
                    ContentBlock::ToolResult { content, .. } => total += bpe.encode_ordinary(content).len(),
                    ContentBlock::ToolUse { input, .. } => total += bpe.encode_ordinary(&input.to_string()).len(),
                }
            }
        }
        total
    }

    /// Flushes older messages when we cross the token limit
    async fn auto_compact_if_needed(&mut self, tx: &Option<mpsc::Sender<EngineEvent>>) {
        let tokens = self.estimate_tokens();
        if tokens < AUTOCOMPACT_THRESHOLD {
            return;
        }

        // In a real implementation:
        // 1. Spawn a sub-agent to summarize self.messages into a markdown document
        // 2. Clear self.messages
        // 3. Push a System/User message containing the summary

        let original_tokens = tokens;

        // MVP Simulation: Just drop the oldest 50% of messages
        let len = self.messages.len();
        if len > 2 {
            let keep_idx = len / 2;
            self.messages.drain(0..keep_idx);

            // Insert a marker
            self.messages.insert(0, Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "<compact_boundary>Previous conversation was summarized to save context tokens.</compact_boundary>".to_string()
                }]
            });
        }

        let new_tokens = self.estimate_tokens();

        if let Some(sender) = tx {
            let _ = sender.send(EngineEvent::ContextCompacted(original_tokens, new_tokens)).await;
        }
        println!("[AutoCompact] Shrunk context from {} to {} tokens", original_tokens, new_tokens);
    }

    pub async fn submit_message(&mut self, prompt: &str, tx: Option<mpsc::Sender<EngineEvent>>) -> Result<()> {
        self.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
            }],
        });

        loop {
            // Check context limits before sending to API
            self.auto_compact_if_needed(&tx).await;

            let api_tools: Vec<ToolSchema> = self.tools.iter().map(|(name, _t)| ToolSchema {
                name: name.clone(),
                description: "Auto-generated tool description".into(),
                input_schema: serde_json::json!({"type": "object"}),
            }).collect();

            let request = CreateMessageRequest {
                model: &self.model,
                max_tokens: 4096,
                messages: &self.messages,
                system: self.system_prompt,
                tools: api_tools,
                stream: Some(true),
            };

            let mut response = self.client.create_message_stream(request).await?;
            if !response.status().is_success() {
                let err_text = response.text().await?;
                if let Some(ref sender) = tx {
                    let _ = sender.send(EngineEvent::Error(err_text.clone())).await;
                }
                return Err(anyhow!("API Error: {}", err_text));
            }

            let mut current_text = String::new();
            let mut current_tool_use_id = String::new();
            let mut current_tool_name = String::new();
            let mut current_tool_input = String::new();

            let mut final_blocks = Vec::new();
            let mut requires_followup = false;

            while let Some(chunk_res) = response.chunk().await? {
                let chunk_str = String::from_utf8_lossy(&chunk_res);
                for line in chunk_str.lines() {
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" { break; }

                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            let event_type = json["type"].as_str().unwrap_or("");

                            match event_type {
                                "content_block_start" => {
                                    let block_type = json["content_block"]["type"].as_str().unwrap_or("");
                                    if block_type == "tool_use" {
                                        current_tool_use_id = json["content_block"]["id"].as_str().unwrap_or("").to_string();
                                        current_tool_name = json["content_block"]["name"].as_str().unwrap_or("").to_string();
                                    }
                                }
                                "content_block_delta" => {
                                    let delta_type = json["delta"]["type"].as_str().unwrap_or("");
                                    if delta_type == "text_delta" {
                                        if let Some(text) = json["delta"]["text"].as_str() {
                                            current_text.push_str(text);

                                            if let Some(ref sender) = tx {
                                                let _ = sender.send(EngineEvent::TextDelta(text.to_string())).await;
                                            } else {
                                                print!("{}", text);
                                                use std::io::Write;
                                                let _ = std::io::stdout().flush();
                                            }
                                        }
                                    } else if delta_type == "input_json_delta" {
                                        if let Some(partial_json) = json["delta"]["partial_json"].as_str() {
                                            current_tool_input.push_str(partial_json);
                                        }
                                    }
                                }
                                "content_block_stop" => {
                                    if !current_text.is_empty() {
                                        final_blocks.push(ContentBlock::Text { text: current_text.clone() });
                                        current_text.clear();
                                    }
                                    if !current_tool_name.is_empty() {
                                        let parsed_input = serde_json::from_str(&current_tool_input).unwrap_or(serde_json::json!({}));
                                        final_blocks.push(ContentBlock::ToolUse {
                                            id: current_tool_use_id.clone(),
                                            name: current_tool_name.clone(),
                                            input: parsed_input,
                                        });
                                        requires_followup = true;

                                        if let Some(ref sender) = tx {
                                            let _ = sender.send(EngineEvent::ToolExecutionStarted(current_tool_name.clone())).await;
                                        } else {
                                            println!("\n[Running Tool: {}]", current_tool_name);
                                        }

                                        current_tool_name.clear();
                                        current_tool_use_id.clear();
                                        current_tool_input.clear();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            if tx.is_none() {
                println!();
            }

            self.messages.push(Message {
                role: Role::Assistant,
                content: final_blocks.clone(),
            });

            if !requires_followup {
                if let Some(ref sender) = tx {
                    let _ = sender.send(EngineEvent::TurnCompleted).await;
                }
                break;
            }

            let mut tool_results = Vec::new();
            for block in final_blocks {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    if let Some(tool) = self.tools.get(&name) {

                        let mut allowed = true;
                        if tool.is_destructive(&input) {
                            if let Some(ref sender) = tx {
                                let (perm_tx, perm_rx) = oneshot::channel();
                                let input_str = serde_json::to_string_pretty(&input).unwrap_or_default();

                                let _ = sender.send(EngineEvent::PermissionRequested(
                                    name.clone(),
                                    input_str,
                                    perm_tx
                                )).await;

                                allowed = perm_rx.await.unwrap_or(false);
                            }
                        }

                        if !allowed {
                            let deny_msg = "User denied execution of this tool.".to_string();
                            if let Some(ref sender) = tx {
                                let _ = sender.send(EngineEvent::ToolExecutionError(name.clone(), deny_msg.clone())).await;
                            }
                            tool_results.push(ContentBlock::ToolResult {
                                tool_use_id: id,
                                content: deny_msg,
                                is_error: Some(true),
                            });
                            continue;
                        }

                        let mut context = ToolUseContext {
                            agent_id: None,
                            is_non_interactive_session: false,
                        };

                        match tool.call(input, &mut context).await {
                            Ok(result) => {
                                let res_str = result.data.to_string();
                                if let Some(ref sender) = tx {
                                    let _ = sender.send(EngineEvent::ToolExecutionCompleted(name.clone(), res_str.clone())).await;
                                }
                                tool_results.push(ContentBlock::ToolResult {
                                    tool_use_id: id,
                                    content: res_str,
                                    is_error: Some(false),
                                });
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                if let Some(ref sender) = tx {
                                    let _ = sender.send(EngineEvent::ToolExecutionError(name.clone(), err_str.clone())).await;
                                }
                                tool_results.push(ContentBlock::ToolResult {
                                    tool_use_id: id,
                                    content: format!("Tool Error: {}", err_str),
                                    is_error: Some(true),
                                });
                            }
                        }
                    } else {
                        tool_results.push(ContentBlock::ToolResult {
                            tool_use_id: id,
                            content: format!("Tool {} not found", name),
                            is_error: Some(true),
                        });
                    }
                }
            }

            self.messages.push(Message {
                role: Role::User,
                content: tool_results,
            });
        }

        Ok(())
    }
}
