use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct SyncState {
    /// Universe settings state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub universe: Option<UniverseState>,
    /// Game passes keyed by their Roblox ID
    #[serde(default)]
    pub game_passes: HashMap<u64, ResourceState>,
    /// Developer products keyed by their Roblox ID
    #[serde(default)]
    pub developer_products: HashMap<u64, ResourceState>,
    /// Badges keyed by their Roblox ID
    #[serde(default)]
    pub badges: HashMap<u64, ResourceState>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct UniverseState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playable_devices: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_players: Option<u32>,
    /// Private server cost state: None = not set, Some("disabled") = disabled, Some("0") = free, Some("X") = paid
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_server_cost: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResourceState {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_for_sale: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_asset_id: Option<u64>,
}

impl SyncState {
    pub fn load(project_root: &Path) -> Result<Self> {
        let state_path = Self::get_state_path(project_root);
        if !state_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&state_path)?;
        let state: SyncState = serde_yaml::from_str(&content)?;
        Ok(state)
    }

    pub fn save(&self, project_root: &Path) -> Result<()> {
        let state_path = Self::get_state_path(project_root);
        if let Some(parent) = state_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_yaml::to_string(self)?;
        crate::fsutil::atomic_write(&state_path, content.as_bytes())?;
        Ok(())
    }

    fn get_state_path(project_root: &Path) -> PathBuf {
        project_root.join("rblxsync-lock.yml")
    }

    /// Find a game pass by name (case-insensitive) and return (id, state)
    pub fn find_game_pass_by_name(&self, name: &str) -> Option<(u64, &ResourceState)> {
        self.game_passes
            .iter()
            .find(|(_, state)| state.name.to_lowercase() == name.to_lowercase())
            .map(|(id, state)| (*id, state))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_game_pass(
        &mut self,
        id: u64,
        name: String,
        description: Option<String>,
        price: Option<u64>,
        is_for_sale: Option<bool>,
        icon_hash: Option<String>,
        icon_asset_id: Option<u64>,
    ) {
        self.game_passes.insert(
            id,
            ResourceState {
                name,
                description,
                price,
                is_for_sale,
                is_enabled: None,
                icon_hash,
                icon_asset_id,
            },
        );
    }

    /// Find a developer product by name (case-insensitive) and return (id, state)
    pub fn find_developer_product_by_name(&self, name: &str) -> Option<(u64, &ResourceState)> {
        self.developer_products
            .iter()
            .find(|(_, state)| state.name.to_lowercase() == name.to_lowercase())
            .map(|(id, state)| (*id, state))
    }

    pub fn update_developer_product(
        &mut self,
        id: u64,
        name: String,
        description: Option<String>,
        price: Option<u64>,
        icon_hash: Option<String>,
        icon_asset_id: Option<u64>,
    ) {
        self.developer_products.insert(
            id,
            ResourceState {
                name,
                description,
                price,
                is_for_sale: None,
                is_enabled: None,
                icon_hash,
                icon_asset_id,
            },
        );
    }

    /// Find a badge by name (case-insensitive) and return (id, state)
    pub fn find_badge_by_name(&self, name: &str) -> Option<(u64, &ResourceState)> {
        self.badges
            .iter()
            .find(|(_, state)| state.name.to_lowercase() == name.to_lowercase())
            .map(|(id, state)| (*id, state))
    }

    pub fn update_badge(
        &mut self,
        id: u64,
        name: String,
        description: Option<String>,
        is_enabled: Option<bool>,
        icon_hash: Option<String>,
        icon_asset_id: Option<u64>,
    ) {
        self.badges.insert(
            id,
            ResourceState {
                name,
                description,
                price: None,
                is_for_sale: None,
                is_enabled,
                icon_hash,
                icon_asset_id,
            },
        );
    }

    pub fn update_universe(
        &mut self,
        name: Option<String>,
        description: Option<String>,
        genre: Option<String>,
        playable_devices: Option<Vec<String>>,
        max_players: Option<u32>,
        private_server_cost: Option<String>,
    ) {
        self.universe = Some(UniverseState {
            name,
            description,
            genre,
            playable_devices,
            max_players,
            private_server_cost,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_returns_default_when_missing() {
        let dir = tempdir().unwrap();
        let state = SyncState::load(dir.path()).unwrap();
        assert!(state.universe.is_none());
        assert!(state.game_passes.is_empty());
        assert!(state.developer_products.is_empty());
        assert!(state.badges.is_empty());
    }

    #[test]
    fn test_save_load_round_trip() {
        let dir = tempdir().unwrap();
        let mut state = SyncState::default();
        state.update_universe(
            Some("Game".to_string()),
            Some("Desc".to_string()),
            None,
            Some(vec!["computer".to_string()]),
            Some(30),
            Some("disabled".to_string()),
        );
        state.update_game_pass(
            1,
            "VIP".to_string(),
            Some("d".to_string()),
            Some(100),
            Some(true),
            Some("hash".to_string()),
            Some(999),
        );
        state.update_developer_product(2, "Boost".to_string(), None, Some(50), None, None);
        state.update_badge(3, "Win".to_string(), None, Some(true), None, None);

        state.save(dir.path()).unwrap();
        assert!(dir.path().join("rblxsync-lock.yml").exists());

        let loaded = SyncState::load(dir.path()).unwrap();
        assert_eq!(loaded.universe, state.universe);
        assert_eq!(loaded.game_passes.get(&1).unwrap().name, "VIP");
        assert_eq!(loaded.game_passes.get(&1).unwrap().price, Some(100));
        assert_eq!(loaded.developer_products.get(&2).unwrap().name, "Boost");
        assert_eq!(loaded.badges.get(&3).unwrap().is_enabled, Some(true));
    }

    #[test]
    fn test_find_game_pass_by_name_case_insensitive() {
        let mut state = SyncState::default();
        state.update_game_pass(5, "Super VIP".to_string(), None, None, None, None, None);
        let found = state.find_game_pass_by_name("super vip");
        assert!(found.is_some());
        assert_eq!(found.unwrap().0, 5);
        assert!(state.find_game_pass_by_name("missing").is_none());
    }

    #[test]
    fn test_find_developer_product_by_name_case_insensitive() {
        let mut state = SyncState::default();
        state.update_developer_product(7, "Speed Boost".to_string(), None, None, None, None);
        let found = state.find_developer_product_by_name("SPEED BOOST");
        assert!(found.is_some());
        assert_eq!(found.unwrap().0, 7);
    }

    #[test]
    fn test_find_badge_by_name_case_insensitive() {
        let mut state = SyncState::default();
        state.update_badge(9, "First Win".to_string(), None, None, None, None);
        let found = state.find_badge_by_name("first win");
        assert!(found.is_some());
        assert_eq!(found.unwrap().0, 9);
    }

    #[test]
    fn test_update_overwrites_existing() {
        let mut state = SyncState::default();
        state.update_game_pass(1, "Old".to_string(), None, Some(10), None, None, None);
        state.update_game_pass(1, "New".to_string(), None, Some(20), None, None, None);
        assert_eq!(state.game_passes.len(), 1);
        assert_eq!(state.game_passes.get(&1).unwrap().name, "New");
        assert_eq!(state.game_passes.get(&1).unwrap().price, Some(20));
    }

    #[test]
    fn test_update_universe_replaces() {
        let mut state = SyncState::default();
        state.update_universe(Some("A".to_string()), None, None, None, None, None);
        state.update_universe(Some("B".to_string()), None, None, None, None, None);
        assert_eq!(state.universe.as_ref().unwrap().name.as_deref(), Some("B"));
    }
}
