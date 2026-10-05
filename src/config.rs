use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml::Value;
use std::collections::BTreeMap;
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
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let api_key =
            env::var("ROBLOX_API_KEY").context("ROBLOX_API_KEY environment variable not set")?;

        Ok(Self { api_key })
    }
}

// --- YAML Configuration ---

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RblxSyncConfig {
    #[serde(default = "default_assets_dir")]
    pub assets_dir: String,
    /// Ignored: icons go up with the pass or product they belong to, so
    /// there's no asset to own. Parsed only so `validate` can warn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<Value>,
    pub universe: UniverseConfig,
    /// Other universes the same entries sync to (a dev or staging copy of the
    /// game), by name. Each one overrides top-level keys for `--env <name>`;
    /// see [`RblxSyncConfig::resolve`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environments: BTreeMap<String, Value>,
    /// The environment this config was resolved for (`--env`), `None` for the
    /// top level. Set by [`RblxSyncConfig::resolve`], never read from YAML.
    #[serde(skip)]
    pub env: Option<String>,
    #[serde(default)]
    pub game_passes: Vec<GamePassConfig>,
    #[serde(default)]
    pub developer_products: Vec<DeveloperProductConfig>,
    #[serde(default)]
    pub badges: Vec<BadgeConfig>,
    #[serde(default)]
    pub places: Vec<PlaceConfig>,
    #[serde(default, skip_serializing_if = "ThumbnailsConfig::is_empty")]
    pub thumbnails: ThumbnailsConfig,
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
pub struct UniverseConfig {
    /// Universe ID (required)
    pub id: u64,
    pub name: Option<String>,
    pub description: Option<String>,
    /// Ignored: Roblox lets a genre change only once every 3 months, so it's
    /// set in the Creator Hub. Parsed only so `validate` can warn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<Value>,
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
            || self.playable_devices.is_some()
            || self.max_players.is_some()
            || self.private_server_cost.is_some()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GamePassConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// This pass's id in each environment's universe (`id` is the top level's).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ids: BTreeMap<String, u64>,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ids: BTreeMap<String, u64>,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ids: BTreeMap<String, u64>,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PlaceConfig {
    /// The place in the top-level universe. Optional, so a place that so far
    /// exists only in an environment can be listed by its `ids` alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_id: Option<u64>,
    /// The same place in each environment's universe.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ids: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Most players in one server of this place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_size: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub publish: bool,
}

impl PlaceConfig {
    /// How log lines name this place: its name, else its id.
    pub fn label(&self) -> String {
        match (&self.name, self.place_id) {
            (Some(name), _) => format!("'{}'", name),
            (None, Some(id)) => id.to_string(),
            (None, None) => "<no id>".to_string(),
        }
    }

    /// Whether `run` has any setting of this place to sync.
    pub fn has_settings(&self) -> bool {
        self.name.is_some() || self.description.is_some() || self.server_size.is_some()
    }
}

/// Top-level keys an environment can't override: entries are shared by every
/// environment, which keeps its own id for each one in the entry's `ids`.
const SHARED_KEYS: [&str; 5] = [
    "game_passes",
    "developer_products",
    "badges",
    "places",
    "environments",
];

/// Checks one `environments.<name>` overlay and returns its universe id. The
/// overlay must name its own universe: one inherited from the top level would
/// point a dev run at the live game.
pub fn check_environment(name: &str, overlay: &Value, top_universe_id: u64) -> Result<u64> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "environments.{}: use only letters, digits, '-' and '_' in an environment's name (it names the lock file)",
            name
        );
    }
    let overlay = overlay.as_mapping().ok_or_else(|| {
        anyhow!(
            "environments.{}: expected the top-level keys this environment overrides, such as `universe: {{ id: ... }}`",
            name
        )
    })?;
    if let Some(key) = SHARED_KEYS
        .iter()
        .find(|key| overlay.contains_key(Value::from(**key)))
    {
        bail!(
            "environments.{}.{}: entries are shared by every environment, so they can't be listed per environment. Give each top-level entry its id in this environment with `ids: {{ {}: <id> }}`",
            name,
            key,
            name
        );
    }
    let id = overlay
        .get("universe")
        .and_then(|universe| universe.get("id"))
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("environments.{}.universe.id is required", name))?;
    if id == top_universe_id {
        bail!(
            "environments.{}.universe.id is {}, the top-level universe. Each environment needs its own universe",
            name,
            id
        );
    }
    Ok(id)
}

/// Lays `overlay` over `base`: mappings merge key by key, `null` removes the
/// key (so the field falls back to its default, whatever its type), and
/// anything else replaces what was there.
fn merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Mapping(base), Value::Mapping(overlay)) => {
            for (key, value) in overlay {
                if value.is_null() {
                    base.remove(key);
                    continue;
                }
                match base.get_mut(key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

/// Experience thumbnails. An empty list leaves that set alone on Roblox.
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ThumbnailsConfig {
    /// The game page carousel, in display order (up to 10).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub game_page: Vec<ThumbnailConfig>,
    /// The home page tiles (up to 5). Two or more turn on personalization.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub home_page: Vec<ThumbnailConfig>,
}

impl ThumbnailsConfig {
    pub fn is_empty(&self) -> bool {
        self.game_page.is_empty() && self.home_page.is_empty()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ThumbnailConfig {
    /// Image file, relative to `assets_dir`.
    pub image: String,
    /// Game page only; home page thumbnails have no alt text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
}

impl RblxSyncConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file at {:?}", path))?;
        let config: RblxSyncConfig =
            serde_yaml::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }

    /// The config as environment `env` sees it: `environments.<env>` laid over
    /// the top-level keys (an explicit `null` clears one), and each entry's
    /// `ids.<env>` as its id. Entries with no id there are matched by name in
    /// that universe, like entries without an `id` at the top level. The
    /// environment's `name_prefix` goes in front of the experience's and each
    /// place's name, so a dev copy reads "[dev] Lobby" in Studio. `None`
    /// returns the top level unchanged.
    pub fn resolve(self, env: Option<&str>) -> Result<Self> {
        let Some(env) = env else {
            return Ok(self);
        };
        let overlay = self.environments.get(env).ok_or_else(|| {
            let known: Vec<&str> = self.environments.keys().map(String::as_str).collect();
            anyhow!(
                "No environment '{}' in the config (defined: {}). Leave out --env to sync the top-level universe {}",
                env,
                if known.is_empty() { "none".to_string() } else { known.join(", ") },
                self.universe.id
            )
        })?;
        let universe_id = check_environment(env, overlay, self.universe.id)?;
        // Two environments on one universe would each take the other's
        // uploads for someone else's and never delete them.
        let shared = self.environments.iter().find(|(name, other)| {
            name.as_str() != env
                && other
                    .get("universe")
                    .and_then(|u| u.get("id"))
                    .and_then(Value::as_u64)
                    == Some(universe_id)
        });
        if let Some((other, _)) = shared {
            bail!(
                "environments.{} and environments.{} both use universe {}",
                other,
                env,
                universe_id
            );
        }
        let mut overlay = overlay.clone();
        let prefix = match overlay
            .as_mapping_mut()
            .and_then(|o| o.remove("name_prefix"))
        {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(prefix)) => prefix,
            Some(_) => bail!("environments.{}.name_prefix must be text", env),
        };

        let mut merged = serde_yaml::to_value(&self).context("Failed to read the config back")?;
        merge(&mut merged, &overlay);
        let mut resolved: RblxSyncConfig = serde_yaml::from_value(merged)
            .with_context(|| format!("Failed to parse environments.{}", env))?;

        let names = (resolved.universe.name.iter_mut())
            .chain(resolved.places.iter_mut().filter_map(|p| p.name.as_mut()));
        for name in names {
            name.insert_str(0, &prefix);
        }

        let id_in = |ids: &BTreeMap<String, u64>| ids.get(env).copied();
        for pass in &mut resolved.game_passes {
            pass.id = id_in(&pass.ids);
        }
        for product in &mut resolved.developer_products {
            product.id = id_in(&product.ids);
        }
        for badge in &mut resolved.badges {
            badge.id = id_in(&badge.ids);
        }
        for place in &mut resolved.places {
            place.place_id = id_in(&place.ids);
        }
        resolved.env = Some(env.to_string());
        Ok(resolved)
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
        assert!(config.creator.is_some());
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

    // --- environments ---

    fn env_config(environments: &str) -> RblxSyncConfig {
        let yaml = format!(
            "universe:\n  id: 1\n  name: Live Game\n  description: Live text\n  private_server_cost: 100\n\
             output_path: Config.luau\n\
             environments:\n{}\
             game_passes:\n  - name: VIP\n    id: 10\n    ids: {{ dev: 20 }}\n    price: 100\n  - name: Fresh\n    id: 11\n\
             developer_products:\n  - name: Coins\n    ids: {{ dev: 30 }}\n    price: 5\n\
             badges:\n  - name: Win\n    id: 12\n    ids:\n      dev: 40\n\
             places:\n  - place_id: 50\n    ids: {{ dev: 60 }}\n    name: Lobby\n  - ids: {{ dev: 61 }}\n    name: Dev only\n",
            environments
        );
        serde_yaml::from_str(&yaml).unwrap()
    }

    const DEV: &str =
        "  dev:\n    universe:\n      id: 2\n      name: null\n    output_path: ConfigDev.luau\n";

    #[test]
    fn test_resolve_without_env_is_the_top_level() {
        let config = env_config(DEV).resolve(None).unwrap();
        assert_eq!(config.env, None);
        assert_eq!(config.universe.id, 1);
        assert_eq!(config.game_passes[0].id, Some(10));
        assert_eq!(config.places[1].place_id, None);
    }

    #[test]
    fn test_resolve_env_overrides_keys_and_swaps_ids() {
        let config = env_config(DEV).resolve(Some("dev")).unwrap();
        assert_eq!(config.env.as_deref(), Some("dev"));
        assert_eq!(config.universe.id, 2);
        // `null` stops managing a field; keys the overlay leaves out carry over.
        assert_eq!(config.universe.name, None);
        assert_eq!(config.universe.description.as_deref(), Some("Live text"));
        assert_eq!(
            config.universe.private_server_cost,
            Some(PrivateServerCost::Paid(100))
        );
        assert_eq!(config.output_path.as_deref(), Some("ConfigDev.luau"));

        // Each entry's id becomes its dev id; with none it matches by name.
        assert_eq!(config.game_passes[0].id, Some(20));
        assert_eq!(config.game_passes[1].id, None);
        assert_eq!(config.developer_products[0].id, Some(30));
        assert_eq!(config.badges[0].id, Some(40));
        assert_eq!(config.places[0].place_id, Some(60));
        assert_eq!(config.places[1].place_id, Some(61));
        // The shared fields don't change.
        assert_eq!(config.game_passes[0].name, "VIP");
        assert_eq!(config.game_passes[0].price, Some(100));
    }

    #[test]
    fn test_resolve_null_falls_back_to_the_default() {
        let mut config = env_config(
            "  dev:\n    universe:\n      id: 2\n    assets_dir: null\n    thumbnails: null\n",
        );
        config.assets_dir = "icons".to_string();
        config.thumbnails.game_page = vec![ThumbnailConfig {
            image: "a.png".to_string(),
            alt_text: None,
        }];
        let config = config.resolve(Some("dev")).unwrap();
        assert_eq!(config.assets_dir, "assets");
        assert!(config.thumbnails.is_empty());
    }

    #[test]
    fn test_resolve_name_prefix_marks_the_copy_but_not_items() {
        let dev = "  dev:\n    universe:\n      id: 2\n    name_prefix: \"[dev] \"\n";
        let config = env_config(dev).resolve(Some("dev")).unwrap();
        assert_eq!(config.universe.name.as_deref(), Some("[dev] Live Game"));
        assert_eq!(config.places[0].name.as_deref(), Some("[dev] Lobby"));
        // Game code looks items up by name, so they keep theirs.
        assert_eq!(config.game_passes[0].name, "VIP");

        let bad = "  dev:\n    universe:\n      id: 2\n    name_prefix: 5\n";
        let err = env_config(bad).resolve(Some("dev")).unwrap_err();
        assert!(err.to_string().contains("name_prefix"), "{}", err);
    }

    #[test]
    fn test_resolve_unknown_env_names_the_defined_ones() {
        let err = env_config(DEV).resolve(Some("prod")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("'prod'") && msg.contains("dev"), "{}", msg);
    }

    #[test]
    fn test_resolve_refuses_unsafe_environments() {
        for (overlay, expected) in [
            (
                "  dev:\n    output_path: x.luau\n",
                "universe.id is required",
            ),
            ("  dev:\n    universe:\n      id: 1\n", "top-level universe"),
            (
                "  dev:\n    universe:\n      id: 2\n    game_passes: []\n",
                "can't be listed per environment",
            ),
            ("  ../dev:\n    universe:\n      id: 2\n", "letters, digits"),
            ("  dev: 2\n", "top-level keys"),
            (
                "  dev:\n    universe:\n      id: 2\n  staging:\n    universe:\n      id: 2\n",
                "both use universe 2",
            ),
        ] {
            let config = env_config(overlay);
            let env = config.environments.keys().next().unwrap().clone();
            let err = config.resolve(Some(&env)).unwrap_err().to_string();
            assert!(err.contains(expected), "{} -> {}", overlay, err);
        }
    }

    #[test]
    fn test_places_parse_settings_and_skip_empty_fields() {
        let yaml =
            "universe:\n  id: 1\nplaces:\n  - place_id: 5\n    name: Lobby\n    server_size: 30\n";
        let config: RblxSyncConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.places[0].name.as_deref(), Some("Lobby"));
        assert_eq!(config.places[0].server_size, Some(30));
        assert!(!config.places[0].publish);

        let written = serde_yaml::to_string(&config.places).unwrap();
        assert!(!written.contains("file_path"), "{}", written);
        assert!(!written.contains("publish"), "{}", written);
        assert!(!written.contains("ids"), "{}", written);
    }
}
