use crate::{Tool, ToolResult, ToolUseContext};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;

// In a real implementation, this would hold an `Arc<Mutex<Coordinator>>`
// or a channel `Sender` to request a spawn. For simplicity, we just
// mock the behavior here.
pub struct AgentTool {}

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &'static str {
        "Agent"
    }

    async fn description(&self, _input: &Value, _ctx: &ToolUseContext) -> Result<String> {
        Ok("Spawns a new worker agent to run a subtask in parallel.".to_string())
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        true
    }

    async fn call(
        &self,
        args: Value,
        _context: &mut ToolUseContext
    ) -> Result<ToolResult> {
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("prompt is required and must be a string"))?;

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("subtask");

        println!("Coordinator launching Agent [{}]: {}", description, prompt);

        let task_id = uuid::Uuid::new_v4().to_string();

        Ok(ToolResult {
            data: serde_json::json!({
                "message": format!("Worker agent spawned with ID: {}. Result will be delivered as a user message with <task-notification> when complete.", task_id),
                "task_id": task_id,
            }),
            mcp_meta: None,
        })
    }
}
