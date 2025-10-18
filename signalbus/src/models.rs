use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Signal {
    pub name: String,
    pub payload: Option<serde_json::Value>,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PersistentSignal {
    pub signal: Signal,
    pub id: u64,
    pub ttl: Option<u64>, 
}

impl Signal {
    pub fn new(name: String, payload: Option<String>) -> anyhow::Result<Self> {
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

pub fn pattern_match(pattern: &str, signal_name: &str) -> bool {
    if pattern.ends_with(":*") {
        let prefix = &pattern[..pattern.len() - 2];
        signal_name.starts_with(prefix)
    } else if pattern == "*" {
        true
    } else {
        pattern == signal_name
    }
}
