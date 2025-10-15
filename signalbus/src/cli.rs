use crate::daemon::SOCKET_PATH;
use crate::models::Signal;
use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command as TokioCommand;
use std::process::Stdio;

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
    },
    Listen {
        pattern: String,
        #[arg(long)]
        exec: Option<String>,
    },
    Daemon,
}

pub async fn emit_signal(signal_name: String, payload: Option<String>) -> Result<()> {
    let signal = Signal::new(signal_name, payload)?;
    
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let json = serde_json::to_string(&signal)?;
    let message = format!("EMIT:{}\n", json);
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;
    
    println!("Signal emitted: {}", signal.name);
    Ok(())
}

pub async fn listen_signals(pattern: String, exec_cmd: Option<String>) -> Result<()> {
    println!("Listening for pattern: {}", pattern);
    if let Some(cmd) = &exec_cmd {
        println!("Will execute: {}", cmd);
    }
    
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let message = format!("LISTEN:{}\n", pattern);
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;
    
    let (reader, _writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    
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

