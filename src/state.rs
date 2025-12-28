use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct SyncState {
    #[serde(default)]
    pub game_passes: HashMap<String, ResourceState>,
    #[serde(default)]
    pub developer_products: HashMap<String, ResourceState>,
    #[serde(default)]
    pub badges: HashMap<String, ResourceState>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResourceState {
    pub id: u64,
    pub icon_hash: Option<String>,
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

    pub fn get_game_pass_id(&self, name: &str) -> Option<u64> {
        self.game_passes.get(name).map(|r| r.id)
    }

    pub fn update_game_pass(&mut self, name: String, id: u64, icon_hash: Option<String>, icon_asset_id: Option<u64>) {
        self.game_passes.insert(name, ResourceState { id, icon_hash, icon_asset_id });
    }
    
    // Helpers for other types...
    pub fn update_developer_product(&mut self, name: String, id: u64, icon_hash: Option<String>, icon_asset_id: Option<u64>) {
        self.developer_products.insert(name, ResourceState { id, icon_hash, icon_asset_id });
    }

    pub fn update_badge(&mut self, name: String, id: u64, icon_hash: Option<String>, icon_asset_id: Option<u64>) {
        self.badges.insert(name, ResourceState { id, icon_hash, icon_asset_id });
    }
}

