use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;

/// Represents a JSON-RPC request for the MCP Protocol
#[derive(Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

/// Represents a JSON-RPC response from the MCP server
#[derive(Deserialize, Debug)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<Value>,
}

/// A lightweight Model Context Protocol (MCP) Client managing Stdio communication.
pub struct McpClient {
    pub name: String,
    child: Child,
    request_id_counter: u64,
    // Note: In a robust implementation, we would spawn a background reader task
    // and use oneshot channels to correlate responses with requests by ID.
    // For this skeleton, we represent the concept.
}

impl McpClient {
    pub async fn spawn(name: &str, command: &str, args: &[&str]) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()) // Capture to avoid polluting the UI
            .spawn()?;

        Ok(Self {
            name: name.to_string(),
            child,
            request_id_counter: 1,
        })
    }

    pub async fn initialize(&mut self) -> Result<Value> {
        self.call_method("initialize", serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "claude-rs",
                "version": "0.1.0"
            }
        })).await
    }

    /// Makes a synchronous JSON-RPC call over Stdio.
    /// (Warning: This simplistic implementation reads lines directly. A full implementation
    /// needs a dedicated reader task to handle interleaved messages).
    pub async fn call_method(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.request_id_counter;
        self.request_id_counter += 1;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let req_json = serde_json::to_string(&request)?;

        let stdin = self.child.stdin.as_mut().ok_or_else(|| anyhow!("Failed to get stdin"))?;
        stdin.write_all(req_json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        let stdout = self.child.stdout.as_mut().ok_or_else(|| anyhow!("Failed to get stdout"))?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        reader.read_line(&mut line).await?;

        if line.is_empty() {
             return Err(anyhow!("MCP Server disconnected unexpectedly"));
        }

        let response: JsonRpcResponse = serde_json::from_str(&line)?;

        if let Some(err) = response.error {
            return Err(anyhow!("MCP Error: {:?}", err));
        }

        if response.id != Some(id) {
             return Err(anyhow!("MCP Error: Response ID mismatch. Expected {}, got {:?}", id, response.id));
        }

        response.result.ok_or_else(|| anyhow!("MCP Error: Missing result field"))
    }
}
