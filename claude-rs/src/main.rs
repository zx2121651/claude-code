use clap::{Parser, Subcommand};
use anyhow::Result;

pub mod tools;

#[derive(Parser, Debug)]
#[command(name = "claude")]
#[command(about = "Claude Code - starts an interactive session by default", long_about = None)]
struct Cli {
    /// Your prompt
    #[arg(index = 1)]
    prompt: Option<String>,

    /// Print response and exit (useful for pipes)
    #[arg(short = 'p', long = "print")]
    print: bool,

    /// Minimal mode: skip hooks, LSP, plugin sync
    #[arg(long = "bare")]
    bare: bool,

    /// Enable debug mode
    #[arg(short = 'd', long = "debug")]
    debug: bool,

    /// Output format (only works with --print): "text" (default), "json", or "stream-json"
    #[arg(long = "output-format", default_value = "text")]
    output_format: String,

    /// Bypass all permission checks
    #[arg(long = "dangerously-skip-permissions")]
    dangerously_skip_permissions: bool,

    /// Model for the current session
    #[arg(long = "model")]
    model: Option<String>,

    /// Subcommands (e.g. ssh, mcp, plugin)
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Connect your local environment to claude.ai/code
    RemoteControl {
        /// Name for the session
        #[arg(long)]
        name: Option<String>,

        /// Spawn mode: same-dir, worktree, session
        #[arg(long)]
        spawn: Option<String>,
    },
    /// MCP Server Management
    Mcp {
        // Nested subcommands for MCP could go here
    }
}

#[tokio::main]
async fn main() -> Result<()> {
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

    if cli.print {
        println!("Headless print mode enabled (-p).");
        if let Some(ref prompt) = cli.prompt {
            println!("Sending prompt to QueryEngine: {}", prompt);
        } else {
            eprintln!("Error: A prompt is required for --print mode.");
        }
    } else {
        println!("Starting Interactive REPL UI...");
    }

    Ok(())
}
