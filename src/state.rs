use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct SyncState {
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
        fs::write(state_path, content)?;
        Ok(())
    }

    fn get_state_path(project_root: &Path) -> PathBuf {
        project_root.join(".rbxsync").join("state.yaml")
    }

    /// Find a game pass by name (case-insensitive) and return (id, state)
    pub fn find_game_pass_by_name(&self, name: &str) -> Option<(u64, &ResourceState)> {
        self.game_passes.iter()
            .find(|(_, state)| state.name.to_lowercase() == name.to_lowercase())
            .map(|(id, state)| (*id, state))
    }

    pub fn update_game_pass(
        &mut self, 
        id: u64, 
        name: String, 
        description: Option<String>,
        price: Option<u64>,
        is_for_sale: Option<bool>,
        icon_hash: Option<String>, 
        icon_asset_id: Option<u64>
    ) {
        self.game_passes.insert(id, ResourceState { 
            name, 
            description,
            price,
            is_for_sale,
            is_enabled: None,
            icon_hash, 
            icon_asset_id 
        });
    }
    
    /// Find a developer product by name (case-insensitive) and return (id, state)
    pub fn find_developer_product_by_name(&self, name: &str) -> Option<(u64, &ResourceState)> {
        self.developer_products.iter()
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
        icon_asset_id: Option<u64>
    ) {
        self.developer_products.insert(id, ResourceState { 
            name, 
            description,
            price,
            is_for_sale: None,
            is_enabled: None,
            icon_hash, 
            icon_asset_id 
        });
    }

    /// Find a badge by name (case-insensitive) and return (id, state)
    pub fn find_badge_by_name(&self, name: &str) -> Option<(u64, &ResourceState)> {
        self.badges.iter()
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
        icon_asset_id: Option<u64>
    ) {
        self.badges.insert(id, ResourceState { 
            name, 
            description,
            price: None,
            is_for_sale: None,
            is_enabled,
            icon_hash, 
            icon_asset_id 
        });
    }
}

