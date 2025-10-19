mod cli;
mod daemon;
mod models;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    
    match cli.command {
        cli::Command::Emit { signal, payload, ttl } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::emit_signal(signal, payload, ttl).await
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
        cli::Command::History { pattern, limit } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::show_history(pattern, limit).await
            })?;
        }
        cli::Command::RateLimit { pattern, max_signals, per_seconds } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::set_rate_limit(pattern, max_signals, per_seconds).await
            })?;
        }
        cli::Command::ShowRateLimits => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::show_rate_limits().await
            })?;
        }
    }
    
    Ok(())
}
