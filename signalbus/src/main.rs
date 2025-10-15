mod cli;
mod daemon;
mod models;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    
    match cli.command {
        cli::Command::Emit { signal, payload } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::emit_signal(signal, payload).await
            })?;
        }
        cli::Command::Listen { pattern, exec } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::listen_signals(pattern, exec).await
            })?;
        }
        cli::Command::Daemon => {
            println!("Starting SignalBus daemon...");
            tokio::runtime::Runtime::new()?.block_on(async {
                daemon::run_daemon().await
            })?;
        }
    }
    
    Ok(())
}
