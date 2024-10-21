use std::{collections::HashSet, env};

use color_eyre::Result;
use once_cell::sync::Lazy;

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config::new_from_env().unwrap());

#[derive(Debug)]
pub struct Config {
    pub worker_name: String,
    pub worker_topics: Vec<String>,
    pub log_level: String,
    pub heartbeat_interval_sec: u64,
}
impl Config {
    pub fn new_from_env() -> Result<Self, anyhow::Error> {
        let mut worker_topics: Vec<String> = env::var("WORKER_TOPICS")
            .unwrap_or_else(|_| "all".to_string())
            .split(',')
            .map(|s| s.to_string())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        // Ensure "all" is included in the vector
        if !worker_topics.contains(&"all".to_string()) {
            worker_topics.push("all".to_string());
        }

        Ok(Self {
            worker_name: env::var("WORKER_NAME").expect("WORKER_NAME is not set"),
            worker_topics,
            heartbeat_interval_sec: env::var("HEARTBEAT_INTERVAL_SEC")
                .unwrap_or_else(|_| "5".to_string())
                .parse::<u64>()
                .expect("Invalid HEARTBEAT_INTERVAL value"),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        })
    }
}
