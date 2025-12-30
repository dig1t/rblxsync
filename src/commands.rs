use crate::api::RobloxClient;
use crate::config::{RbxSyncConfig};
use crate::state::{SyncState, ResourceState};
use anyhow::{anyhow, Result};
use log::{info, warn, error};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::collections::{HashMap, HashSet};

pub async fn run(config: RbxSyncConfig, mut state: SyncState, client: RobloxClient, dry_run: bool) -> Result<()> {
    info!("Starting sync... (dry_run: {})", dry_run);

    // 1. Universe Settings
    if let Some(_universe_id) = config.universe.name.as_ref().and(crate::config::Config::from_env().ok().and_then(|c| c.universe_id)) { 
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
        if dry_run {
            info!("Dry Run: Would update universe settings: {:?}", universe_patch);
        } else {
            client.update_universe_settings(universe_id, &serde_json::Value::Object(universe_patch)).await?;
            info!("Universe settings updated.");
        }
    }

    // 2. Sync Resources
    sync_game_passes(universe_id, &config, &mut state, &client, dry_run).await?;
    sync_developer_products(universe_id, &config, &mut state, &client, dry_run).await?;
    sync_badges(universe_id, &config, &mut state, &client, dry_run).await?;

    // Save state
    if !dry_run {
        let root = std::env::current_dir()?;
        state.save(&root)?;
    } else {
        info!("Dry Run: Would save state.");
    }
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

async fn sync_game_passes(universe_id: u64, config: &RbxSyncConfig, state: &mut SyncState, client: &RobloxClient, dry_run: bool) -> Result<()> {
    info!("Syncing Game Passes...");
    
    // Check for duplicate names (case-insensitive)
    let names: Vec<&str> = config.game_passes.iter().map(|p| p.name.as_str()).collect();
    check_for_duplicates(&names, "game pass")?;
    
    let mut created_count = 0;
    let mut updated_count = 0;
    let mut skipped_count = 0;
    
    // Fetch existing to handle initial discovery
    let existing = if !dry_run {
         client.list_game_passes(universe_id, None).await?
    } else {
        match client.list_game_passes(universe_id, None).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Dry Run: Failed to list game passes (likely due to invalid credentials/universe): {}", e);
                crate::api::ListResponse { data: vec![], next_page_cursor: None }
            }
        }
    };

    let mut remote_map: HashMap<String, (String, u64)> = HashMap::new();
    for item in &existing.data {
        log::debug!("Game pass item from API: {}", item);
        let id = item["id"].as_u64()
            .or_else(|| item["gamePassId"].as_u64())
            .or_else(|| item["id"].as_str().and_then(|s| s.parse().ok()))
            .or_else(|| item["gamePassId"].as_str().and_then(|s| s.parse().ok()));
        
        if let (Some(name), Some(id)) = (item["name"].as_str(), id) {
            log::debug!("Found game pass: {} with ID: {}", name, id);
            remote_map.insert(name.to_lowercase(), (name.to_string(), id));
        }
    }

    for pass in &config.game_passes {
        // Case-insensitive state lookup
        let state_entry = state.game_passes.iter()
            .find(|(k, _)| k.to_lowercase() == pass.name.to_lowercase())
            .map(|(_, v)| v);
        let mut asset_id = None;
        let mut icon_hash = None;
        let mut icon_changed = false;
        let mut changes: Vec<&str> = Vec::new();

        // Handle Icon - calculate hash and check for changes
        if let Some(icon_path_str) = &pass.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            let current_hash = calculate_file_hash(&icon_path).await?;
            let stored_hash = state_entry.and_then(|s| s.icon_hash.as_ref());
            
            if stored_hash == Some(&current_hash) && state_entry.and_then(|s| s.icon_asset_id).is_some() {
                asset_id = state_entry.and_then(|s| s.icon_asset_id);
                icon_hash = Some(current_hash);
                icon_changed = false;
            } else if dry_run {
                asset_id = Some(0); 
                icon_hash = Some(current_hash);
                icon_changed = true;
                changes.push("icon");
            } else {
                let creator = config.creator.as_ref().ok_or_else(|| anyhow!("Creator configuration is required for asset uploads"))?;
                let (aid, hash) = ensure_icon(client, &icon_path, state_entry, creator).await?;
                asset_id = Some(aid);
                icon_hash = Some(hash);
                icon_changed = true;
                changes.push("icon");
            }
        }

        // Determine ID (State -> Remote -> Create) - case-insensitive matching
        let state_id = state.game_passes.iter()
            .find(|(k, _)| k.to_lowercase() == pass.name.to_lowercase())
            .map(|(_, v)| v.id);
        let remote_entry = remote_map.get(&pass.name.to_lowercase());
        let is_new = state_id.is_none() && remote_entry.is_none();
        
        let id = if let Some(sid) = state_id {
            sid
        } else if let Some((_, rid)) = remote_entry {
            *rid
        } else {
            if dry_run {
                info!("  [CREATE] Game Pass '{}' - would create with: name, description, price{}", 
                    pass.name, 
                    if pass.icon.is_some() { ", icon" } else { "" });
                created_count += 1;
                0
            } else {
                let mut body = serde_json::json!({
                    "name": pass.name,
                    "description": pass.description.clone().unwrap_or_default(),
                    "price": pass.price_in_robux.unwrap_or(0), 
                });
                if let Some(aid) = asset_id {
                    body["iconAssetId"] = aid.into();
                }
                
                let resp = client.create_game_pass(universe_id, &body).await?;
                let new_id = resp["id"].as_u64().ok_or(anyhow!("Created game pass has no ID"))?;
                info!("  [CREATED] Game Pass '{}' (ID: {}) - created with: name, description, price{}", 
                    pass.name, new_id,
                    if pass.icon.is_some() { ", icon" } else { "" });
                created_count += 1;
                new_id
            }
        };

        // Update State
        if !dry_run {
            state.update_game_pass(pass.name.clone(), id, icon_hash.clone(), asset_id);
        }

        // Update Remote (Idempotent PATCH) - only if newly created or icon changed
        if is_new {
            // Already created above
        } else if dry_run {
            if icon_changed {
                info!("  [UPDATE] Game Pass '{}' (ID: {}) - would update: {}", 
                    pass.name, id, changes.join(", "));
                updated_count += 1;
            } else {
                info!("  [SKIP] Game Pass '{}' (ID: {}) - no changes detected", pass.name, id);
                skipped_count += 1;
            }
        } else if icon_changed {
            let mut patch = serde_json::Map::new();
            patch.insert("name".to_string(), pass.name.clone().into());
            if let Some(d) = &pass.description { patch.insert("description".to_string(), d.clone().into()); }
            if let Some(p) = pass.price_in_robux { patch.insert("price".to_string(), p.into()); }
            
            client.update_game_pass(universe_id, id, &serde_json::Value::Object(patch)).await?;
            info!("  [UPDATED] Game Pass '{}' (ID: {}) - updated: {}", 
                pass.name, id, changes.join(", "));
            updated_count += 1;
        } else {
            info!("  [SKIP] Game Pass '{}' (ID: {}) - no changes detected", pass.name, id);
            skipped_count += 1;
        }
    }
    
    info!("Game Passes Summary: {} created, {} updated, {} skipped (unchanged)", 
        created_count, updated_count, skipped_count);
    Ok(())
}

async fn sync_developer_products(universe_id: u64, config: &RbxSyncConfig, state: &mut SyncState, client: &RobloxClient, dry_run: bool) -> Result<()> {
    info!("Syncing Developer Products...");
    
    // Check for duplicate names (case-insensitive)
    let names: Vec<&str> = config.developer_products.iter().map(|p| p.name.as_str()).collect();
    check_for_duplicates(&names, "developer product")?;
    
    let mut created_count = 0;
    let mut updated_count = 0;
    let mut skipped_count = 0;
    
    let existing = if !dry_run {
        client.list_developer_products(universe_id, None).await?
    } else {
        match client.list_developer_products(universe_id, None).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Dry Run: Failed to list developer products: {}", e);
                crate::api::ListResponse { data: vec![], next_page_cursor: None }
            }
        }
    };

    let mut remote_map: HashMap<String, (String, u64)> = HashMap::new();
    for item in &existing.data {
        log::debug!("Developer product item from API: {}", item);
        let id = item["id"].as_u64()
            .or_else(|| item["productId"].as_u64())
            .or_else(|| item["developerProductId"].as_u64())
            .or_else(|| item["id"].as_str().and_then(|s| s.parse().ok()))
            .or_else(|| item["productId"].as_str().and_then(|s| s.parse().ok()));
        
        if let (Some(name), Some(id)) = (item["name"].as_str(), id) {
            log::debug!("Found developer product: {} with ID: {}", name, id);
            remote_map.insert(name.to_lowercase(), (name.to_string(), id));
        }
    }

    for prod in &config.developer_products {
        // Case-insensitive state lookup
        let state_entry = state.developer_products.iter()
            .find(|(k, _)| k.to_lowercase() == prod.name.to_lowercase())
            .map(|(_, v)| v);
        let mut asset_id = None;
        let mut icon_hash = None;
        let mut icon_changed = false;
        let mut changes: Vec<&str> = Vec::new();

        if let Some(icon_path_str) = &prod.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            let current_hash = calculate_file_hash(&icon_path).await?;
            let stored_hash = state_entry.and_then(|s| s.icon_hash.as_ref());
            
            if stored_hash == Some(&current_hash) && state_entry.and_then(|s| s.icon_asset_id).is_some() {
                asset_id = state_entry.and_then(|s| s.icon_asset_id);
                icon_hash = Some(current_hash);
                icon_changed = false;
            } else if dry_run {
                asset_id = Some(0);
                icon_hash = Some(current_hash);
                icon_changed = true;
                changes.push("icon");
            } else {
                let creator = config.creator.as_ref().ok_or_else(|| anyhow!("Creator configuration is required for asset uploads"))?;
                let (aid, hash) = ensure_icon(client, &icon_path, state_entry, creator).await?;
                asset_id = Some(aid);
                icon_hash = Some(hash);
                icon_changed = true;
                changes.push("icon");
            }
        }

        // Case-insensitive matching for ID lookup
        let state_id = state.developer_products.iter()
            .find(|(k, _)| k.to_lowercase() == prod.name.to_lowercase())
            .map(|(_, v)| v.id);
        let remote_entry = remote_map.get(&prod.name.to_lowercase());
        let is_new = state_id.is_none() && remote_entry.is_none();

        let id = if let Some(sid) = state_id {
            sid
        } else if let Some((_, rid)) = remote_entry {
            *rid
        } else {
            if dry_run {
                info!("  [CREATE] Developer Product '{}' - would create with: name, price, description{}", 
                    prod.name,
                    if prod.icon.is_some() { ", icon" } else { "" });
                created_count += 1;
                0
            } else {
                let mut body = serde_json::json!({
                    "name": prod.name,
                    "price": prod.price_in_robux,
                    "description": prod.description.clone().unwrap_or_default(),
                });
                if let Some(aid) = asset_id { body["iconAssetId"] = aid.into(); }
                let resp = client.create_developer_product(universe_id, &body).await?;
                let new_id = resp["id"].as_u64().ok_or(anyhow!("Created product has no ID"))?;
                info!("  [CREATED] Developer Product '{}' (ID: {}) - created with: name, price, description{}", 
                    prod.name, new_id,
                    if prod.icon.is_some() { ", icon" } else { "" });
                created_count += 1;
                new_id
            }
        };

        if !dry_run {
            state.update_developer_product(prod.name.clone(), id, icon_hash, asset_id);
        }

        // Update Remote (Idempotent PATCH) - only if newly created or icon changed
        if is_new {
            // Already created above
        } else if dry_run {
            if icon_changed {
                info!("  [UPDATE] Developer Product '{}' (ID: {}) - would update: {}", 
                    prod.name, id, changes.join(", "));
                updated_count += 1;
            } else {
                info!("  [SKIP] Developer Product '{}' (ID: {}) - no changes detected", prod.name, id);
                skipped_count += 1;
            }
        } else if icon_changed {
            let mut patch = serde_json::Map::new();
            patch.insert("name".to_string(), prod.name.clone().into());
            patch.insert("price".to_string(), prod.price_in_robux.into());
            if let Some(d) = &prod.description { patch.insert("description".to_string(), d.clone().into()); }
            
            client.update_developer_product(universe_id, id, &serde_json::Value::Object(patch)).await?;
            info!("  [UPDATED] Developer Product '{}' (ID: {}) - updated: {}", 
                prod.name, id, changes.join(", "));
            updated_count += 1;
        } else {
            info!("  [SKIP] Developer Product '{}' (ID: {}) - no changes detected", prod.name, id);
            skipped_count += 1;
        }
    }
    
    info!("Developer Products Summary: {} created, {} updated, {} skipped (unchanged)", 
        created_count, updated_count, skipped_count);
    Ok(())
}

async fn sync_badges(universe_id: u64, config: &RbxSyncConfig, state: &mut SyncState, client: &RobloxClient, dry_run: bool) -> Result<()> {
    info!("Syncing Badges...");
    
    // Check for duplicate names (case-insensitive)
    let names: Vec<&str> = config.badges.iter().map(|b| b.name.as_str()).collect();
    check_for_duplicates(&names, "badge")?;
    
    let mut created_count = 0;
    let mut updated_count = 0;
    let mut skipped_count = 0;
    
    let existing = if !dry_run {
        client.list_badges(universe_id, None).await?
    } else {
        match client.list_badges(universe_id, None).await {
            Ok(r) => r,
            Err(e) => {
                warn!("Dry Run: Failed to list badges: {}", e);
                crate::api::ListResponse { data: vec![], next_page_cursor: None }
            }
        }
    };

    let mut remote_map: HashMap<String, (String, u64)> = HashMap::new();
    for item in existing.data {
        if let (Some(name), Some(id)) = (item["name"].as_str(), item["id"].as_u64()) {
            remote_map.insert(name.to_lowercase(), (name.to_string(), id));
        }
    }

    for badge in &config.badges {
        // Case-insensitive state lookup
        let state_entry = state.badges.iter()
            .find(|(k, _)| k.to_lowercase() == badge.name.to_lowercase())
            .map(|(_, v)| v);
        let mut changes: Vec<&str> = Vec::new();
        
        // Prepare icon data if provided
        let icon_data = if let Some(icon_path_str) = &badge.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            if icon_path.exists() {
                let data = tokio::fs::read(&icon_path).await?;
                let filename = icon_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let hash = format!("{:x}", hasher.finalize());
                
                Some((data, filename, hash))
            } else {
                warn!("Badge icon not found: {:?}", icon_path);
                None
            }
        } else {
            None
        };

        // Check if icon has changed
        let icon_changed = if let Some((_, _, new_hash)) = &icon_data {
            let stored_hash = state_entry.and_then(|s| s.icon_hash.as_ref());
            if stored_hash != Some(new_hash) {
                changes.push("icon");
                true
            } else {
                false
            }
        } else {
            false
        };

        // Case-insensitive matching for ID lookup
        let state_id = state.badges.iter()
            .find(|(k, _)| k.to_lowercase() == badge.name.to_lowercase())
            .map(|(_, v)| v.id);
        let remote_entry = remote_map.get(&badge.name.to_lowercase());
        let is_new = state_id.is_none() && remote_entry.is_none();

        let id = if let Some(sid) = state_id {
            sid
        } else if let Some((_, rid)) = remote_entry {
            *rid
        } else {
            if dry_run {
                info!("  [CREATE] Badge '{}' - would create with: name, description{}", 
                    badge.name,
                    if badge.icon.is_some() { ", icon" } else { "" });
                created_count += 1;
                0
            } else {
                let image_for_create = icon_data.as_ref().map(|(data, filename, _)| (data.clone(), filename.clone()));
                
                let result = client.create_badge(
                    universe_id,
                    &badge.name,
                    badge.description.as_deref().unwrap_or(""),
                    image_for_create,
                    config.badge_payment_source.as_deref()
                ).await;
                
                let resp = match result {
                    Ok(r) => r,
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("Payment source is invalid") || err_str.contains("code\":16") {
                            error!("Badge creation failed: Payment source is required.");
                            error!("");
                            error!("Creating badges costs 100 Robux. Please add the following to your rbxsync.yml:");
                            error!("");
                            error!("  badge_payment_source: \"user\"   # Pay from your user account");
                            error!("  # OR");
                            error!("  badge_payment_source: \"group\"  # Pay from group funds");
                            error!("");
                            return Err(anyhow!("Badge creation requires badge_payment_source configuration"));
                        }
                        return Err(e);
                    }
                };
                
                let new_id = resp["id"].as_u64().ok_or(anyhow!("Created badge has no ID"))?;
                info!("  [CREATED] Badge '{}' (ID: {}) - created with: name, description{}", 
                    badge.name, new_id,
                    if badge.icon.is_some() { ", icon" } else { "" });
                created_count += 1;
                new_id
            }
        };

        // Update state with icon hash
        let icon_hash = icon_data.as_ref().map(|(_, _, hash)| hash.clone());
        if !dry_run {
            state.update_badge(badge.name.clone(), id, icon_hash.clone(), None);
        }

        // Update Remote (Idempotent PATCH) - only if newly created or icon changed
        if is_new {
            // Already created above
        } else if dry_run {
            if icon_changed {
                info!("  [UPDATE] Badge '{}' (ID: {}) - would update: {}", 
                    badge.name, id, changes.join(", "));
                updated_count += 1;
            } else {
                info!("  [SKIP] Badge '{}' (ID: {}) - no changes detected", badge.name, id);
                skipped_count += 1;
            }
        } else if icon_changed {
            let mut patch = serde_json::Map::new();
            patch.insert("name".to_string(), badge.name.clone().into());
            if let Some(d) = &badge.description { patch.insert("description".to_string(), d.clone().into()); }
            if let Some(e) = badge.is_enabled { patch.insert("enabled".to_string(), e.into()); }
            
            client.update_badge(id, &serde_json::Value::Object(patch)).await?;
            
            // Update icon since it changed
            if let Some((data, filename, _)) = &icon_data {
                client.update_badge_icon(id, data.clone(), filename).await?;
            }
            info!("  [UPDATED] Badge '{}' (ID: {}) - updated: {}", 
                badge.name, id, changes.join(", "));
            updated_count += 1;
        } else {
            info!("  [SKIP] Badge '{}' (ID: {}) - no changes detected", badge.name, id);
            skipped_count += 1;
        }
    }
    
    info!("Badges Summary: {} created, {} updated, {} skipped (unchanged)", 
        created_count, updated_count, skipped_count);
    Ok(())
}

/// Check for duplicate names (case-insensitive) in a list
fn check_for_duplicates(names: &[&str], resource_type: &str) -> Result<()> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut duplicates: Vec<String> = Vec::new();
    
    for name in names {
        let lower = name.to_lowercase();
        if seen.contains(&lower) {
            duplicates.push((*name).to_string());
        } else {
            seen.insert(lower);
        }
    }
    
    if !duplicates.is_empty() {
        return Err(anyhow!(
            "Duplicate {} names found (names must be unique, case-insensitive): {:?}",
            resource_type,
            duplicates
        ));
    }
    
    Ok(())
}

/// Calculate SHA-256 hash of a file
async fn calculate_file_hash(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(anyhow!("File not found: {:?}", path));
    }
    let content = tokio::fs::read(path).await?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

async fn ensure_icon(client: &RobloxClient, path: &Path, state: Option<&ResourceState>, creator: &crate::config::CreatorConfig) -> Result<(u64, String)> {
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
    let asset_id_str = client.upload_asset(path, &name, creator).await?;
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

