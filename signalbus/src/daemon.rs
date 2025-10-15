use crate::models::{Signal, pattern_match};
use anyhow::Result;
use async_channel::Sender;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

pub const SOCKET_PATH: &str = "/tmp/signalbus.sock";

pub struct DaemonState {
    subscribers: Mutex<HashMap<String, Vec<Sender<Signal>>>>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            subscribers: Mutex::new(HashMap::new()),
        }
    }

    pub async fn subscribe(&self, pattern: String, tx: Sender<Signal>) {
        let mut subs = self.subscribers.lock().await;
        subs.entry(pattern.clone()).or_insert_with(Vec::new).push(tx);
        println!("New subscriber for pattern: {}", pattern);
    }

    pub async fn publish(&self, signal: Signal) -> Result<()> {
        let subs = self.subscribers.lock().await;
        let mut matched = 0;
        
        for (pattern, clients) in subs.iter() {
            if pattern_match(pattern, &signal.name) {
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

pub async fn run_daemon() -> Result<()> {
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
