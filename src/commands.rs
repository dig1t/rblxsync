use crate::api::RobloxClient;
use crate::config::{RbxSyncConfig, GamePassConfig, DeveloperProductConfig, BadgeConfig};
use crate::state::{SyncState, ResourceState};
use anyhow::{anyhow, Result};
use log::{info, warn, error};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::collections::HashMap;

pub async fn run(config: RbxSyncConfig, mut state: SyncState, client: RobloxClient) -> Result<()> {
    info!("Starting sync...");

    // 1. Universe Settings
    if let Some(universe_id) = config.universe.name.as_ref().and(crate::config::Config::from_env().ok().and_then(|c| c.universe_id)) { 
        // Logic to update universe settings if provided
        // NOTE: The config.universe struct has fields like name, description etc.
        // We need the universe ID from somewhere. 
        // The user config has `universe` block, but usually `universe_id` is env var or arg?
        // User query: "Universe: PATCH .../universes/{universeId}/configuration"
        // User config example doesn't have ID in `universe` block, only metadata.
        // ID comes from Env Var `ROBLOX_UNIVERSE_ID`.
    }
    
    let universe_id = std::env::var("ROBLOX_UNIVERSE_ID")
        .map_err(|_| anyhow!("ROBLOX_UNIVERSE_ID is required for sync"))?
        .parse::<u64>()?;

    // Update Universe Settings
    info!("Syncing Universe Settings...");
    // Construct patch body
    let mut universe_patch = serde_json::Map::new();
    if let Some(name) = &config.universe.name { universe_patch.insert("name".to_string(), name.clone().into()); }
    if let Some(desc) = &config.universe.description { universe_patch.insert("description".to_string(), desc.clone().into()); }
    if let Some(genre) = &config.universe.genre { universe_patch.insert("genre".to_string(), genre.clone().into()); }
    if let Some(devices) = &config.universe.playable_devices { universe_patch.insert("playableDevices".to_string(), serde_json::json!(devices)); }
    
    if !universe_patch.is_empty() {
        client.update_universe_settings(universe_id, &serde_json::Value::Object(universe_patch)).await?;
        info!("Universe settings updated.");
    }

    // 2. Sync Resources
    sync_game_passes(universe_id, &config, &mut state, &client).await?;
    sync_developer_products(universe_id, &config, &mut state, &client).await?;
    sync_badges(universe_id, &config, &mut state, &client).await?;

    // Save state
    let root = std::env::current_dir()?;
    state.save(&root)?;
    info!("Sync complete!");
    Ok(())
}

pub async fn publish(config: RbxSyncConfig, client: RobloxClient) -> Result<()> {
    let universe_id = std::env::var("ROBLOX_UNIVERSE_ID")
        .map_err(|_| anyhow!("ROBLOX_UNIVERSE_ID is required for publish"))?
        .parse::<u64>()?;

    for place in config.places {
        if place.publish {
            info!("Publishing place {} from {}", place.place_id, place.file_path);
            let path = Path::new(&place.file_path);
            if !path.exists() {
                error!("File not found: {}", place.file_path);
                continue;
            }
            match client.publish_place(universe_id, place.place_id, path).await {
                Ok(_) => info!("Published place {}", place.place_id),
                Err(e) => error!("Failed to publish place {}: {}", place.place_id, e),
            }
        }
    }
    Ok(())
}

async fn sync_game_passes(universe_id: u64, config: &RbxSyncConfig, state: &mut SyncState, client: &RobloxClient) -> Result<()> {
    info!("Syncing Game Passes...");
    // Fetch existing to handle initial discovery
    let existing = client.list_game_passes(universe_id, None).await?;
    let mut remote_map: HashMap<String, u64> = HashMap::new();
    for item in existing.data {
        if let (Some(name), Some(id)) = (item["name"].as_str(), item["id"].as_u64()) {
            remote_map.insert(name.to_string(), id);
        }
    }

    for pass in &config.game_passes {
        let mut asset_id = None;
        let mut icon_hash = None;

        // Handle Icon
        if let Some(icon_path_str) = &pass.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            let state_entry = state.game_passes.get(&pass.name);
            let (aid, hash) = ensure_icon(client, &icon_path, state_entry).await?;
            asset_id = Some(aid);
            icon_hash = Some(hash);
        }

        // Determine ID (State -> Remote -> Create)
        let id = if let Some(sid) = state.get_game_pass_id(&pass.name) {
            sid
        } else if let Some(rid) = remote_map.get(&pass.name) {
            *rid
        } else {
            // Create
            info!("Creating Game Pass: {}", pass.name);
            let mut body = serde_json::json!({
                "name": pass.name,
                "description": pass.description.clone().unwrap_or_default(),
                "price": pass.price_in_robux.unwrap_or(0), 
            });
            if let Some(aid) = asset_id {
                body["iconAssetId"] = aid.into();
            }
            
            let resp = client.create_game_pass(universe_id, &body).await?;
            resp["id"].as_u64().ok_or(anyhow!("Created game pass has no ID"))?
        };

        // Update State
        state.update_game_pass(pass.name.clone(), id, icon_hash.clone(), asset_id);

        // Update Remote (Idempotent PATCH)
        info!("Updating Game Pass: {}", pass.name);
        let mut patch = serde_json::Map::new();
        patch.insert("name".to_string(), pass.name.clone().into());
        if let Some(d) = &pass.description { patch.insert("description".to_string(), d.clone().into()); }
        if let Some(p) = pass.price_in_robux { patch.insert("price".to_string(), p.into()); }
        if let Some(aid) = asset_id { patch.insert("iconAssetId".to_string(), aid.into()); }
        // Game Pass specific: isForSale ?? The user schema has `is_for_sale`.
        // Check API: `price` usually implies for sale if > 0? 
        // Or there might be specific field.
        // User query: "isForSale/on-sale"
        // Let's assume standard field name.
        
        client.update_game_pass(id, &serde_json::Value::Object(patch)).await?;
    }
    Ok(())
}

async fn sync_developer_products(universe_id: u64, config: &RbxSyncConfig, state: &mut SyncState, client: &RobloxClient) -> Result<()> {
    info!("Syncing Developer Products...");
    // Similar logic...
    let existing = client.list_developer_products(universe_id, None).await?;
    let mut remote_map: HashMap<String, u64> = HashMap::new();
    for item in existing.data {
        if let (Some(name), Some(id)) = (item["name"].as_str(), item["id"].as_u64()) {
            remote_map.insert(name.to_string(), id);
        }
    }

    for prod in &config.developer_products {
        let mut asset_id = None;
        let mut icon_hash = None;

        if let Some(icon_path_str) = &prod.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            let state_entry = state.developer_products.get(&prod.name);
            let (aid, hash) = ensure_icon(client, &icon_path, state_entry).await?;
            asset_id = Some(aid);
            icon_hash = Some(hash);
        }

        let id = if let Some(sid) = state.developer_products.get(&prod.name).map(|r| r.id) {
            sid
        } else if let Some(rid) = remote_map.get(&prod.name) {
            *rid
        } else {
             info!("Creating Developer Product: {}", prod.name);
             let mut body = serde_json::json!({
                 "name": prod.name,
                 "price": prod.price_in_robux,
                 "description": prod.description.clone().unwrap_or_default(),
             });
             if let Some(aid) = asset_id { body["iconAssetId"] = aid.into(); }
             let resp = client.create_developer_product(universe_id, &body).await?;
             resp["id"].as_u64().ok_or(anyhow!("Created product has no ID"))?
        };

        state.update_developer_product(prod.name.clone(), id, icon_hash, asset_id);

        info!("Updating Developer Product: {}", prod.name);
        let mut patch = serde_json::Map::new();
        patch.insert("name".to_string(), prod.name.clone().into());
        patch.insert("price".to_string(), prod.price_in_robux.into());
        if let Some(d) = &prod.description { patch.insert("description".to_string(), d.clone().into()); }
        if let Some(aid) = asset_id { patch.insert("iconAssetId".to_string(), aid.into()); }
        
        client.update_developer_product(id, &serde_json::Value::Object(patch)).await?;
    }
    Ok(())
}

async fn sync_badges(universe_id: u64, config: &RbxSyncConfig, state: &mut SyncState, client: &RobloxClient) -> Result<()> {
    info!("Syncing Badges...");
     let existing = client.list_badges(universe_id, None).await?;
    let mut remote_map: HashMap<String, u64> = HashMap::new();
    for item in existing.data {
        if let (Some(name), Some(id)) = (item["name"].as_str(), item["id"].as_u64()) {
            remote_map.insert(name.to_string(), id);
        }
    }

    for badge in &config.badges {
        let mut asset_id = None;
        let mut icon_hash = None;

        if let Some(icon_path_str) = &badge.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            let state_entry = state.badges.get(&badge.name);
            let (aid, hash) = ensure_icon(client, &icon_path, state_entry).await?;
            asset_id = Some(aid);
            icon_hash = Some(hash);
        }

        let id = if let Some(sid) = state.badges.get(&badge.name).map(|r| r.id) {
            sid
        } else if let Some(rid) = remote_map.get(&badge.name) {
            *rid
        } else {
             info!("Creating Badge: {}", badge.name);
             let mut body = serde_json::json!({
                 "name": badge.name,
                 "description": badge.description.clone().unwrap_or_default(),
             });
             if let Some(aid) = asset_id { body["iconImageId"] = aid.into(); } // Note: Badges might use iconImageId
             let resp = client.create_badge(universe_id, &body).await?;
             resp["id"].as_u64().ok_or(anyhow!("Created badge has no ID"))?
        };

        state.update_badge(badge.name.clone(), id, icon_hash, asset_id);

        info!("Updating Badge: {}", badge.name);
        let mut patch = serde_json::Map::new();
        patch.insert("name".to_string(), badge.name.clone().into());
        if let Some(d) = &badge.description { patch.insert("description".to_string(), d.clone().into()); }
        if let Some(aid) = asset_id { patch.insert("iconImageId".to_string(), aid.into()); }
        if let Some(e) = badge.is_enabled { patch.insert("enabled".to_string(), e.into()); }
        
        client.update_badge(id, &serde_json::Value::Object(patch)).await?;
    }
    Ok(())
}

async fn ensure_icon(client: &RobloxClient, path: &Path, state: Option<&ResourceState>) -> Result<(u64, String)> {
    if !path.exists() {
        return Err(anyhow!("Icon file not found: {:?}", path));
    }

    // Calculate Hash
    let content = tokio::fs::read(path).await?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let hash = format!("{:x}", hasher.finalize());

    // Check State
    if let Some(s) = state {
        if let (Some(sh), Some(sid)) = (&s.icon_hash, s.icon_asset_id) {
            if sh == &hash {
                return Ok((sid, hash));
            }
        }
    }

    // Upload
    info!("Uploading icon: {:?}", path);
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    let asset_id_str = client.upload_asset(path, &name).await?;
    let asset_id = asset_id_str.parse::<u64>()?;
    
    Ok((asset_id, hash))
}

pub async fn export(client: RobloxClient, output: Option<String>, format_lua: bool) -> Result<()> {
    let universe_id = std::env::var("ROBLOX_UNIVERSE_ID")
        .map_err(|_| anyhow!("ROBLOX_UNIVERSE_ID is required for export"))?
        .parse::<u64>()?;

    info!("Exporting universe {}...", universe_id);
    // Fetch all data
    let passes = client.list_game_passes(universe_id, None).await?;
    let products = client.list_developer_products(universe_id, None).await?;
    let badges = client.list_badges(universe_id, None).await?;

    // Generate output
    // Simple Luau table generation
    let mut lua = String::from("return {\n");
    
    lua.push_str("  game_passes = {\n");
    for item in passes.data {
        lua.push_str("    {\n");
        if let Some(n) = item["name"].as_str() { lua.push_str(&format!("      name = \"{}\",\n", n)); }
        if let Some(id) = item["id"].as_u64() { lua.push_str(&format!("      id = {},\n", id)); }
        if let Some(p) = item["price"].as_u64() { lua.push_str(&format!("      price = {},\n", p)); }
        lua.push_str("    },\n");
    }
    lua.push_str("  },\n");

    lua.push_str("  developer_products = {\n");
    for item in products.data {
        lua.push_str("    {\n");
        if let Some(n) = item["name"].as_str() { lua.push_str(&format!("      name = \"{}\",\n", n)); }
        if let Some(id) = item["id"].as_u64() { lua.push_str(&format!("      id = {},\n", id)); }
        if let Some(p) = item["price"].as_u64() { lua.push_str(&format!("      price = {},\n", p)); }
        lua.push_str("    },\n");
    }
    lua.push_str("  },\n");

    lua.push_str("  badges = {\n");
    for item in badges.data {
        lua.push_str("    {\n");
        if let Some(n) = item["name"].as_str() { lua.push_str(&format!("      name = \"{}\",\n", n)); }
        if let Some(id) = item["id"].as_u64() { lua.push_str(&format!("      id = {},\n", id)); }
        lua.push_str("    },\n");
    }
    lua.push_str("  },\n");

    lua.push_str("}\n");

    let out_path = output.unwrap_or_else(|| if format_lua { "config.lua".to_string() } else { "config.luau".to_string() });
    std::fs::write(&out_path, lua)?;
    info!("Exported to {}", out_path);

    Ok(())
}

