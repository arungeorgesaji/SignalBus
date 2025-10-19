use crate::models::{Signal, PersistentSignal, pattern_match};
use anyhow::Result;
use async_channel::Sender;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

pub const SOCKET_PATH: &str = "/tmp/signalbus.sock";

#[derive(Clone)]
struct RateLimitRule {
    max_signals: u32,
    time_window: Duration,
}

pub struct DaemonState {
    subscribers: Mutex<HashMap<String, Vec<Sender<Signal>>>>,
    signal_history: Mutex<VecDeque<PersistentSignal>>,
    max_history_size: usize,
    next_id: AtomicU64,
    rate_limits: Mutex<HashMap<String, RateLimitRule>>,
    signal_counters: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            subscribers: Mutex::new(HashMap::new()),
            signal_history: Mutex::new(VecDeque::new()),
            max_history_size: 1000,
            next_id: AtomicU64::new(1),
            rate_limits: Mutex::new(HashMap::new()),
            signal_counters: Mutex::new(HashMap::new()),
        }
    }

    pub async fn subscribe(&self, pattern: String, tx: Sender<Signal>) {
        let mut subs = self.subscribers.lock().await;
        subs.entry(pattern.clone()).or_insert_with(Vec::new).push(tx);
        println!("New subscriber for pattern: {}", pattern);
    }

    pub async fn publish(&self, signal: Signal, ttl: Option<u64>) -> Result<()> {
        if !self.check_rate_limit(&signal.name).await {
            return Err(anyhow::anyhow!(
                "Rate limit exceeded for signal: {}", 
                signal.name
            ));
        }

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

    pub async fn set_rate_limit(&self, pattern: String, max_signals: u32, time_window_secs: u64) {
        let rule = RateLimitRule {
            max_signals,
            time_window: Duration::from_secs(time_window_secs),
        };
        
        let mut limits = self.rate_limits.lock().await;
        limits.insert(pattern.clone(), rule);
        println!("Rate limit set: {} signals per {} seconds for pattern '{}'", 
                 max_signals, time_window_secs, pattern);
    }

    pub async fn check_rate_limit(&self, signal_name: &str) -> bool {
        let limits = self.rate_limits.lock().await;
        let mut counters = self.signal_counters.lock().await;
        
        let now = Instant::now();
        
        for (pattern, rule) in limits.iter() {
            if pattern_match(pattern, signal_name) {
                let counter = counters.entry(pattern.clone()).or_insert_with(VecDeque::new);
                
                while let Some(front) = counter.front() {
                    if now.duration_since(*front) > rule.time_window {
                        counter.pop_front();
                    } else {
                        break;
                    }
                }
                
                if counter.len() >= rule.max_signals as usize {
                    println!("Rate limit exceeded for pattern '{}': {} signals in {} seconds", 
                             pattern, counter.len(), rule.time_window.as_secs());
                    return false;
                }
                
                counter.push_back(now);
                return true;
            }
        }
        
        true
    }

    pub async fn cleanup_rate_limit_counters(&self) {
        let limits = self.rate_limits.lock().await;
        let mut counters = self.signal_counters.lock().await;
        let now = Instant::now();
        
        for (pattern, rule) in limits.iter() {
            if let Some(counter) = counters.get_mut(pattern) {
                counter.retain(|&timestamp| now.duration_since(timestamp) <= rule.time_window);
            }
        }
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
    
    if line.starts_with("LISTEN|") {
        let pattern = line.trim_start_matches("LISTEN|").to_string();
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
    } else if line.starts_with("HISTORY|") {
        let rest = line.trim_start_matches("HISTORY|");
        
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
    } else if line.starts_with("RATE_LIMIT|") {
        let rest = line.trim_start_matches("RATE_LIMIT|");
        let parts: Vec<&str> = rest.split('|').collect();
        
        if parts.len() == 3 {
            let pattern = parts[0];
            let max_signals: u32 = parts[1].parse()?;
            let per_seconds: u64 = parts[2].parse()?;
            
            state.set_rate_limit(pattern.to_string(), max_signals, per_seconds).await;
            writer.write_all(b"Rate limit configured successfully\n").await?;
        } else {
            writer.write_all(b"ERROR: Invalid RATE_LIMIT command format\n").await?;
        }
    }
    else if line == "SHOW_RATE_LIMITS" {
        writer.write_all(b"Rate limits feature active\n").await?;
    } 
    
    Ok(())
}

async fn start_cleanup_task(state: Arc<DaemonState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60)); 
    loop {
        interval.tick().await;
        state.cleanup_expired().await;
        state.cleanup_rate_limit_counters().await;
    }
}
