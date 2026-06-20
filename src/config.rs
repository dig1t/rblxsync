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
                    other => {
                        // Accept quoted numeric strings like "0" or "100"
                        if let Ok(num) = other.parse::<u64>() {
                            return self.visit_u64(num);
                        }
                        Err(de::Error::custom(format!(
                            "invalid private_server_cost: '{}'. Use 'disabled', 0 (free), or a positive number",
                            value
                        )))
                    }
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

        let api_key =
            env::var("ROBLOX_API_KEY").context("ROBLOX_API_KEY environment variable not set")?;

        let roblox_cookie = env::var("ROBLOX_COOKIE").ok();

        Ok(Self {
            api_key,
            roblox_cookie,
        })
    }
}

// --- YAML Configuration ---

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RblxSyncConfig {
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
    /// Output path for generating Luau config from the lock file after sync
    /// e.g. "Config.luau" or "src/shared/Config.luau"
    pub output_path: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub name: String,
    pub description: Option<String>,
    pub price: Option<u32>,
    pub icon: Option<String>,
    pub is_for_sale: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeveloperProductConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub name: String,
    pub description: Option<String>,
    pub price: u32,
    pub icon: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BadgeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
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

impl RblxSyncConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file at {:?}", path))?;
        let config: RblxSyncConfig =
            serde_yaml::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- PrivateServerCost deserialization ---

    #[test]
    fn test_psc_deserialize_disabled() {
        let v: PrivateServerCost = serde_yaml::from_str("\"disabled\"").unwrap();
        assert_eq!(v, PrivateServerCost::Disabled);
    }

    #[test]
    fn test_psc_deserialize_disabled_case_insensitive() {
        let v: PrivateServerCost = serde_yaml::from_str("\"Disabled\"").unwrap();
        assert_eq!(v, PrivateServerCost::Disabled);
    }

    #[test]
    fn test_psc_deserialize_free_string() {
        let v: PrivateServerCost = serde_yaml::from_str("\"free\"").unwrap();
        assert_eq!(v, PrivateServerCost::Free);
    }

    #[test]
    fn test_psc_deserialize_zero_is_free() {
        let v: PrivateServerCost = serde_yaml::from_str("0").unwrap();
        assert_eq!(v, PrivateServerCost::Free);
    }

    #[test]
    fn test_psc_deserialize_positive_is_paid() {
        let v: PrivateServerCost = serde_yaml::from_str("100").unwrap();
        assert_eq!(v, PrivateServerCost::Paid(100));
    }

    #[test]
    fn test_psc_deserialize_negative_errors() {
        let result: std::result::Result<PrivateServerCost, _> = serde_yaml::from_str("-5");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("negative"));
    }

    #[test]
    fn test_psc_deserialize_too_large_errors() {
        let too_large = (u32::MAX as u64) + 1;
        let result: std::result::Result<PrivateServerCost, _> =
            serde_yaml::from_str(&too_large.to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn test_psc_deserialize_quoted_numeric_strings() {
        let cost: PrivateServerCost = serde_yaml::from_str("\"0\"").unwrap();
        assert_eq!(cost, PrivateServerCost::Free);

        let cost: PrivateServerCost = serde_yaml::from_str("\"100\"").unwrap();
        assert_eq!(cost, PrivateServerCost::Paid(100));
    }

    #[test]
    fn test_psc_deserialize_invalid_string_errors() {
        let result: std::result::Result<PrivateServerCost, _> =
            serde_yaml::from_str("\"nonsense\"");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid private_server_cost"));
    }

    // --- PrivateServerCost serialization round-trip ---

    #[test]
    fn test_psc_serialize_round_trip() {
        for value in [
            PrivateServerCost::Disabled,
            PrivateServerCost::Free,
            PrivateServerCost::Paid(250),
        ] {
            let serialized = serde_yaml::to_string(&value).unwrap();
            let deserialized: PrivateServerCost = serde_yaml::from_str(&serialized).unwrap();
            assert_eq!(value, deserialized);
        }
    }

    #[test]
    fn test_psc_serialize_disabled_as_string() {
        assert_eq!(
            serde_yaml::to_string(&PrivateServerCost::Disabled)
                .unwrap()
                .trim(),
            "disabled"
        );
    }

    #[test]
    fn test_psc_serialize_free_as_zero() {
        assert_eq!(
            serde_yaml::to_string(&PrivateServerCost::Free)
                .unwrap()
                .trim(),
            "0"
        );
    }

    // --- RblxSyncConfig parsing ---

    fn full_config_yaml() -> &'static str {
        "assets_dir: assets/icons/\n\
         badge_payment_source: \"user\"\n\
         output_path: \"src/shared/Config.luau\"\n\
         creator:\n\
         \x20 id: \"12345678\"\n\
         \x20 type: \"user\"\n\
         universe:\n\
         \x20 id: 123456789\n\
         \x20 name: \"My Awesome Game\"\n\
         \x20 description: \"Updated via rblxsync!\"\n\
         \x20 genre: \"adventure\"\n\
         \x20 playable_devices: [\"computer\", \"phone\"]\n\
         \x20 max_players: 50\n\
         \x20 private_server_cost: \"disabled\"\n\
         game_passes:\n\
         \x20 - name: \"VIP Pass\"\n\
         \x20   price: 100\n\
         developer_products:\n\
         \x20 - name: \"Speed Boost\"\n\
         \x20   price: 50\n\
         badges:\n\
         \x20 - name: \"First Win\"\n\
         places:\n\
         \x20 - place_id: 1234567890\n\
         \x20   file_path: \"places/start_place.rbxl\"\n\
         \x20   publish: true\n"
    }

    #[test]
    fn test_config_parses_full_yaml() {
        let config: RblxSyncConfig = serde_yaml::from_str(full_config_yaml()).unwrap();
        assert_eq!(config.assets_dir, "assets/icons/");
        assert_eq!(config.universe.id, 123456789);
        assert_eq!(config.universe.name.as_deref(), Some("My Awesome Game"));
        assert_eq!(
            config.universe.private_server_cost,
            Some(PrivateServerCost::Disabled)
        );
        assert_eq!(config.game_passes.len(), 1);
        assert_eq!(config.game_passes[0].name, "VIP Pass");
        assert_eq!(config.developer_products.len(), 1);
        assert_eq!(config.badges.len(), 1);
        assert_eq!(config.places.len(), 1);
        assert!(config.places[0].publish);
        assert_eq!(config.creator.as_ref().unwrap().creator_type, "user");
    }

    #[test]
    fn test_config_default_assets_dir() {
        let yaml = "universe:\n  id: 42\n";
        let config: RblxSyncConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.assets_dir, "assets");
        assert!(config.game_passes.is_empty());
        assert!(config.developer_products.is_empty());
        assert!(config.badges.is_empty());
        assert!(config.places.is_empty());
    }

    #[test]
    fn test_config_missing_universe_id_errors() {
        let yaml = "universe:\n  name: \"No Id\"\n";
        let result: std::result::Result<RblxSyncConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }
}
