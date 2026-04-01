use crate::api::{AnthropicClient, ContentBlock, CreateMessageRequest, Message, Role, ToolSchema};
use crate::tools::{Tool, ToolUseContext};
use anyhow::Result;
use std::collections::HashMap;

/// Rust translation of the `QueryEngine` in Claude Code.
/// Handles the conversation loop, tool execution, and context management.
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

    /// Register a tool with the engine.
    pub fn register_tool(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Submit a user prompt to the engine and run the interaction loop.
    pub async fn submit_message(&mut self, prompt: &str) -> Result<()> {
        // Append user prompt to history
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
            };

            let response = self.client.create_message(request).await?;

            self.messages.push(Message {
                role: Role::Assistant,
                content: response.content.clone(),
            });

            let mut tool_results = Vec::new();
            let mut requires_followup = false;

            for block in &response.content {
                match block {
                    ContentBlock::Text { text } => {
                        println!("Claude: {}", text);
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        println!("[Running Tool: {}]", name);
                        requires_followup = true;

                        if let Some(tool) = self.tools.get(name) {
                            let mut context = ToolUseContext {
                                agent_id: None,
                                is_non_interactive_session: false,
                            };

                            // Await the tool call asynchronously
                            match tool.call(input.clone(), &mut context).await {
                                Ok(result) => {
                                    tool_results.push(ContentBlock::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: result.data.to_string(),
                                        is_error: Some(false),
                                    });
                                }
                                Err(e) => {
                                    tool_results.push(ContentBlock::ToolResult {
                                        tool_use_id: id.clone(),
                                        content: format!("Tool Error: {}", e),
                                        is_error: Some(true),
                                    });
                                }
                            }
                        } else {
                            tool_results.push(ContentBlock::ToolResult {
                                tool_use_id: id.clone(),
                                content: format!("Tool {} not found", name),
                                is_error: Some(true),
                            });
                        }
                    }
                    _ => {}
                }
            }

            if requires_followup {
                self.messages.push(Message {
                    role: Role::User,
                    content: tool_results,
                });
            } else {
                break;
            }
        }

        Ok(())
    }
}
