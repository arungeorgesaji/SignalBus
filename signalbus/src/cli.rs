use crate::daemon::SOCKET_PATH;
use crate::models::Signal;
use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

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

pub async fn listen_signals(pattern: String) -> Result<()> {
    println!("Listening for pattern: {}", pattern);
    
    let stream = UnixStream::connect(SOCKET_PATH).await?;
    let (reader, _) = stream.into_split();
    let mut reader = BufReader::new(reader);
    
    let message = format!("LISTEN:{}\n", pattern);
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;
    
    drop(stream);
    
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).await?;
        let line = line.trim();
        
        if !line.is_empty() {
            match serde_json::from_str::<Signal>(line) {
                Ok(signal) => {
                    println!("Received signal: {}", signal.name);
                    if let Some(payload) = signal.payload {
                        println!("   Payload: {}", payload);
                    }
                    println!("   Timestamp: {}", signal.timestamp);
                    println!("---");
                }
                Err(e) => eprintln!("Invalid signal: {}", e),
            }
        }
    }
}
