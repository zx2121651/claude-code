use crate::{Tool, ToolResult, ToolUseContext};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;
use tokio::fs;

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &'static str {
        "FileEdit"
    }

    async fn description(&self, _input: &Value, _ctx: &ToolUseContext) -> Result<String> {
        Ok("Applies targeted search-and-replace edits to a file. It searches for the exact text in `search_string` and replaces it with `replace_string`.".to_string())
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        true
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
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

        let search_string = args
            .get("search_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("search_string is required and must be a string"))?;

        let replace_string = args
            .get("replace_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("replace_string is required and must be a string"))?;

        println!("Editing file: {}", path_str);

        let path = Path::new(path_str);

        let mut content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return Err(anyhow!("Failed to read file '{}': {}", path_str, e)),
        };

        if !content.contains(search_string) {
             return Err(anyhow!("The specified search_string was not found in the file '{}'", path_str));
        }

        // Count occurrences to ensure we're targeting a unique snippet
        let occurrences = content.matches(search_string).count();
        if occurrences > 1 {
             return Err(anyhow!("The search_string appears {} times in the file. Please provide a more specific search string to ensure unique replacement.", occurrences));
        }

        content = content.replace(search_string, replace_string);

        match fs::write(&path, content).await {
            Ok(_) => {},
            Err(e) => return Err(anyhow!("Failed to write to file '{}': {}", path_str, e)),
        };

        Ok(ToolResult {
            data: serde_json::json!({
                "message": format!("Successfully replaced 1 occurrence in {}", path_str),
            }),
            mcp_meta: None,
        })
    }
}
