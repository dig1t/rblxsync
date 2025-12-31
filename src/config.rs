use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::env;
use std::fs;
use std::path::Path;

// --- Private Server Cost ---

/// Represents private server cost configuration
/// - `Disabled` = private servers are not allowed
/// - `Free` = private servers are free (cost 0)
/// - `Paid(u32)` = private servers cost the specified amount in Robux
#[derive(Debug, Clone, PartialEq)]
pub enum PrivateServerCost {
    Disabled,
    Free,
    Paid(u32),
}

impl<'de> Deserialize<'de> for PrivateServerCost {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        
        struct PrivateServerCostVisitor;
        
        impl<'de> Visitor<'de> for PrivateServerCostVisitor {
            type Value = PrivateServerCost;
            
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number (0 for free, positive for paid) or \"disabled\"")
            }
            
            fn visit_str<E>(self, value: &str) -> std::result::Result<PrivateServerCost, E>
            where
                E: de::Error,
            {
                match value.to_lowercase().as_str() {
                    "disabled" => Ok(PrivateServerCost::Disabled),
                    "free" => Ok(PrivateServerCost::Free),
                    _ => Err(de::Error::custom(format!(
                        "invalid private_server_cost: '{}'. Use 'disabled', 0 (free), or a positive number",
                        value
                    ))),
                }
            }
            
            fn visit_u64<E>(self, value: u64) -> std::result::Result<PrivateServerCost, E>
            where
                E: de::Error,
            {
                if value == 0 {
                    Ok(PrivateServerCost::Free)
                } else if value <= u32::MAX as u64 {
                    Ok(PrivateServerCost::Paid(value as u32))
                } else {
                    Err(de::Error::custom("private_server_cost too large"))
                }
            }
            
            fn visit_i64<E>(self, value: i64) -> std::result::Result<PrivateServerCost, E>
            where
                E: de::Error,
            {
                if value < 0 {
                    Err(de::Error::custom("private_server_cost cannot be negative"))
                } else {
                    self.visit_u64(value as u64)
                }
            }
        }
        
        deserializer.deserialize_any(PrivateServerCostVisitor)
    }
}

impl Serialize for PrivateServerCost {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            PrivateServerCost::Disabled => serializer.serialize_str("disabled"),
            PrivateServerCost::Free => serializer.serialize_u32(0),
            PrivateServerCost::Paid(cost) => serializer.serialize_u32(*cost),
        }
    }
}

// --- Environment Configuration ---

#[derive(Clone, Debug)]
pub struct Config {
    pub api_key: String,
    /// .ROBLOSECURITY cookie for develop.roblox.com API (required for universe settings)
    pub roblox_cookie: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let api_key = env::var("ROBLOX_API_KEY")
            .context("ROBLOX_API_KEY environment variable not set")?;

        let roblox_cookie = env::var("ROBLOX_COOKIE").ok();

        Ok(Self {
            api_key,
            roblox_cookie,
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
    /// Universe ID (required)
    pub id: u64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub playable_devices: Option<Vec<String>>,
    pub max_players: Option<u32>,
    /// Private server cost: "disabled", 0 (free), or a positive number (Robux cost)
    pub private_server_cost: Option<PrivateServerCost>,
}

impl UniverseConfig {
    /// Check if any universe settings are defined
    pub fn has_settings(&self) -> bool {
        self.name.is_some() 
            || self.description.is_some() 
            || self.genre.is_some() 
            || self.playable_devices.is_some() 
            || self.max_players.is_some()
            || self.private_server_cost.is_some()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GamePassConfig {
    pub name: String,
    pub description: Option<String>,
    pub price: Option<u32>,
    pub icon: Option<String>,
    pub is_for_sale: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeveloperProductConfig {
    pub name: String,
    pub description: Option<String>,
    pub price: u32,
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
