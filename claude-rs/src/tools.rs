use anyhow::Result;
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

pub struct ToolUseContext {
    pub agent_id: Option<String>,
    pub is_non_interactive_session: bool,
}

pub struct ToolResult<T> {
    pub data: T,
    pub mcp_meta: Option<Value>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    type Input: DeserializeOwned + Send + Sync;
    type Output: Serialize + Send + Sync;

    fn name(&self) -> &'static str;

    async fn description(
        &self,
        input: &Self::Input,
        options: &ToolUseContext
    ) -> Result<String>;

    fn is_destructive(&self, _input: &Self::Input) -> bool {
        false
    }

    fn is_concurrency_safe(&self, _input: &Self::Input) -> bool {
        false
    }

    async fn call(
        &self,
        args: Self::Input,
        context: &mut ToolUseContext
    ) -> Result<ToolResult<Self::Output>>;

    async fn validate_input(
        &self,
        _input: &Self::Input,
        _context: &ToolUseContext
    ) -> Result<bool> {
        Ok(true)
    }
}

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct BashInput {
    pub command: String,
}

#[derive(Serialize, Debug)]
pub struct BashOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    type Input = BashInput;
    type Output = BashOutput;

    fn name(&self) -> &'static str {
        "Bash"
    }

    async fn description(&self, _input: &Self::Input, _ctx: &ToolUseContext) -> Result<String> {
        Ok("Executes a bash command in the local environment.".to_string())
    }

    fn is_destructive(&self, _input: &Self::Input) -> bool {
        true
    }

    async fn call(
        &self,
        args: Self::Input,
        _context: &mut ToolUseContext
    ) -> Result<ToolResult<Self::Output>> {
        println!("Executing Bash command: {}", args.command);

        Ok(ToolResult {
            data: BashOutput {
                stdout: "Simulation successful.".to_string(),
                stderr: "".to_string(),
                exit_code: 0,
            },
            mcp_meta: None,
        })
    }
}
