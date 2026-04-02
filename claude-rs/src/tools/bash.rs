use crate::tools::core::{Tool, ToolResult, ToolUseContext};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

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
