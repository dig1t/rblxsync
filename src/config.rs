use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

// --- Environment Configuration ---

#[derive(Clone, Debug)]
pub struct Config {
    pub api_key: String,
    pub universe_id: Option<u64>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
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

// --- YAML Configuration ---

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RbxSyncConfig {
    #[serde(default = "default_assets_dir")]
    pub assets_dir: String,
    pub creator: Option<CreatorConfig>,
    pub universe: UniverseConfig,
    #[serde(default)]
    pub game_passes: Vec<GamePassConfig>,
    #[serde(default)]
    pub developer_products: Vec<DeveloperProductConfig>,
    #[serde(default)]
    pub badges: Vec<BadgeConfig>,
    #[serde(default)]
    pub places: Vec<PlaceConfig>,
    /// Payment source type for badge creation (costs 100 Robux per badge)
    /// Valid values: "user" (pay from user funds) or "group" (pay from group funds)
    pub badge_payment_source: Option<String>,
}

fn default_assets_dir() -> String {
    "assets".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreatorConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub creator_type: String, // "user" or "group"
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UniverseConfig {
    pub name: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub playable_devices: Option<Vec<String>>,
    pub max_players: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GamePassConfig {
    pub name: String,
    pub description: Option<String>,
    pub price_in_robux: Option<u32>,
    pub icon: Option<String>,
    pub is_for_sale: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeveloperProductConfig {
    pub name: String,
    pub description: Option<String>,
    pub price_in_robux: u32,
    pub icon: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BadgeConfig {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PlaceConfig {
    pub place_id: u64,
    pub file_path: String,
    #[serde(default)]
    pub publish: bool,
}

impl RbxSyncConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file at {:?}", path))?;
        let config: RbxSyncConfig = serde_yaml::from_str(&content)
            .context("Failed to parse config file")?;
        Ok(config)
    }
}
