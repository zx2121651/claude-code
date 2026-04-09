use crate::api::{AnthropicClient, ContentBlock, CreateMessageRequest, Message, Role, ToolSchema};
use crate::tools::{Tool, ToolUseContext};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub enum EngineEvent {
    TextDelta(String),
    ToolExecutionStarted(String), // Tool Name
    ToolExecutionCompleted(String, String), // Tool Name, Result
    ToolExecutionError(String, String), // Tool Name, Error message
    TurnCompleted,
    Error(String),
    // Security Sandbox: Request user permission before executing a destructive tool.
    // Contains the tool name, the command/input, and a oneshot sender to reply true (allow) or false (deny).
    PermissionRequested(String, String, oneshot::Sender<bool>),
}

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

    pub async fn submit_message(&mut self, prompt: &str, tx: Option<mpsc::Sender<EngineEvent>>) -> Result<()> {
        self.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
            }],
        });

        loop {
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

                        // --- SECURITY SANDBOX (Permission Request) ---
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

                                // Suspend the engine and await the user's decision from the UI
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
                        // ---------------------------------------------

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
