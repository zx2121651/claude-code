use crate::{Tool, ToolResult, ToolUseContext};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use tokio::fs;

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &'static str {
        "FileRead"
    }

    async fn description(&self, _input: &Value, _ctx: &ToolUseContext) -> Result<String> {
        Ok("Reads the contents of a file at the specified absolute_path.".to_string())
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    async fn call(
        &self,
        args: Value,
        _context: &mut ToolUseContext
    ) -> Result<ToolResult> {
        let path_str = args
            .get("absolute_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("absolute_path is required and must be a string"))?;

        println!("Reading file: {}", path_str);

        let path = Path::new(path_str);

        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return Err(anyhow!("Failed to read file '{}': {}", path_str, e)),
        };

        Ok(ToolResult {
            data: serde_json::json!({
                "file_content": content,
            }),
            mcp_meta: None,
        })
    }
}
