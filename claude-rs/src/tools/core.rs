use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

pub struct ToolUseContext {
    pub agent_id: Option<String>,
    pub is_non_interactive_session: bool,
}

pub struct ToolResult {
    pub data: Value,
    pub mcp_meta: Option<Value>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;

    async fn description(
        &self,
        input: &Value,
        options: &ToolUseContext
    ) -> Result<String>;

    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        false
    }

    async fn call(
        &self,
        args: Value,
        context: &mut ToolUseContext
    ) -> Result<ToolResult>;

    async fn validate_input(
        &self,
        _input: &Value,
        _context: &ToolUseContext
    ) -> Result<bool> {
        Ok(true)
    }
}
