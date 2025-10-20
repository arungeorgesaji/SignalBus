use crate::models::{Signal, PersistentSignal, pattern_match, Permission, AuthToken};
use anyhow::Result;
use async_channel::Sender;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use rand::{Rng, rng};

pub const SOCKET_PATH: &str = "/tmp/signalbus.sock";

#[derive(Clone)]
struct RateLimitRule {
    max_signals: u32,
    time_window: Duration,
}

#[derive(Clone)]
#[allow(dead_code)]
struct User {
    pub user_id: String,
    pub password_hash: String, 
    pub permissions: HashSet<Permission>,
}

pub struct DaemonState {
    subscribers: Mutex<HashMap<String, Vec<Sender<Signal>>>>,
    signal_history: Mutex<VecDeque<PersistentSignal>>,
    max_history_size: usize,
    next_id: AtomicU64,
    rate_limits: Mutex<HashMap<String, RateLimitRule>>,
    signal_counters: Mutex<HashMap<String, VecDeque<Instant>>>,
    users: Mutex<HashMap<String, User>>, 
    auth_tokens: Mutex<HashMap<String, AuthToken>>, 
    default_tokens: Mutex<HashMap<String, String>>
}

impl DaemonState {
    pub async fn new() -> Arc<Self> {
        println!("[DAEMON] Creating new DaemonState..."); 

        let state = Arc::new(Self {
            subscribers: Mutex::new(HashMap::new()),
            signal_history: Mutex::new(VecDeque::new()),
            max_history_size: 1000,
            next_id: AtomicU64::new(1),
            rate_limits: Mutex::new(HashMap::new()),
            signal_counters: Mutex::new(HashMap::new()),
            users: Mutex::new(HashMap::new()),
            auth_tokens: Mutex::new(HashMap::new()),
            default_tokens: Mutex::new(HashMap::new()),
        });
            
        println!("[DAEMON] DaemonState created, initializing default users...");
        state.initialize_default_users().await;
        println!("[DAEMON] Default users initialized successfully");

        state
    }

    async fn initialize_default_users(&self) {
        println!("[DAEMON] Starting initialize_default_users...");
        
        let user_id = "admin".to_string();
        {
            let mut users = self.users.lock().await;
            println!("[DAEMON] Acquired users lock");
            
            let admin_perms: HashSet<Permission> = [
                Permission::Read,
                Permission::Write, 
                Permission::History,
                Permission::RateLimit,
                Permission::Admin,
            ].iter().cloned().collect();
            
            println!("[DAEMON] Creating admin user...");
            users.insert(user_id.clone(), User {
                user_id: user_id.clone(),
                password_hash: "admin123".to_string(), 
                permissions: admin_perms,
            });
            
        }
        
        println!("[DAEMON] Generating admin token...");
        let token = self.generate_token(user_id, None).await;
        
        {
            let mut default_tokens = self.default_tokens.lock().await;
            println!("[DAEMON] Acquired default_tokens lock");
            default_tokens.insert("admin".to_string(), token);
        }
        
        println!("[DAEMON] initialize_default_users completed");
    }
    
    pub async fn add_user(&self, user_id: String, password_hash: String, permissions: HashSet<Permission>) {
        let mut users = self.users.lock().await;
        users.insert(user_id.clone(), User {
            user_id,
            password_hash,
            permissions,
        });
    }
    
    pub async fn authenticate(&self, token: &str, required_permission: Option<Permission>) -> bool {
        let tokens = self.auth_tokens.lock().await;
        
        if let Some(auth_token) = tokens.get(token) {
            if let Some(expires_at) = auth_token.expires_at {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                if now > expires_at {
                    return false;
                }
            }
            
            if let Some(required_perm) = required_permission {
                auth_token.permissions.contains(&required_perm) || 
                auth_token.permissions.contains(&Permission::Admin)
            } else {
                true
            }
        } else {
            false
        }
    }

    pub async fn generate_token(&self, user_id: String, expires_in: Option<u64>) -> String {
        let token: String = {
            let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
            let mut rng = rng();
            (0..32).map(|_| chars[rng.random_range(0..chars.len())]).collect()
        };
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let expires_at = expires_in.map(|seconds| now + seconds);
        
        let auth_token = AuthToken {
            token: token.clone(),
            user_id: user_id.clone(),
            permissions: self.get_user_permissions(&user_id).await,
            created_at: now,
            expires_at,
        };
        
        let mut tokens = self.auth_tokens.lock().await;
        tokens.insert(token.clone(), auth_token);
        
        token
    }

    async fn get_user_permissions(&self, user_id: &str) -> HashSet<Permission> {
        let users = self.users.lock().await;
        users.get(user_id)
            .map(|user| user.permissions.clone())
            .unwrap_or_else(|| {
                let mut perms = HashSet::new();
                perms.insert(Permission::Read);
                perms.insert(Permission::Write);
                perms
            })
    }

    pub async fn revoke_token(&self, token_to_revoke: &str) -> bool {
        let mut tokens = self.auth_tokens.lock().await;
        tokens.remove(token_to_revoke).is_some()
    }
    
    pub async fn login(&self, user_id: &str, password: &str) -> Option<String> {
        let maybe_user = {
            let users = self.users.lock().await;
            users.get(user_id).cloned()
        };

        if let Some(user) = maybe_user {
            if user.password_hash == password {
                return Some(self.generate_token(user_id.to_string(), Some(3600)).await);
            }
        }
        None
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
    
    let state = DaemonState::new().await;
    
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        start_cleanup_task(cleanup_state).await;
    });

    println!("Daemon is ready to accept connections...");

    loop {
        println!("Waiting for client connection...");
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("New client connected! Address: {:?}", addr);
                let state = state.clone();
                tokio::spawn(async move {
                    println!("Spawning new task to handle client");
                    if let Err(e) = handle_client(stream, state).await {
                        eprintln!("Client error: {}", e);
                    }
                    println!("Client handling task completed");
                });
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_client(mut stream: UnixStream, state: Arc<DaemonState>) -> Result<()> {
    println!("Daemon: New client connected");

    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    
    reader.read_line(&mut line).await?;
    let line = line.trim();

    println!("Daemon: Received command: {}", line);

    if line.starts_with("LOGIN|") {
        let rest = line.trim_start_matches("LOGIN|");
        let parts: Vec<&str> = rest.splitn(2, '|').collect();
        if parts.len() == 2 {
            let user_id = parts[0];
            let password = parts[1];
            
            println!("Daemon: Processing login for {}", user_id);  
            
            if let Some(token) = state.login(user_id, password).await {
                println!("Daemon: Login SUCCESS, token generated");  
                let response = format!("TOKEN:{}\n", token);
                if let Err(e) = stream.write_all(response.as_bytes()).await {
                    eprintln!("Write error: {}", e);
                }
                if let Err(e) = stream.flush().await {
                    eprintln!("Flush error: {}", e);
                }
                println!("Daemon: Response SENT: {}", response.trim());  
            } else {
                println!("Daemon: Login FAILED");  
                let _ = stream.write_all(b"ERROR:Invalid credentials\n").await;
                let _ = stream.flush().await;
            }
        }
    }
    else if line.starts_with("CREATE_TOKEN|") {
        let rest = line.trim_start_matches("CREATE_TOKEN|");
        let parts: Vec<&str> = rest.splitn(4, '|').collect();
        
        if parts.len() >= 3 {
            let token = parts[0];
            let user_id = parts[1];
            let permissions_str = parts[2];
            let expires_in = parts.get(3).and_then(|s| s.parse().ok());
            
            if state.authenticate(token, Some(Permission::Admin)).await {
                let permissions: Vec<String> = permissions_str.split(',').map(|s| s.to_string()).collect();
                
                let mut perms = HashSet::new();
                for perm_str in permissions {
                    match perm_str.as_str() {
                        "Read" => perms.insert(Permission::Read),
                        "Write" => perms.insert(Permission::Write),
                        "History" => perms.insert(Permission::History),
                        "RateLimit" => perms.insert(Permission::RateLimit),
                        "Admin" => perms.insert(Permission::Admin),
                        _ => continue,
                    };
                }
                
                state.add_user(user_id.to_string(), "default_password".to_string(), perms).await;
                
                let new_token = state.generate_token(user_id.to_string(), expires_in).await;
                let _ = stream.write_all(format!("New token created: {}\n", new_token).as_bytes()).await;
            } else {
                let _ = stream.write_all(b"ERROR:Authentication failed or insufficient permissions\n").await;
            }
        } else {
            let _ = stream.write_all(b"ERROR:Invalid CREATE_TOKEN format\n").await;
        }
    }
    else if line.starts_with("EMIT|") {
        let rest = line.trim_start_matches("EMIT|");
        let parts: Vec<&str> = rest.splitn(3, '|').collect();  
        if parts.len() >= 2 {
            let token = parts[0];
            let signal_json = parts[1];
            let ttl = parts.get(2).and_then(|s| s.parse().ok());
            
            if state.authenticate(token, Some(Permission::Write)).await {
                match serde_json::from_str::<Signal>(signal_json) {
                    Ok(signal) => {
                        match state.publish(signal, ttl).await {
                            Ok(_) => {
                                let _ = stream.write_all(b"OK\n").await;
                            }
                            Err(e) => {
                                let error_msg = format!("ERROR:{}\n", e);
                                let _ = stream.write_all(error_msg.as_bytes()).await;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Invalid signal JSON: {}", e);
                        let _ = stream.write_all(format!("ERROR:{}\n", e).as_bytes()).await;
                    }
                }
            } else {
                let _ = stream.write_all(b"ERROR:Authentication failed or insufficient permissions\n").await;
            }
        } else {
            let _ = stream.write_all(b"ERROR:Invalid EMIT format\n").await;
        }
    }
    else if line.starts_with("LISTEN|") {
        let rest = line.trim_start_matches("LISTEN|");
        let parts: Vec<&str> = rest.splitn(2, '|').collect(); 
        if parts.len() == 2 {
            let token = parts[0];
            let pattern = parts[1].to_string();
            
            if state.authenticate(token, Some(Permission::Read)).await {
                let (tx, rx) = async_channel::bounded(100);
                state.subscribe(pattern.clone(), tx).await;
                
                let _ = stream.write_all(b"LISTENING\n").await;
                let _ = stream.flush().await;
                
                while let Ok(signal) = rx.recv().await {
                    let json = serde_json::to_string(&signal)?;
                    stream.write_all(json.as_bytes()).await?;
                    stream.write_all(b"\n").await?;
                    stream.flush().await?;
                }
            } else {
                let _ = stream.write_all(b"ERROR:Authentication failed or insufficient permissions\n").await;
            }
        } else {
            let _ = stream.write_all(b"ERROR:Invalid LISTEN format\n").await;
        }
    }
    else if line.starts_with("HISTORY|") {
        let rest = line.trim_start_matches("HISTORY|");
        let parts: Vec<&str> = rest.splitn(3, '|').collect(); 
        if parts.len() == 3 {
            let token = parts[0];
            let pattern = parts[1];
            let limit_str = parts[2];
            
            if state.authenticate(token, Some(Permission::History)).await {
                let limit = limit_str.parse().unwrap_or(10);
                let signals = state.get_recent_signals(pattern, limit).await;
                
                match serde_json::to_string(&signals) {
                    Ok(json) => {
                        if let Err(e) = stream.write_all(json.as_bytes()).await {
                            eprintln!("Write error: {}", e);
                        }
                        if let Err(e) = stream.write_all(b"\n").await {
                            eprintln!("Write error: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("JSON serialization error: {}", e);
                        let _ = stream.write_all(b"[]\n").await;
                    }
                }
            } else {
                let _ = stream.write_all(b"ERROR:Authentication failed or insufficient permissions\n").await;
            }
        } else {
            let _ = stream.write_all(b"ERROR:Invalid HISTORY format\n").await;
        }
    }
    else if line.starts_with("RATE_LIMIT|") {
        let rest = line.trim_start_matches("RATE_LIMIT|");
        let parts: Vec<&str> = rest.splitn(4, '|').collect(); 
        if parts.len() == 4 {
            let token = parts[0];
            let pattern = parts[1];
            let max_signals: u32 = parts[2].parse()?;
            let per_seconds: u64 = parts[3].parse()?;
            
            if state.authenticate(token, Some(Permission::RateLimit)).await {
                state.set_rate_limit(pattern.to_string(), max_signals, per_seconds).await;
                stream.write_all(b"Rate limit configured successfully\n").await?;
            } else {
                stream.write_all(b"ERROR:Authentication failed or insufficient permissions\n").await?;
            }
        } else {
            stream.write_all(b"ERROR:Invalid RATE_LIMIT command format\n").await?;
        }
    }
    else if line.starts_with("SHOW_RATE_LIMITS|") {
        let rest = line.trim_start_matches("SHOW_RATE_LIMITS|");
        let token = rest; 
        
        if state.authenticate(token, Some(Permission::Read)).await {
            let limits = state.rate_limits.lock().await;
            if limits.is_empty() {
                let _ = stream.write_all(b"No rate limits configured\n").await;
            } else {
                let mut response = String::new();
                response.push_str("Configured rate limits:\n");
                for (pattern, rule) in limits.iter() {
                    response.push_str(&format!(
                        "  {}: {} signals per {} seconds\n",
                        pattern, rule.max_signals, rule.time_window.as_secs()
                    ));
                }
                let _ = stream.write_all(response.as_bytes()).await;
            }
        } else {
            let _ = stream.write_all(b"ERROR:Authentication failed or insufficient permissions\n").await;
        }
        let _ = stream.flush().await;
    } 
    else if line.starts_with("REVOKE_TOKEN|") {
        let rest = line.trim_start_matches("REVOKE_TOKEN|");
        let parts: Vec<&str> = rest.splitn(2, '|').collect();
        
        if parts.len() == 2 {
            let admin_token = parts[0];
            let token_to_revoke = parts[1];
            
            if state.authenticate(admin_token, Some(Permission::Admin)).await {
                if state.revoke_token(token_to_revoke).await {
                    let _ = stream.write_all(b"OK\n").await;
                } else {
                    let _ = stream.write_all(b"ERROR:Token not found\n").await;
                }
            } else {
                let _ = stream.write_all(b"ERROR:Authentication failed or insufficient permissions\n").await;
            }
        } else {
            let _ = stream.write_all(b"ERROR:Invalid REVOKE_TOKEN format\n").await;
        }
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
