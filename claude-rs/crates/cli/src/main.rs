use clap::{Parser, Subcommand};
use anyhow::Result;
use dotenvy::dotenv;

pub mod engine;
pub mod tui;
pub mod coordinator;
pub mod mcp;
pub mod config;

use engine::QueryEngine;
use claude_tools::{agent::AgentTool, bash::BashTool, file_edit::FileEditTool, file_read::FileReadTool, glob::GlobTool};
use tui::run_tui;
use config::ConfigManager;

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
    let _ = dotenv();
    let cli = Cli::parse();

    if cli.debug {
        println!("[DEBUG] Debug mode enabled.");
    }

    match &cli.command {
        Some(Commands::RemoteControl { name: _, spawn: _ }) => {
            println!("Starting Remote Control Bridge...");
            return Ok(());
        }
        Some(Commands::Mcp { .. }) => {
            println!("MCP Management...");
            return Ok(());
        }
        None => {}
    }

    // 1. Load cascading configurations
    let mut config_mgr = ConfigManager::new().await?;
    // Override settings with CLI flags
    if let Some(ref m) = cli.model {
        config_mgr.active_settings.model = Some(m.clone());
    }

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| "".to_string());

    // Resolve effective model
    let effective_model = config_mgr.active_settings.model.unwrap_or_else(|| "claude-3-7-sonnet-20250219".to_string());
    let effective_sys_prompt = config_mgr.active_settings.custom_system_prompt.unwrap_or_else(|| "You are Claude Code, an AI assistant.".to_string());

    if cli.print {
        if let Some(ref prompt) = cli.prompt {
            if api_key.is_empty() {
                eprintln!("Error: ANTHROPIC_API_KEY is not set.");
                return Ok(());
            }

            println!("Sending prompt to QueryEngine: {}", prompt);

            let mut engine = QueryEngine::new(
                api_key,
                effective_model,
                Some(&effective_sys_prompt)
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
        run_tui(api_key, effective_model, effective_sys_prompt).await?;
    }

    Ok(())
}
