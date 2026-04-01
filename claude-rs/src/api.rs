use reqwest::{Client, Error as ReqwestError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: String, is_error: Option<bool> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Serialize, Debug)]
pub struct CreateMessageRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    pub messages: &'a [Message],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSchema>,
}

#[derive(Deserialize, Debug)]
pub struct CreateMessageResponse {
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
}

/// A lightweight Anthropic API client wrapper.
pub struct AnthropicClient {
    api_key: String,
    client: Client,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }

    /// Sends a non-streaming request to the Anthropic Messages API.
    pub async fn create_message(
        &self,
        req: CreateMessageRequest<'_>
    ) -> Result<CreateMessageResponse, ReqwestError> {
        let url = "https://api.anthropic.com/v1/messages";

        let response = self.client.post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&req)
            .send()
            .await?;

        response.json::<CreateMessageResponse>().await
    }
}
