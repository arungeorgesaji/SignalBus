use crate::models::{Signal, PersistentSignal, pattern_match};
use anyhow::Result;
use async_channel::Sender;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

pub const SOCKET_PATH: &str = "/tmp/signalbus.sock";

pub struct DaemonState {
    subscribers: Mutex<HashMap<String, Vec<Sender<Signal>>>>,
    signal_history: Mutex<VecDeque<PersistentSignal>>,
    max_history_size: usize,
    next_id: AtomicU64,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            subscribers: Mutex::new(HashMap::new()),
            signal_history: Mutex::new(VecDeque::new()),
            max_history_size: 1000, 
            next_id: AtomicU64::new(1),
        }
    }

    pub async fn subscribe(&self, pattern: String, tx: Sender<Signal>) {
        let mut subs = self.subscribers.lock().await;
        subs.entry(pattern.clone()).or_insert_with(Vec::new).push(tx);
        println!("New subscriber for pattern: {}", pattern);
    }

    pub async fn publish(&self, signal: Signal, ttl: Option<u64>) -> Result<()> {
        self.add_to_history(signal.clone(), ttl).await;
        
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
        
        println!("Published signal '{}' to {} clients (TTL: {:?})", signal.name, matched, ttl);
        Ok(())
    }

    pub async fn add_to_history(&self, signal: Signal, ttl: Option<u64>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let persistent_signal = PersistentSignal { signal, id, ttl };
        
        let mut history = self.signal_history.lock().await;
        history.push_back(persistent_signal);
        
        while history.len() > self.max_history_size {
            history.pop_front();
        }
        
        id
    }
    
    pub async fn get_recent_signals(&self, pattern: &str, limit: usize) -> Vec<PersistentSignal> {
        let history = self.signal_history.lock().await;
        history.iter()
            .rev()
            .filter(|ps| pattern_match(pattern, &ps.signal.name))
            .take(limit)
            .cloned()
            .collect()
    }

    pub async fn cleanup_expired(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let mut history = self.signal_history.lock().await;
        history.retain(|ps| {
            if let Some(ttl) = ps.ttl {
                ps.signal.timestamp + ttl > now
            } else {
                true 
            }
        });
        
        println!("Cleanup completed, {} signals in history", history.len());
    }
}

pub async fn run_daemon() -> Result<()> {
    let _ = fs::remove_file(SOCKET_PATH).await;
    
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("Daemon listening on {}", SOCKET_PATH);
    
    let state = Arc::new(DaemonState::new());
    
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        start_cleanup_task(cleanup_state).await;
    });
    
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
    } else if line.starts_with("EMIT|") {
        let rest = line.trim_start_matches("EMIT|");
    
        let parts: Vec<&str> = rest.split('|').collect();
        
        match serde_json::from_str::<Signal>(parts[0]) {
            Ok(signal) => {
                let ttl = if parts.len() == 2 {
                    parts[1].parse().ok()  
                } else {
                    None  
                };
                
                state.publish(signal, ttl).await?;
                let _ = writer.write_all(b"OK\n").await;
            }
            Err(e) => {
                eprintln!("Invalid signal JSON: {}", e);
                let _ = writer.write_all(format!("ERROR:{}\n", e).as_bytes()).await;
            }
        }
    } else if line.starts_with("HISTORY:") {
        let rest = line.trim_start_matches("HISTORY:");
        
        let parts: Vec<&str> = rest.splitn(2, '|').collect();
        println!("HISTORY parts: {:?}", parts);
        
        if parts.len() == 2 {
            let pattern = parts[0];
            let limit_str = parts[1];
            
            let limit = limit_str.parse().unwrap_or(10);
            
            println!("Fetching history for pattern '{}', limit: {}", pattern, limit);
            
            let signals = state.get_recent_signals(pattern, limit).await;
            println!("Found {} matching signals", signals.len());
            
            match serde_json::to_string(&signals) {
                Ok(json) => {
                    println!("Sending JSON response: {}", json);
                    if let Err(e) = writer.write_all(json.as_bytes()).await {
                        eprintln!("Write error: {}", e);
                    }
                    if let Err(e) = writer.write_all(b"\n").await {
                        eprintln!("Write error: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("JSON serialization error: {}", e);
                    let _ = writer.write_all(b"[]\n").await;
                }
            }
        } else {
            eprintln!("Invalid HISTORY command format: {}", line);
            let _ = writer.write_all(b"[]\n").await;
        }
    } 
    
    Ok(())
}

async fn start_cleanup_task(state: Arc<DaemonState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60)); 
    loop {
        interval.tick().await;
        state.cleanup_expired().await;
    }
}
