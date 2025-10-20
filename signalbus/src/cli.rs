use crate::daemon::SOCKET_PATH;
use crate::models::{Signal, PersistentSignal};
use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command as TokioCommand;
use std::process::Stdio;
use std::fs;

pub const TOKEN_FILE: &str = ".signalbus_token";

#[derive(Parser)]
#[command(name = "signalbus")]
#[command(about = "Lightweight local signal bus")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Emit {
        signal: String,
        #[arg(long)]
        payload: Option<String>,
        #[arg(long)]
        ttl: Option<u64>,
        #[arg(long)]
        token: Option<String>,
    },
    Listen {
        pattern: String,
        #[arg(long)]
        exec: Option<String>,
        #[arg(long)]
        token: Option<String>,
    },
    Daemon,
    History {
        pattern: String,
        #[arg(short, long, default_value = "10")]
        limit: usize,
        #[arg(long)]
        token: Option<String>,
    },
    RateLimit {
        pattern: String,
        max_signals: u32,
        #[arg(long)]
        per_seconds: u64,
        #[arg(long)]
        token: Option<String>,
    },
    ShowRateLimits {
        #[arg(long)]
        token: Option<String>,
    },
    Login {
        #[arg(short, long)]
        user_id: String,
        #[arg(short, long)]
        password: String,
    },
    Logout,
    CreateToken {
        #[arg(short, long)]
        user_id: String,
        #[arg(short, long)]
        permissions: Vec<String>,
        #[arg(long)]
        expires_in: Option<u64>, 
    },
    RevokeToken {
        token: String,
        #[arg(long)]
        admin_token: Option<String>,
    },
}

pub async fn login(user_id: String, password: String) -> Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let command = format!("LOGIN|{}|{}\n", user_id, password);
    println!("Sending: {}", command.trim());  
    stream.write_all(command.as_bytes()).await?;
    stream.flush().await?;
    
    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    println!("Waiting for response...");
    reader.read_line(&mut response).await?;
    
    let response = response.trim();
    println!("Got: {}", response);  
    
    if response.starts_with("TOKEN:") {
        let token = response.trim_start_matches("TOKEN:");
        save_token(token)?;
        println!("Login successful! Token saved to ~/.signalbus_token");
        println!("Token: {}", token);  
    } else {
        eprintln!("Login failed: {}", response);
    }
    
    Ok(())
}

pub async fn create_token(user_id: String, permissions: Vec<String>, expires_in: Option<u64>) -> Result<()> {
    let token = load_token().ok_or_else(|| anyhow::anyhow!("Not logged in"))?;
    
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let perms_str = permissions.join(",");
    let command = if let Some(expires) = expires_in {
        format!("CREATE_TOKEN|{}|{}|{}|{}\n", token, user_id, perms_str, expires)
    } else {
        format!("CREATE_TOKEN|{}|{}|{}\n", token, user_id, perms_str)
    };
    
    stream.write_all(command.as_bytes()).await?;
    stream.flush().await?;
    
    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    
    println!("{}", response.trim());
    Ok(())
}

fn save_token(token: &str) -> Result<()> {
    let mut path = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    path.push(TOKEN_FILE);
    fs::write(path, token)?;
    Ok(())
}

pub fn load_token() -> Option<String> {
    let mut path = dirs::home_dir()?;
    path.push(TOKEN_FILE);
    fs::read_to_string(path).ok()
}

pub async fn revoke_token(token: String, admin_token: Option<String>) -> Result<()> {
    let auth_token = admin_token.or_else(load_token).ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let command = format!("REVOKE_TOKEN|{}|{}\n", auth_token, token);
    stream.write_all(command.as_bytes()).await?;
    stream.flush().await?;
    
    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    
    let response = response.trim();
    if response == "OK" {
        println!("Token revoked successfully");
    } else if response.starts_with("ERROR:") {
        eprintln!("Failed to revoke token: {}", response);
    } else {
        println!("{}", response);
    }
    
    Ok(())
}

pub async fn emit_signal(signal_name: String, payload: Option<String>, ttl: Option<u64>, token: Option<String>) -> Result<()> {
    let auth_token = token.or_else(load_token).ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    let signal = Signal::new(signal_name, payload)?;
    
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let emit_command = if let Some(ttl_secs) = ttl {
        format!("EMIT|{}|{}|{}\n", auth_token, serde_json::to_string(&signal)?, ttl_secs)
    } else {
        format!("EMIT|{}|{}\n", auth_token, serde_json::to_string(&signal)?)
    };
    
    stream.write_all(emit_command.as_bytes()).await?;
    stream.flush().await?;
    
    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    let response = response.trim();
    
    if response == "OK" {
        println!("Signal emitted: {}", signal.name);
        if let Some(ttl_secs) = ttl {
            println!("TTL: {} seconds", ttl_secs);
        }
        Ok(())
    } else if response.starts_with("ERROR:") {
        Err(anyhow::anyhow!("Failed to emit signal: {}", response))
    } else {
        println!("Signal emitted: {}", signal.name);
        if let Some(ttl_secs) = ttl {
            println!("TTL: {} seconds", ttl_secs);
        }
        Ok(())
    }
}

pub async fn listen_signals(pattern: String, exec_cmd: Option<String>, token: Option<String>) -> Result<()> {
    let auth_token = token.or_else(load_token).ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    println!("Listening for pattern: {}", pattern);
    if let Some(cmd) = &exec_cmd {
        println!("Will execute: {}", cmd);
    }
    
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let message = format!("LISTEN|{}|{}\n", auth_token, pattern);
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;
    
    let mut reader = BufReader::new(&mut stream);
    
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                println!("Daemon disconnected");
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if !line.is_empty() {
                    match serde_json::from_str::<Signal>(line) {
                        Ok(signal) => {
                            println!("Received signal: {}", signal.name);
                            if let Some(payload) = &signal.payload {
                                println!("   Payload: {}", payload);
                            }
                            println!("   Timestamp: {}", signal.timestamp);
                            
                            if let Some(cmd) = &exec_cmd {
                                if let Err(e) = execute_command(cmd, &signal).await {
                                    eprintln!("Error executing command: {}", e);
                                }
                            }
                            println!("---");
                        }
                        Err(e) => eprintln!("Invalid signal: {}", e),
                    }
                }
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}

async fn execute_command(cmd: &str, signal: &Signal) -> Result<()> {
    println!("Executing: {}", cmd);
    
    let mut command = TokioCommand::new("sh");
    command
        .arg("-c")
        .arg(cmd)  
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("SIGNALBUS_SIGNAL", &signal.name)
        .env("SIGNALBUS_TIMESTAMP", signal.timestamp.to_string());
    
    if let Some(payload) = &signal.payload {
        command.env("SIGNALBUS_PAYLOAD", payload.to_string());
    } else {
        command.env("SIGNALBUS_PAYLOAD", "null");
    }
    
    let mut child = command.spawn()?;
    let status = child.wait().await?;
    
    if status.success() {
        println!("Command executed successfully");
    } else {
        eprintln!("Command failed with exit code: {}", status);
    }
    
    Ok(())
}

pub async fn show_history(pattern: String, limit: usize, token: Option<String>) -> Result<()> {
    let auth_token = token.or_else(load_token).ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    println!("Connecting to daemon...");
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    println!("Connected to daemon");
    
    let message = format!("HISTORY|{}|{}|{}\n", auth_token, pattern, limit);
    println!("Sending: {}", message.trim());
    
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;
    
    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    
    println!("Waiting for response...");
    match reader.read_line(&mut response).await {
        Ok(0) => {
            println!("Daemon closed connection unexpectedly");
            return Ok(());
        }
        Ok(_) => {
            let response = response.trim();
            println!("Raw response: '{}'", response);
            
            if response.is_empty() {
                println!("No history data received");
                return Ok(());
            }
            
            match serde_json::from_str::<Vec<PersistentSignal>>(response) {
                Ok(signals) => {
                    if signals.is_empty() {
                        println!("No recent signals matching '{}'", pattern);
                    } else {
                        println!("Recent signals matching '{}':", pattern);
                        for ps in signals {
                            println!("ID: {} | Signal: {} | Timestamp: {}", 
                                ps.id, ps.signal.name, ps.signal.timestamp);
                            if let Some(payload) = &ps.signal.payload {
                                println!("   Payload: {}", payload);
                            }
                            if let Some(ttl) = ps.ttl {
                                println!("   TTL: {}s", ttl);
                            }
                            println!("---");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error parsing history: {}", e);
                    eprintln!("Raw response was: '{}'", response);
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading response: {}", e);
        }
    }
    
    Ok(())
}

pub async fn set_rate_limit(pattern: String, max_signals: u32, per_seconds: u64, token: Option<String>) -> Result<()> {
    let auth_token = token.or_else(load_token).ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let command = format!("RATE_LIMIT|{}|{}|{}|{}\n", auth_token, pattern, max_signals, per_seconds);
    stream.write_all(command.as_bytes()).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    
    println!("{}", response.trim());
    Ok(())
}

pub async fn show_rate_limits(token: Option<String>) -> Result<()> {
    let auth_token = token.or_else(load_token).ok_or_else(|| anyhow::anyhow!("Not authenticated"))?;

    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let command = format!("SHOW_RATE_LIMITS|{}\n", auth_token);
    stream.write_all(command.as_bytes()).await?;
    stream.flush().await?;
    
    let mut reader = BufReader::new(&mut stream);
    let mut response = String::new();
    
    loop {
        response.clear();
        match reader.read_line(&mut response).await {
            Ok(0) => break, 
            Ok(_) => {
                let line = response.trim();
                if !line.is_empty() {
                    println!("{}", line);
                }
            }
            Err(e) => {
                eprintln!("Error reading response: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}
