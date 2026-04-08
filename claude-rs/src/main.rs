use clap::{Parser, Subcommand};
use anyhow::Result;
use dotenvy::dotenv;

pub mod api;
pub mod engine;
pub mod tools;
pub mod tui;
pub mod coordinator;
pub mod mcp;

use engine::QueryEngine;
use tools::{BashTool, AgentTool, FileReadTool, FileEditTool, GlobTool};
use tui::run_tui;

#[derive(Parser, Debug)]
#[command(name = "claude")]
#[command(about = "Claude Code - starts an interactive session by default", long_about = None)]
struct Cli {
    #[arg(index = 1)]
    prompt: Option<String>,

    #[arg(short = 'p', long = "print")]
    print: bool,

    #[arg(long = "bare")]
    bare: bool,

    #[arg(short = 'd', long = "debug")]
    debug: bool,

    #[arg(long = "output-format", default_value = "text")]
    output_format: String,

    #[arg(long = "dangerously-skip-permissions")]
    dangerously_skip_permissions: bool,

    #[arg(long = "model")]
    model: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    RemoteControl {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        spawn: Option<String>,
    },
    Mcp {}
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if it exists
    let _ = dotenv();

    let cli = Cli::parse();

    if cli.debug {
        println!("[DEBUG] Debug mode enabled.");
    }

    match &cli.command {
        Some(Commands::RemoteControl { name, spawn }) => {
            println!("Starting Remote Control Bridge...");
            if let Some(n) = name {
                println!("Session name: {}", n);
            }
            if let Some(s) = spawn {
                println!("Spawn mode: {}", s);
            }
            return Ok(());
        }
        Some(Commands::Mcp { .. }) => {
            println!("MCP Management...");
            return Ok(());
        }
        None => {}
    }

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| "".to_string());

    if cli.print {
        if let Some(ref prompt) = cli.prompt {
            if api_key.is_empty() {
                eprintln!("Error: ANTHROPIC_API_KEY is not set.");
                return Ok(());
            }

            println!("Sending prompt to QueryEngine: {}", prompt);

            let mut engine = QueryEngine::new(
                api_key,
                cli.model.unwrap_or_else(|| "claude-3-7-sonnet-20250219".to_string()),
                Some("You are Claude Code, an AI assistant.")
            );

            engine.register_tool(Box::new(BashTool));
            engine.register_tool(Box::new(AgentTool {}));
            engine.register_tool(Box::new(FileReadTool));
            engine.register_tool(Box::new(FileEditTool));
            engine.register_tool(Box::new(GlobTool));

            engine.submit_message(prompt, None).await?;
        } else {
            eprintln!("Error: A prompt is required for --print mode.");
        }
    } else {
        if api_key.is_empty() {
            eprintln!("Error: ANTHROPIC_API_KEY is not set. Please set it in your environment or .env file before launching the interactive session.");
            return Ok(());
        }
        run_tui(api_key, cli.model.unwrap_or_else(|| "claude-3-7-sonnet-20250219".to_string())).await?;
    }

    Ok(())
}
