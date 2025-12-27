use anyhow::Context;
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub api_key: String,
    pub universe_id: Option<u64>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        // Load .env file if it exists (for local development)
        let _ = dotenvy::dotenv();

        let api_key = env::var("ROBLOX_API_KEY")
            .context("ROBLOX_API_KEY environment variable not set")?;

        let universe_id = env::var("ROBLOX_UNIVERSE_ID")
            .ok()
            .and_then(|s| s.parse().ok());

        Ok(Self {
            api_key,
            universe_id,
        })
    }
}

