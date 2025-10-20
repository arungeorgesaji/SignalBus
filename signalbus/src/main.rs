mod cli;
mod daemon;
mod models;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    
    match cli.command {
        cli::Command::Emit { signal, payload, ttl, token } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::emit_signal(signal, payload, ttl, token).await
            })?;
        }
        cli::Command::Listen { pattern, exec, token } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::listen_signals(pattern, exec, token).await
            })?;
        }
        cli::Command::Daemon => {
            println!("Starting SignalBus daemon...");
            tokio::runtime::Runtime::new()?.block_on(async {
                daemon::run_daemon().await
            })?;
        }
        cli::Command::History { pattern, limit, token } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::show_history(pattern, limit, token).await
            })?;
        }
        cli::Command::RateLimit { pattern, max_signals, per_seconds, token } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::set_rate_limit(pattern, max_signals, per_seconds, token).await
            })?;
        }
        cli::Command::ShowRateLimits { token } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::show_rate_limits(token).await
            })?;
        }
        cli::Command::Login { user_id, password } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::login(user_id, password).await
            })?;
        }
        cli::Command::Logout => {
            let mut path = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
            path.push(cli::TOKEN_FILE);
            if path.exists() {
                std::fs::remove_file(&path)?;
                println!("Logged out successfully");
            } else {
                println!("Not logged in");
            }
        }
        cli::Command::CreateToken { user_id, permissions, expires_in } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::create_token(user_id, permissions, expires_in).await
            })?;
        }
        cli::Command::RevokeToken { token, admin_token } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                cli::revoke_token(token, admin_token).await
            })?;
        }
    }
    
    Ok(())
}
