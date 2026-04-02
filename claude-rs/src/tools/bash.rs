use crate::tools::core::{Tool, ToolResult, ToolUseContext};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "Bash"
    }

    async fn description(&self, _input: &Value, _ctx: &ToolUseContext) -> Result<String> {
        Ok("Executes a bash command in the local environment and returns stdout, stderr, and exit code.".to_string())
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    async fn call(
        &self,
        args: Value,
        _context: &mut ToolUseContext
    ) -> Result<ToolResult> {
        let command_str = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("command is required and must be a string"))?;

        println!("Executing Bash command: {}", command_str);

        let output = Command::new("bash")
            .arg("-c")
            .arg(command_str)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(ToolResult {
            data: serde_json::json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": exit_code
            }),
            mcp_meta: None,
        })
    }
}
