use crate::tools::core::{Tool, ToolResult, ToolUseContext};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &'static str {
        "Glob"
    }

    async fn description(&self, _input: &Value, _ctx: &ToolUseContext) -> Result<String> {
        Ok("Searches for files matching a glob pattern.".to_string())
    }

    fn is_destructive(&self, _input: &Value) -> bool {
        false
    }

    async fn call(
        &self,
        args: Value,
        _context: &mut ToolUseContext
    ) -> Result<ToolResult> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("pattern is required and must be a string"))?;

        println!("Globbing pattern: {}", pattern);

        let mut matches = Vec::new();

        match glob::glob(pattern) {
            Ok(paths) => {
                for path in paths {
                    match path {
                        Ok(p) => matches.push(p.display().to_string()),
                        Err(e) => println!("Glob error: {:?}", e),
                    }
                }
            }
            Err(e) => return Err(anyhow!("Invalid glob pattern '{}': {}", pattern, e)),
        }

        Ok(ToolResult {
            data: serde_json::json!({
                "matches": matches,
            }),
            mcp_meta: None,
        })
    }
}
