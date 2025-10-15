use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;
use async_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "signalbus")]
#[command(about = "Lightweight local signal bus")]
struct Cli {
    #[command(subcommand)]
    command: Command,
} 

#[derive(Subcommand)]
enum Command {
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

struct DaemonState {
    subscribers: Mutex<HashMap<String, Vec<Sender<Signal>>>>,
}

impl DaemonState {
    fn new() -> Self {
        Self {
            subscribers: Mutex::new(HashMap::new()),
        }
    }

    async fn subscribe(&self, pattern: String, tx: Sender<Signal>) {
        let mut subs = self.subscribers.lock().await;
        subs.entry(pattern.clone()).or_insert_with(Vec::new).push(tx);
        println!("New subscriber for pattern: {}", pattern);
    }

    async fn publish(&self, signal: Signal) -> Result<()> {
        let subs = self.subscribers.lock().await;
        let mut matched = 0;
        
        for (pattern, clients) in subs.iter() {
            if pattern_match(&pattern, &signal.name) {
                matched += clients.len();
                for client in clients {
                    let _ = client.send(signal.clone()).await;
                }
            }
        }
        
        println!("Published signal '{}' to {} clients", signal.name, matched);
        Ok(())
    }
}

fn pattern_match(pattern: &str, signal_name: &str) -> bool {
    if pattern.ends_with(":*") {
        let prefix = &pattern[..pattern.len() - 2];
        signal_name.starts_with(prefix)
    } else if pattern == "*" {
        true
    } else {
        pattern == signal_name
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Signal {
    name: String,
    payload: Option<serde_json::Value>,
    timestamp: u64,
}

impl Signal {
    fn new(name: String, payload: Option<String>) -> anyhow::Result<Self> {
        let payload_value = match payload {
            Some(p) => Some(serde_json::from_str(&p)?),
            None => None,
        };
        
        Ok(Signal {
            name,
            payload: payload_value,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_secs(),
        })
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Command::Emit { signal, payload } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                emit_signal(signal, payload).await
            })?;
        }
        Command::Listen { pattern } => {
            tokio::runtime::Runtime::new()?.block_on(async {
                listen_signals(pattern).await
            })?;
        }
        Command::Daemon => {
            println!("Starting SignalBus daemon...");
            tokio::runtime::Runtime::new()?.block_on(async {
                run_daemon().await
            })?;
        }
    }
    
    Ok(())
}

async fn emit_signal(signal_name: String, payload: Option<String>) -> Result<()> {
    let signal = Signal::new(signal_name, payload)?;
    
    let mut stream = UnixStream::connect(SOCKET_PATH).await?;
    
    let json = serde_json::to_string(&signal)?;
    let message = format!("EMIT:{}\n", json);
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;
    
    println!("Signal emitted: {}", signal.name);
    Ok(())
}

async fn listen_signals(pattern: String) -> Result<()> {
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

const SOCKET_PATH: &str = "/tmp/signalbus.sock";

async fn run_daemon() -> Result<()> {
    let _ = fs::remove_file(SOCKET_PATH).await;
    
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("Daemon listening on {}", SOCKET_PATH);
    
    let state = Arc::new(DaemonState::new());
    
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, state).await {
                        eprintln!("Client error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }
}

async fn handle_client(stream: UnixStream, state: Arc<DaemonState>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    
    reader.read_line(&mut line).await?;
    let line = line.trim();
    
    if line.starts_with("LISTEN:") {
        let pattern = line.trim_start_matches("LISTEN:").to_string();
        let (tx, rx) = async_channel::bounded(100);
        
        state.subscribe(pattern, tx).await;
        
        while let Ok(signal) = rx.recv().await {
            let json = serde_json::to_string(&signal)?;
            writer.write_all(json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    } else if line.starts_with("EMIT:") {
        let json_str = line.trim_start_matches("EMIT:");
        match serde_json::from_str::<Signal>(json_str) {
            Ok(signal) => {
                state.publish(signal).await?;
            }
            Err(e) => eprintln!("Invalid signal JSON: {}", e),
        }
    }
    
    Ok(())
}
