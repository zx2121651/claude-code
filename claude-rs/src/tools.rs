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

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "Bash"
    }

    async fn description(&self, _input: &Value, _ctx: &ToolUseContext) -> Result<String> {
        Ok("Executes a bash command in the local environment.".to_string())
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    async fn call(
        &self,
        args: Value,
        _context: &mut ToolUseContext
    ) -> Result<ToolResult> {
        println!("Executing Bash command: {:?}", args.get("command"));

        Ok(ToolResult {
            data: serde_json::json!({
                "stdout": "Simulation successful.",
                "stderr": "",
                "exit_code": 0
            }),
            mcp_meta: None,
        })
    }
}
