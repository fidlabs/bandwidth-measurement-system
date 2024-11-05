use std::env;

use color_eyre::Result;
use once_cell::sync::Lazy;

use crate::types::DbConnectParams;

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config::new_from_env().unwrap());

#[derive(Debug)]
pub struct Config {
    pub db_url: String,
    pub log_level: String,
    pub auth_token: String,
    pub local_mode: String,
}
impl Config {
    pub fn new_from_env() -> Result<Self> {
        // Initialize database connection pool & run migrations
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            let json_params = env::var("DB_CONNECT_PARAMS_JSON")
                .expect("DB_CONNECT_PARAMS_JSON environment variable not set");

            let params: DbConnectParams =
                serde_json::from_str(&json_params).expect("Invalid JSON in DB_CONNECT_PARAMS_JSON");

            params.to_url()
        });

        Ok(Self {
            db_url,
            log_level: env::var("LOG_LEVEL").unwrap_or("info".to_string()),
            auth_token: env::var("AUTH_TOKEN")
                .unwrap_or("mysecrettokenthatdefinatelyisnotongithubpublicrepo".to_string()),
            local_mode: env::var("LOCAL_MODE").unwrap_or("false".to_string()),
        })
    }
}
