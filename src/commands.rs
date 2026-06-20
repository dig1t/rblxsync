use crate::api::{RobloxClient, RobloxCookieClient};
use crate::config::{
    BadgeConfig, DeveloperProductConfig, GamePassConfig, PlaceConfig, PrivateServerCost,
    RblxSyncConfig, UniverseConfig,
};
use crate::output;
use crate::state::{ResourceState, SyncState, UniverseState};
use crate::yml_edit;
use anyhow::{anyhow, Context, Result};
use log::{error, info, warn};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Validate the configuration for errors (including case-insensitive duplicate names)
pub fn validate(config: &RblxSyncConfig) -> Result<()> {
    // Check for duplicate game pass names (case-insensitive)
    let game_pass_names: Vec<&str> = config.game_passes.iter().map(|p| p.name.as_str()).collect();
    check_for_duplicates(&game_pass_names, "game pass")?;

    // Check for duplicate developer product names (case-insensitive)
    let product_names: Vec<&str> = config
        .developer_products
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    check_for_duplicates(&product_names, "developer product")?;

    // Check for duplicate badge names (case-insensitive)
    let badge_names: Vec<&str> = config.badges.iter().map(|b| b.name.as_str()).collect();
    check_for_duplicates(&badge_names, "badge")?;

    Ok(())
}

/// Collect the lowercased `name` values from a list response, tolerating a
/// failed listing only in dry-run (where bad creds shouldn't block a preview).
fn remote_name_set(
    list: Result<crate::api::ListResponse<serde_json::Value>>,
    dry_run: bool,
    kind: &str,
) -> Result<HashSet<String>> {
    match list {
        Ok(r) => Ok(r
            .data
            .iter()
            .filter_map(|i| i["name"].as_str().map(|s| s.to_lowercase()))
            .collect()),
        Err(e) if dry_run => {
            warn!(
                "Preflight: could not list {} (dry-run; assuming none exist): {}",
                kind, e
            );
            Ok(HashSet::new())
        }
        Err(e) => Err(e.context(format!("Preflight: failed to list {}", kind))),
    }
}

/// Read-only preflight run before any mutation. Validates everything knowable up
/// front so a destructive `run` never half-applies: referenced icon files must
/// exist, icons on passes/products need a `creator`, and a badge that would be
/// CREATED needs an icon (Roblox rejects creation without one) plus a payment
/// source. Aborts listing every problem; makes no changes.
async fn preflight(
    universe_id: u64,
    config: &RblxSyncConfig,
    state: &SyncState,
    client: &RobloxClient,
    dry_run: bool,
) -> Result<()> {
    let assets_dir = Path::new(&config.assets_dir);
    let mut errors: Vec<String> = Vec::new();

    // Passes / products: a referenced icon file must exist, and uploading any
    // icon needs a configured creator.
    for p in &config.game_passes {
        if let Some(icon) = &p.icon {
            let path = assets_dir.join(icon);
            if !path.exists() {
                errors.push(format!(
                    "Game pass '{}': icon file not found at {}",
                    p.name,
                    path.display()
                ));
            } else if config.creator.is_none() {
                errors.push(format!(
                    "Game pass '{}': has an icon but no `creator:` is configured (required to upload assets)",
                    p.name
                ));
            }
        }
    }
    for p in &config.developer_products {
        if let Some(icon) = &p.icon {
            let path = assets_dir.join(icon);
            if !path.exists() {
                errors.push(format!(
                    "Developer product '{}': icon file not found at {}",
                    p.name,
                    path.display()
                ));
            } else if config.creator.is_none() {
                errors.push(format!(
                    "Developer product '{}': has an icon but no `creator:` is configured (required to upload assets)",
                    p.name
                ));
            }
        }
    }

    // Badges: need the live list to know which entries would be CREATED.
    let badge_remote = remote_name_set(
        client.list_badges(universe_id, None).await,
        dry_run,
        "badges",
    )?;
    for b in &config.badges {
        if let Some(icon) = &b.icon {
            let path = assets_dir.join(icon);
            if !path.exists() {
                errors.push(format!(
                    "Badge '{}': icon file not found at {}",
                    b.name,
                    path.display()
                ));
            }
        }
        let is_new = b.id.is_none()
            && state.find_badge_by_name(&b.name).is_none()
            && !badge_remote.contains(&b.name.to_lowercase());
        if is_new {
            if b.icon.is_none() {
                errors.push(format!(
                    "Badge '{}': new badges require an `icon:` - Roblox rejects creation without one",
                    b.name
                ));
            }
            if config.badge_payment_source.is_none() {
                errors.push(format!(
                    "Badge '{}': new badges cost 100 Robux and require `badge_payment_source: \"user\"` or `\"group\"`",
                    b.name
                ));
            }
        }
    }

    if !errors.is_empty() {
        let joined = errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow!(
            "Preflight found {} problem(s); no changes were made:\n{}",
            errors.len(),
            joined
        ));
    }

    info!("Preflight checks passed.");
    Ok(())
}

pub async fn run(
    config: RblxSyncConfig,
    mut state: SyncState,
    client: RobloxClient,
    cookie_client: Option<RobloxCookieClient>,
    dry_run: bool,
    config_path: &Path,
) -> Result<()> {
    info!("Starting sync... (dry_run: {})", dry_run);

    // Validate config before proceeding
    validate(&config)?;

    let universe_id = config.universe.id;

    // Preflight: catch everything knowable BEFORE any mutation, so a failed
    // run never half-applies (no resources created, no Robux spent on a config
    // that can't fully succeed). Runs in dry-run too, so previews surface these.
    preflight(universe_id, &config, &state, &client, dry_run).await?;

    // Update Universe Settings (requires cookie client)
    if config.universe.has_settings() {
        if let Some(ref cookie_client) = cookie_client {
            sync_universe_settings(universe_id, &config, &mut state, cookie_client, dry_run)
                .await?;
        }
    }

    // 2. Sync Resources. Each sync writes a newly-created resource's id back
    // into the yml *immediately* (see write_back_id), so a created resource's
    // identity is durable even if a later step fails or the lock file is never
    // written — the next run then matches by id and never makes a duplicate.
    sync_game_passes(
        universe_id,
        &config,
        &mut state,
        &client,
        dry_run,
        config_path,
    )
    .await?;
    sync_developer_products(
        universe_id,
        &config,
        &mut state,
        &client,
        dry_run,
        config_path,
    )
    .await?;
    sync_badges(
        universe_id,
        &config,
        &mut state,
        &client,
        dry_run,
        config_path,
    )
    .await?;

    // Save state next to the config (where it is also loaded from), so a
    // `--config sub/x.yml` keeps the lock file beside it. Atomic via state.save.
    if !dry_run {
        state.save(lock_root(config_path))?;
    } else {
        info!("Dry Run: Would save state.");
    }

    // Generate output config file if output_path is specified
    if let Some(output_path) = &config.output_path {
        if dry_run {
            info!("Dry Run: Would generate config file at {}", output_path);
        } else {
            output::generate_config(&state, config.universe.id, output_path)?;
        }
    }

    info!("Sync complete!");
    Ok(())
}

pub async fn publish(config: RblxSyncConfig, client: RobloxClient) -> Result<()> {
    let universe_id = config.universe.id;

    for place in config.places {
        if place.publish {
            info!(
                "Publishing place {} from {}",
                place.place_id, place.file_path
            );
            let path = Path::new(&place.file_path);
            if !path.exists() {
                error!("File not found: {}", place.file_path);
                continue;
            }
            match client
                .publish_place(universe_id, place.place_id, path)
                .await
            {
                Ok(_) => info!("Published place {}", place.place_id),
                Err(e) => error!("Failed to publish place {}: {}", place.place_id, e),
            }
        }
    }
    Ok(())
}

/// Directory the lock file (`rblxsync-lock.yml`) lives in: the config file's
/// parent, falling back to the current directory for a bare filename.
fn lock_root(config_path: &Path) -> &Path {
    config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Immediately write a newly-created resource's id back into its `rblxsync.yml`
/// entry, via a surgical comment-preserving insert ([`yml_edit::insert_id`]).
///
/// Called the instant a resource is created — not batched at the end — so the
/// yml is the durable record of identity even if the run later fails or the lock
/// file is never written. Combined with id-first matching, the next run adopts
/// the resource by id instead of creating a duplicate. If the entry can't be
/// located, warn (the id was already logged on create); on success the file is
/// read and rewritten once.
fn write_back_id(config_path: &Path, section: &str, name: &str, id: u64) -> Result<()> {
    if !config_path.exists() {
        return Ok(());
    }

    let yaml = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config for id write-back: {:?}", config_path))?;

    match yml_edit::insert_id(&yaml, section, name, id) {
        Some(updated) => crate::fsutil::atomic_write(config_path, updated.as_bytes())
            .context("Failed to write config after id write-back")?,
        None => warn!(
            "Could not write id for \"{}\" into {:?} - run `rblxsync import` to backfill ids",
            name, config_path
        ),
    }
    Ok(())
}

/// Compute a non-clobbering backup path for `path`: `<stem>.old.<ext>`, then
/// `<stem>.old1.<ext>`, `<stem>.old2.<ext>`, ... using the first that does not
/// already exist.
fn backup_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path.extension().map(|s| s.to_string_lossy().to_string());

    let build = |suffix: &str| -> PathBuf {
        let filename = match &ext {
            Some(ext) => format!("{}.{}.{}", stem, suffix, ext),
            None => format!("{}.{}", stem, suffix),
        };
        parent.join(filename)
    };

    let first = build("old");
    if !first.exists() {
        return first;
    }
    let mut n = 1;
    loop {
        let candidate = build(&format!("old{}", n));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Parse the universe (experience) id from a Place resource path of the form
/// `universes/{universe_id}/places/{place_id}`.
fn parse_universe_id_from_place_path(path: &str) -> Option<u64> {
    let mut segs = path.split('/');
    while let Some(seg) = segs.next() {
        if seg == "universes" {
            return segs.next().and_then(|s| s.parse().ok());
        }
    }
    None
}

/// Import an existing experience's metadata from Roblox into the local
/// `rblxsync.yml` and `rblxsync-lock.yml`. Remote is authoritative on
/// conflicts; local-only entries are preserved.
pub async fn import(
    client: RobloxClient,
    config_path: &Path,
    universe_id_override: Option<u64>,
    place_ids: Vec<u64>,
) -> Result<()> {
    // Load existing config + state if present.
    let existing_config: Option<RblxSyncConfig> = if config_path.exists() {
        Some(RblxSyncConfig::load(config_path).context("Failed to load existing config")?)
    } else {
        None
    };
    let root = config_path.parent().unwrap_or(Path::new("."));
    let existing_state = SyncState::load(root).unwrap_or_default();

    // Resolve universe id.
    let universe_id = universe_id_override
        .or_else(|| existing_config.as_ref().map(|c| c.universe.id))
        .ok_or_else(|| {
            anyhow!(
                "No universe id available. Pass --universe-id or add universe.id to {:?}",
                config_path
            )
        })?;

    info!("Importing universe {}...", universe_id);

    // --- Fetch remote ---
    let remote_universe = client
        .get_universe(universe_id)
        .await
        .context("Failed to fetch universe")?;
    let remote_name = remote_universe
        .get("displayName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let remote_description = remote_universe
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Game passes: list (thin) then fetch full detail per pass.
    let pass_list = client.list_game_passes(universe_id, None).await?;
    let mut remote_passes: Vec<RemoteResource> = Vec::new();
    for item in &pass_list.data {
        let id = item["id"]
            .as_u64()
            .or_else(|| item["gamePassId"].as_u64())
            .or_else(|| item["id"].as_str().and_then(|s| s.parse().ok()))
            .or_else(|| item["gamePassId"].as_str().and_then(|s| s.parse().ok()));
        let Some(id) = id else { continue };
        let detail = client.get_game_pass(universe_id, id).await?;
        let name = detail["name"]
            .as_str()
            .or_else(|| item["name"].as_str())
            .unwrap_or_default()
            .to_string();
        let description = detail["description"].as_str().map(|s| s.to_string());
        let is_for_sale = detail["isForSale"].as_bool();
        let price = detail["priceInformation"]["defaultPriceInRobux"].as_u64();
        remote_passes.push(RemoteResource {
            id,
            name,
            description,
            price,
            is_for_sale,
            is_enabled: None,
        });
    }

    // Developer products: list already carries detail.
    let product_list = client.list_developer_products(universe_id, None).await?;
    let mut remote_products: Vec<RemoteResource> = Vec::new();
    for item in &product_list.data {
        let id = item["id"]
            .as_u64()
            .or_else(|| item["productId"].as_u64())
            .or_else(|| item["developerProductId"].as_u64())
            .or_else(|| item["id"].as_str().and_then(|s| s.parse().ok()))
            .or_else(|| item["productId"].as_str().and_then(|s| s.parse().ok()));
        let Some(id) = id else { continue };
        let name = item["name"].as_str().unwrap_or_default().to_string();
        let description = item["description"].as_str().map(|s| s.to_string());
        let price = item["priceInformation"]["defaultPriceInRobux"].as_u64();
        remote_products.push(RemoteResource {
            id,
            name,
            description,
            price,
            is_for_sale: item["isForSale"].as_bool(),
            is_enabled: None,
        });
    }

    // Badges: list carries full objects.
    let badge_list = client.list_badges(universe_id, None).await?;
    let mut remote_badges: Vec<RemoteResource> = Vec::new();
    for item in &badge_list.data {
        let Some(id) = item["id"].as_u64() else {
            continue;
        };
        let name = item["name"].as_str().unwrap_or_default().to_string();
        let description = item["description"].as_str().map(|s| s.to_string());
        let is_enabled = item["enabled"].as_bool();
        remote_badges.push(RemoteResource {
            id,
            name,
            description,
            price: None,
            is_for_sale: None,
            is_enabled,
        });
    }

    // Places: the API key can only auto-discover the root place. Any extra
    // place ids the user passes via --place-id are validated + named through
    // the universe-scoped single-place GET (a wrong id/universe returns 404).
    let mut remote_place_ids = client.list_places(universe_id).await?;
    for pid in &place_ids {
        if remote_place_ids.contains(pid) {
            continue;
        }
        match client.get_place(universe_id, *pid).await {
            Ok(place) => {
                // Safety: the place's experience (universe) id must match the
                // universe being imported (the one written to rblxsync.yml).
                // The GET is universe-scoped, but verify the returned resource
                // path too, so a place from a different experience can never be
                // pulled in.
                let place_universe_id = place
                    .get("path")
                    .and_then(|v| v.as_str())
                    .and_then(parse_universe_id_from_place_path);
                if let Some(puid) = place_universe_id {
                    if puid != universe_id {
                        warn!(
                            "Skipping place {}: it belongs to experience {}, not {} - mismatched place.",
                            pid, puid, universe_id
                        );
                        continue;
                    }
                }
                let name = place
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<unnamed>");
                info!("  Importing place {} (\"{}\")", pid, name);
                remote_place_ids.push(*pid);
            }
            Err(e) => warn!(
                "Skipping place {}: not found in universe {} (or unreadable): {}",
                pid, universe_id, e
            ),
        }
    }

    // --- Build merged config ---
    let existing_universe = existing_config.as_ref().map(|c| &c.universe);
    let universe = UniverseConfig {
        id: universe_id,
        name: remote_name.clone(),
        description: remote_description.clone(),
        genre: existing_universe.and_then(|u| u.genre.clone()),
        playable_devices: existing_universe.and_then(|u| u.playable_devices.clone()),
        max_players: existing_universe.and_then(|u| u.max_players),
        private_server_cost: existing_universe.and_then(|u| u.private_server_cost.clone()),
    };

    // Game passes reconciliation.
    let mut game_passes: Vec<GamePassConfig> = Vec::new();
    let mut kept_passes: Vec<&GamePassConfig> = Vec::new();
    if let Some(cfg) = &existing_config {
        for local in &cfg.game_passes {
            let matched = remote_passes
                .iter()
                .any(|r| local_matches(local.id, &local.name, r));
            if !matched {
                game_passes.push(local.clone());
                kept_passes.push(local);
            }
        }
    }
    for r in &remote_passes {
        game_passes.push(GamePassConfig {
            id: Some(r.id),
            name: r.name.clone(),
            description: r.description.clone(),
            price: r.price.map(|p| p as u32),
            icon: None,
            is_for_sale: r.is_for_sale,
        });
    }

    // Developer products reconciliation.
    let mut developer_products: Vec<DeveloperProductConfig> = Vec::new();
    let mut kept_products: Vec<&DeveloperProductConfig> = Vec::new();
    if let Some(cfg) = &existing_config {
        for local in &cfg.developer_products {
            let matched = remote_products
                .iter()
                .any(|r| local_matches(local.id, &local.name, r));
            if !matched {
                developer_products.push(local.clone());
                kept_products.push(local);
            }
        }
    }
    for r in &remote_products {
        developer_products.push(DeveloperProductConfig {
            id: Some(r.id),
            name: r.name.clone(),
            description: r.description.clone(),
            price: r.price.unwrap_or(0) as u32,
            icon: None,
            is_active: None,
        });
    }

    // Badges reconciliation.
    let mut badges: Vec<BadgeConfig> = Vec::new();
    let mut kept_badges: Vec<&BadgeConfig> = Vec::new();
    if let Some(cfg) = &existing_config {
        for local in &cfg.badges {
            let matched = remote_badges
                .iter()
                .any(|r| local_matches(local.id, &local.name, r));
            if !matched {
                badges.push(local.clone());
                kept_badges.push(local);
            }
        }
    }
    for r in &remote_badges {
        badges.push(BadgeConfig {
            id: Some(r.id),
            name: r.name.clone(),
            description: r.description.clone(),
            icon: None,
            is_enabled: r.is_enabled,
        });
    }

    // Places: keep all existing, add remote-only with placeholder.
    let mut places: Vec<PlaceConfig> = existing_config
        .as_ref()
        .map(|c| c.places.clone())
        .unwrap_or_default();
    for pid in &remote_place_ids {
        if !places.iter().any(|p| p.place_id == *pid) {
            places.push(PlaceConfig {
                place_id: *pid,
                file_path: String::new(),
                publish: false,
            });
        }
    }
    if !remote_place_ids.is_empty() {
        warn!("The API key auto-discovers only the root place. Re-run with `--place-id <id>` (repeatable) to import additional places, or add them to the `places:` section manually. Set each place's `file_path` to publish it.");
    }

    let merged_config = RblxSyncConfig {
        assets_dir: existing_config
            .as_ref()
            .map(|c| c.assets_dir.clone())
            .unwrap_or_else(|| "assets".to_string()),
        creator: existing_config.as_ref().and_then(|c| c.creator.clone()),
        universe,
        game_passes,
        developer_products,
        badges,
        places,
        badge_payment_source: existing_config
            .as_ref()
            .and_then(|c| c.badge_payment_source.clone()),
        output_path: existing_config.as_ref().and_then(|c| c.output_path.clone()),
    };

    // --- Build merged state (rebuilt from scratch so stale entries drop) ---
    let mut merged_state = SyncState::default();
    let existing_universe_state = existing_state.universe.as_ref();
    merged_state.universe = Some(UniverseState {
        name: remote_name,
        description: remote_description,
        genre: existing_universe_state.and_then(|u| u.genre.clone()),
        playable_devices: existing_universe_state.and_then(|u| u.playable_devices.clone()),
        max_players: existing_universe_state.and_then(|u| u.max_players),
        private_server_cost: existing_universe_state.and_then(|u| u.private_server_cost.clone()),
    });

    for r in &remote_passes {
        merged_state.game_passes.insert(
            r.id,
            ResourceState {
                name: r.name.clone(),
                description: r.description.clone(),
                price: r.price,
                is_for_sale: r.is_for_sale,
                is_enabled: None,
                icon_hash: None,
                icon_asset_id: None,
            },
        );
    }
    for local in &kept_passes {
        if let Some((id, st)) = find_kept_state(&existing_state.game_passes, local.id, &local.name)
        {
            merged_state.game_passes.insert(id, st);
        }
    }

    for r in &remote_products {
        merged_state.developer_products.insert(
            r.id,
            ResourceState {
                name: r.name.clone(),
                description: r.description.clone(),
                price: r.price,
                is_for_sale: None,
                is_enabled: None,
                icon_hash: None,
                icon_asset_id: None,
            },
        );
    }
    for local in &kept_products {
        if let Some((id, st)) =
            find_kept_state(&existing_state.developer_products, local.id, &local.name)
        {
            merged_state.developer_products.insert(id, st);
        }
    }

    for r in &remote_badges {
        merged_state.badges.insert(
            r.id,
            ResourceState {
                name: r.name.clone(),
                description: r.description.clone(),
                price: None,
                is_for_sale: None,
                is_enabled: r.is_enabled,
                icon_hash: None,
                icon_asset_id: None,
            },
        );
    }
    for local in &kept_badges {
        if let Some((id, st)) = find_kept_state(&existing_state.badges, local.id, &local.name) {
            merged_state.badges.insert(id, st);
        }
    }

    // --- Backup + write ---
    let mut backup_name: Option<String> = None;
    if config_path.exists() {
        let backup = backup_path(config_path);
        std::fs::rename(config_path, &backup)
            .with_context(|| format!("Failed to back up config to {:?}", backup))?;
        backup_name = Some(backup.to_string_lossy().to_string());
    }

    let yaml =
        serde_yaml::to_string(&merged_config).context("Failed to serialize merged config")?;
    crate::fsutil::atomic_write(config_path, yaml.as_bytes())
        .with_context(|| format!("Failed to write config to {:?}", config_path))?;
    merged_state
        .save(root)
        .context("Failed to save lock file")?;

    // Regenerate the Luau output if the (carried-over) config requests it, so
    // the imported data is immediately usable in game code — same as `run`.
    if let Some(output_path) = &merged_config.output_path {
        output::generate_config(&merged_state, universe_id, output_path)?;
    }

    info!(
        "Import complete: {} game passes, {} developer products, {} badges, {} places.",
        remote_passes.len(),
        remote_products.len(),
        remote_badges.len(),
        remote_place_ids.len()
    );
    if let Some(name) = backup_name {
        info!("Previous config backed up to {}", name);
    }

    Ok(())
}

/// A normalized remote resource used during import reconciliation.
struct RemoteResource {
    id: u64,
    name: String,
    description: Option<String>,
    price: Option<u64>,
    is_for_sale: Option<bool>,
    is_enabled: Option<bool>,
}

/// Does a local config entry (with optional id + name) match a remote resource?
/// Match by id when the local entry has one, else by case-insensitive name.
fn local_matches(local_id: Option<u64>, local_name: &str, remote: &RemoteResource) -> bool {
    match local_id {
        Some(id) => id == remote.id,
        None => local_name.to_lowercase() == remote.name.to_lowercase(),
    }
}

/// Look up a kept local entry's existing state (by id if set, else by name) and
/// return the (id, state) to carry into the rebuilt lock file.
fn find_kept_state(
    map: &HashMap<u64, ResourceState>,
    local_id: Option<u64>,
    local_name: &str,
) -> Option<(u64, ResourceState)> {
    if let Some(id) = local_id {
        if let Some(st) = map.get(&id) {
            return Some((id, st.clone()));
        }
    }
    map.iter()
        .find(|(_, s)| s.name.to_lowercase() == local_name.to_lowercase())
        .map(|(id, s)| (*id, s.clone()))
}

async fn sync_universe_settings(
    universe_id: u64,
    config: &RblxSyncConfig,
    state: &mut SyncState,
    cookie_client: &RobloxCookieClient,
    dry_run: bool,
) -> Result<()> {
    info!("Syncing Universe Settings...");

    // Build the current desired state from config
    // Convert private_server_cost to state string for comparison
    let private_server_cost_state = config
        .universe
        .private_server_cost
        .as_ref()
        .map(|c| match c {
            PrivateServerCost::Disabled => "disabled".to_string(),
            PrivateServerCost::Free => "0".to_string(),
            PrivateServerCost::Paid(cost) => cost.to_string(),
        });

    let desired_state = UniverseState {
        name: config.universe.name.clone(),
        description: config.universe.description.clone(),
        genre: config.universe.genre.clone(),
        playable_devices: config.universe.playable_devices.clone(),
        max_players: config.universe.max_players,
        private_server_cost: private_server_cost_state.clone(),
    };

    // Check for diffs against stored state
    let stored_state = state.universe.as_ref();
    let mut changes: Vec<&str> = Vec::new();

    if stored_state.map(|s| &s.name) != Some(&desired_state.name) && desired_state.name.is_some() {
        changes.push("name");
    }
    if stored_state.map(|s| &s.description) != Some(&desired_state.description)
        && desired_state.description.is_some()
    {
        changes.push("description");
    }
    if stored_state.map(|s| &s.playable_devices) != Some(&desired_state.playable_devices)
        && desired_state.playable_devices.is_some()
    {
        changes.push("playable_devices");
    }
    if stored_state.map(|s| &s.private_server_cost) != Some(&desired_state.private_server_cost)
        && desired_state.private_server_cost.is_some()
    {
        changes.push("private_server_cost");
    }

    // genre and max_players are tracked locally only. They are not PATCHable via the
    // develop.roblox.com configuration endpoint (genre is documented as not updatable via
    // API; max_players is a per-place setting, not a universe configuration field), so they
    // must never trigger a sync on their own. We still persist them in state below.
    let stored_genre = stored_state.and_then(|s| s.genre.as_ref());
    let stored_max_players = stored_state.and_then(|s| s.max_players);
    let tracked_local_changed = (desired_state.genre.is_some()
        && stored_genre != desired_state.genre.as_ref())
        || (desired_state.max_players.is_some() && stored_max_players != desired_state.max_players);

    // Snapshot stored values as owned so they can be used after `state` is mutably borrowed.
    let stored_name = stored_state.and_then(|s| s.name.clone());
    let stored_description = stored_state.and_then(|s| s.description.clone());
    let stored_genre_owned = stored_state.and_then(|s| s.genre.clone());
    let stored_playable_devices = stored_state.and_then(|s| s.playable_devices.clone());
    let stored_max_players_owned = stored_state.and_then(|s| s.max_players);
    let stored_private_server_cost = stored_state.and_then(|s| s.private_server_cost.clone());

    let has_changes = !changes.is_empty();

    if !has_changes {
        info!("  [SKIP] Universe Settings - no API-updatable changes detected");
        if tracked_local_changed && !dry_run {
            info!("  [LOCAL] Universe genre/max_players are tracked locally only (not updatable via API) - recording in state");
            state.update_universe(
                stored_name.clone(),
                stored_description.clone(),
                desired_state
                    .genre
                    .clone()
                    .or_else(|| stored_genre_owned.clone()),
                stored_playable_devices.clone(),
                desired_state.max_players.or(stored_max_players_owned),
                stored_private_server_cost.clone(),
            );
        }
        return Ok(());
    }

    // Build the request body for develop.roblox.com/v2/universes/{id}/configuration
    let mut body = serde_json::Map::new();

    // Add fields that are changing
    if changes.contains(&"name") {
        if let Some(name) = &desired_state.name {
            body.insert("name".to_string(), name.clone().into());
        }
    }
    if changes.contains(&"description") {
        if let Some(desc) = &desired_state.description {
            body.insert("description".to_string(), desc.clone().into());
        }
    }

    // Map playable devices to numeric array (1=Computer, 2=Phone, 3=Tablet, 4=Console, 5=VR)
    if changes.contains(&"playable_devices") {
        if let Some(devices) = &desired_state.playable_devices {
            let device_ids: Vec<u8> = devices
                .iter()
                .filter_map(|d| match d.to_lowercase().as_str() {
                    "computer" => Some(1),
                    "phone" => Some(2),
                    "tablet" => Some(3),
                    "console" => Some(4),
                    "vr" => Some(5),
                    _ => None,
                })
                .collect();
            body.insert("playableDevices".to_string(), serde_json::json!(device_ids));
        }
    }

    // Handle private server cost
    if changes.contains(&"private_server_cost") {
        if let Some(cost) = &config.universe.private_server_cost {
            match cost {
                PrivateServerCost::Disabled => {
                    body.insert("allowPrivateServers".to_string(), serde_json::json!(false));
                }
                PrivateServerCost::Free => {
                    body.insert("allowPrivateServers".to_string(), serde_json::json!(true));
                    body.insert("privateServerPrice".to_string(), serde_json::json!(0));
                }
                PrivateServerCost::Paid(price) => {
                    body.insert("allowPrivateServers".to_string(), serde_json::json!(true));
                    body.insert("privateServerPrice".to_string(), serde_json::json!(price));
                }
            }
        }
    }

    if dry_run {
        info!(
            "  [UPDATE] Universe Settings - would update: {}",
            changes.join(", ")
        );
        info!(
            "  Dry Run: Would PATCH to https://develop.roblox.com/v2/universes/{}/configuration",
            universe_id
        );
    } else {
        info!(
            "  Request URL: https://develop.roblox.com/v2/universes/{}/configuration",
            universe_id
        );
        info!(
            "  Request Body: {}",
            serde_json::to_string_pretty(&serde_json::Value::Object(body.clone()))
                .unwrap_or_default()
        );
        let response = cookie_client
            .update_universe_configuration(universe_id, &serde_json::Value::Object(body))
            .await?;

        // Output raw response
        info!(
            "  Universe API Response: {}",
            serde_json::to_string_pretty(&response).unwrap_or_else(|_| response.to_string())
        );

        // Update state after successful sync. Only persist fields that were actually applied
        // (present in `changes`); for everything else, retain the previously stored value.
        // genre and max_players are tracked locally only, so carry the desired value when set.
        state.update_universe(
            if changes.contains(&"name") {
                desired_state.name.clone()
            } else {
                stored_name.clone()
            },
            if changes.contains(&"description") {
                desired_state.description.clone()
            } else {
                stored_description.clone()
            },
            desired_state
                .genre
                .clone()
                .or_else(|| stored_genre_owned.clone()),
            if changes.contains(&"playable_devices") {
                desired_state.playable_devices.clone()
            } else {
                stored_playable_devices.clone()
            },
            desired_state.max_players.or(stored_max_players_owned),
            if changes.contains(&"private_server_cost") {
                desired_state.private_server_cost.clone()
            } else {
                stored_private_server_cost.clone()
            },
        );

        info!(
            "  [UPDATED] Universe Settings - updated: {}",
            changes.join(", ")
        );
    }

    Ok(())
}

async fn sync_game_passes(
    universe_id: u64,
    config: &RblxSyncConfig,
    state: &mut SyncState,
    client: &RobloxClient,
    dry_run: bool,
    config_path: &Path,
) -> Result<()> {
    info!("Syncing Game Passes...");

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
                crate::api::ListResponse {
                    data: vec![],
                    next_page_cursor: None,
                }
            }
        }
    };

    let mut remote_map: HashMap<String, (String, u64)> = HashMap::new();
    for item in &existing.data {
        log::debug!("Game pass item from API: {}", item);
        let id = item["id"]
            .as_u64()
            .or_else(|| item["gamePassId"].as_u64())
            .or_else(|| item["id"].as_str().and_then(|s| s.parse().ok()))
            .or_else(|| item["gamePassId"].as_str().and_then(|s| s.parse().ok()));

        if let (Some(name), Some(id)) = (item["name"].as_str(), id) {
            log::debug!("Found game pass: {} with ID: {}", name, id);
            remote_map.insert(name.to_lowercase(), (name.to_string(), id));
        }
    }

    for pass in &config.game_passes {
        // Match precedence: an explicit `id` is authoritative (never create);
        // otherwise fall back to case-insensitive name matching.
        let state_lookup = match pass.id {
            Some(id) => state.game_passes.get(&id).map(|s| (id, s)),
            None => state.find_game_pass_by_name(&pass.name),
        };
        let state_entry = state_lookup.map(|(_, s)| s);
        // True when the entry has an explicit id but no lockfile state yet
        // (adopting a known remote resource): reconcile all configured fields.
        let id_adopt = pass.id.is_some() && state_entry.is_none();
        let mut did_create = false;
        let mut asset_id = None;
        let mut icon_hash = None;
        let mut icon_changed = false;
        let mut changes: Vec<&str> = Vec::new();

        // Check for metadata changes (name, description, price, is_for_sale)
        if let Some(entry) = state_entry {
            if entry.name != pass.name {
                changes.push("name");
            }
            if entry.description.as_ref() != pass.description.as_ref() {
                changes.push("description");
            }
            if entry.price != pass.price.map(|p| p as u64) {
                changes.push("price");
            }
            if entry.is_for_sale != pass.is_for_sale {
                changes.push("is_for_sale");
            }
        } else if id_adopt || remote_map.contains_key(&pass.name.to_lowercase()) {
            // Adopting a pre-existing remote resource with no state entry: we cannot know the
            // remote values, so reconcile by treating all configured fields as changes.
            changes.push("name");
            if pass.description.is_some() {
                changes.push("description");
            }
            if pass.price.is_some() {
                changes.push("price");
            }
            if pass.is_for_sale.is_some() {
                changes.push("is_for_sale");
            }
        }

        // Handle Icon - calculate hash and check for changes
        if let Some(icon_path_str) = &pass.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            let current_hash = calculate_file_hash(&icon_path).await?;
            let stored_hash = state_entry.and_then(|s| s.icon_hash.as_ref());

            if stored_hash == Some(&current_hash)
                && state_entry.and_then(|s| s.icon_asset_id).is_some()
            {
                asset_id = state_entry.and_then(|s| s.icon_asset_id);
                icon_hash = Some(current_hash);
                icon_changed = false;
            } else if dry_run {
                asset_id = Some(0);
                icon_hash = Some(current_hash);
                icon_changed = true;
                changes.push("icon");
            } else {
                let creator = config.creator.as_ref().ok_or_else(|| {
                    anyhow!("Creator configuration is required for asset uploads")
                })?;
                let (aid, hash) = ensure_icon(client, &icon_path, state_entry, creator).await?;
                asset_id = Some(aid);
                icon_hash = Some(hash);
                icon_changed = true;
                changes.push("icon");
            }
        }

        // Determine ID. An explicit config `id` is authoritative and never
        // creates; otherwise fall back to state-by-name -> remote-by-name ->
        // create (case-insensitive matching).
        let state_id = state_lookup.map(|(id, _)| id);
        let remote_entry = remote_map.get(&pass.name.to_lowercase());
        let is_new = pass.id.is_none() && state_id.is_none() && remote_entry.is_none();
        let has_changes = !changes.is_empty();

        let id = if let Some(cid) = pass.id {
            cid
        } else if let Some(sid) = state_id {
            sid
        } else if let Some((_, rid)) = remote_entry {
            *rid
        } else if dry_run {
            info!(
                "  [CREATE] Game Pass '{}' - would create with: name, description, price{}",
                pass.name,
                if pass.icon.is_some() { ", icon" } else { "" }
            );
            created_count += 1;
            0
        } else {
            let mut body = serde_json::json!({
                "name": pass.name,
                "description": pass.description.clone().unwrap_or_default(),
                "price": pass.price.unwrap_or(0),
            });
            if let Some(aid) = asset_id {
                body["iconAssetId"] = aid.into();
            }

            let resp = client.create_game_pass(universe_id, &body).await?;
            // Roblox's create-game-pass endpoint returns `gamePassId`, not `id`.
            // Without this fallback the resource is created on Roblox but
            // sync reports failure — the lock file misses the ID and the
            // next `rblxsync run` creates a duplicate under the same name.
            let new_id = resp["id"]
                .as_u64()
                .or_else(|| resp["gamePassId"].as_u64())
                .or_else(|| resp["gamePassId"].as_str().and_then(|s| s.parse().ok()))
                .ok_or_else(|| anyhow!("Created game pass has no ID. Response: {}", resp))?;
            info!(
                "  [CREATED] Game Pass '{}' (ID: {}) - created with: name, description, price{}",
                pass.name,
                new_id,
                if pass.icon.is_some() { ", icon" } else { "" }
            );
            created_count += 1;
            // Persist the new id into the yml right now (entries with an
            // explicit id never reach this branch), so a duplicate can never be
            // created even if a later step fails before state is saved.
            write_back_id(config_path, "game_passes", &pass.name, new_id)?;
            did_create = true;
            new_id
        };

        // Update Remote (Idempotent PATCH) - only if newly created or has changes
        if is_new {
            // Already created above
        } else if dry_run {
            if has_changes {
                info!(
                    "  [UPDATE] Game Pass '{}' (ID: {}) - would update: {}",
                    pass.name,
                    id,
                    changes.join(", ")
                );
                updated_count += 1;
            } else {
                info!(
                    "  [SKIP] Game Pass '{}' (ID: {}) - no changes detected",
                    pass.name, id
                );
                skipped_count += 1;
            }
        } else if has_changes {
            let mut patch = serde_json::Map::new();
            patch.insert("name".to_string(), pass.name.clone().into());
            if let Some(d) = &pass.description {
                patch.insert("description".to_string(), d.clone().into());
            }
            if let Some(p) = pass.price {
                patch.insert("price".to_string(), p.into());
            }
            if let Some(s) = pass.is_for_sale {
                patch.insert("isForSale".to_string(), s.into());
            }

            // Read image file if icon changed
            let image_data = if icon_changed {
                if let Some(icon_path_str) = &pass.icon {
                    let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
                    if icon_path.exists() {
                        let data = tokio::fs::read(&icon_path).await?;
                        let filename = icon_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        Some((data, filename))
                    } else {
                        warn!("Game pass icon not found: {:?}", icon_path);
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            client
                .update_game_pass_with_icon(
                    universe_id,
                    id,
                    &serde_json::Value::Object(patch),
                    image_data,
                )
                .await?;
            info!(
                "  [UPDATED] Game Pass '{}' (ID: {}) - updated: {}",
                pass.name,
                id,
                changes.join(", ")
            );
            updated_count += 1;
        } else {
            info!(
                "  [SKIP] Game Pass '{}' (ID: {}) - no changes detected",
                pass.name, id
            );
            skipped_count += 1;
        }

        // Update State after successful sync
        if !dry_run && id != 0 {
            state.update_game_pass(
                id,
                pass.name.clone(),
                pass.description.clone(),
                pass.price.map(|p| p as u64),
                pass.is_for_sale,
                icon_hash.clone(),
                asset_id,
            );
            // Persist the lock file as soon as a resource is created, so its
            // full state survives a later failure (the yml already has the id).
            if did_create {
                state
                    .save(lock_root(config_path))
                    .context("Failed to save lock file after create")?;
            }
        }
    }

    info!(
        "Game Passes Summary: {} created, {} updated, {} skipped (unchanged)",
        created_count, updated_count, skipped_count
    );
    Ok(())
}

async fn sync_developer_products(
    universe_id: u64,
    config: &RblxSyncConfig,
    state: &mut SyncState,
    client: &RobloxClient,
    dry_run: bool,
    config_path: &Path,
) -> Result<()> {
    info!("Syncing Developer Products...");

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
                crate::api::ListResponse {
                    data: vec![],
                    next_page_cursor: None,
                }
            }
        }
    };

    let mut remote_map: HashMap<String, (String, u64)> = HashMap::new();
    for item in &existing.data {
        log::debug!("Developer product item from API: {}", item);
        let id = item["id"]
            .as_u64()
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
        // Match precedence: an explicit `id` is authoritative (never create);
        // otherwise fall back to case-insensitive name matching.
        let state_lookup = match prod.id {
            Some(id) => state.developer_products.get(&id).map(|s| (id, s)),
            None => state.find_developer_product_by_name(&prod.name),
        };
        let state_entry = state_lookup.map(|(_, s)| s);
        let id_adopt = prod.id.is_some() && state_entry.is_none();
        let mut did_create = false;
        let mut asset_id = None;
        let mut icon_hash = None;
        let mut icon_changed = false;
        let mut changes: Vec<&str> = Vec::new();

        // Check for metadata changes (name, description, price)
        if let Some(entry) = state_entry {
            if entry.name != prod.name {
                changes.push("name");
            }
            if entry.description.as_ref() != prod.description.as_ref() {
                changes.push("description");
            }
            if entry.price != Some(prod.price as u64) {
                changes.push("price");
            }
        } else if id_adopt || remote_map.contains_key(&prod.name.to_lowercase()) {
            // Adopting a pre-existing remote resource with no state entry: reconcile all
            // configured fields with a single PATCH since remote values are unknown.
            changes.push("name");
            changes.push("price");
            if prod.description.is_some() {
                changes.push("description");
            }
        }

        if let Some(icon_path_str) = &prod.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            let current_hash = calculate_file_hash(&icon_path).await?;
            let stored_hash = state_entry.and_then(|s| s.icon_hash.as_ref());

            if stored_hash == Some(&current_hash)
                && state_entry.and_then(|s| s.icon_asset_id).is_some()
            {
                asset_id = state_entry.and_then(|s| s.icon_asset_id);
                icon_hash = Some(current_hash);
                icon_changed = false;
            } else if dry_run {
                asset_id = Some(0);
                icon_hash = Some(current_hash);
                icon_changed = true;
                changes.push("icon");
            } else {
                let creator = config.creator.as_ref().ok_or_else(|| {
                    anyhow!("Creator configuration is required for asset uploads")
                })?;
                let (aid, hash) = ensure_icon(client, &icon_path, state_entry, creator).await?;
                asset_id = Some(aid);
                icon_hash = Some(hash);
                icon_changed = true;
                changes.push("icon");
            }
        }

        // Determine ID. An explicit config `id` is authoritative and never
        // creates; otherwise state-by-name -> remote-by-name -> create.
        let state_id = state_lookup.map(|(id, _)| id);
        let remote_entry = remote_map.get(&prod.name.to_lowercase());
        let is_new = prod.id.is_none() && state_id.is_none() && remote_entry.is_none();
        let has_changes = !changes.is_empty();

        let id = if let Some(cid) = prod.id {
            cid
        } else if let Some(sid) = state_id {
            sid
        } else if let Some((_, rid)) = remote_entry {
            *rid
        } else if dry_run {
            info!(
                "  [CREATE] Developer Product '{}' - would create with: name, price, description{}",
                prod.name,
                if prod.icon.is_some() { ", icon" } else { "" }
            );
            created_count += 1;
            0
        } else {
            let mut body = serde_json::json!({
                "name": prod.name,
                "price": prod.price,
                "description": prod.description.clone().unwrap_or_default(),
            });
            if let Some(aid) = asset_id {
                body["iconAssetId"] = aid.into();
            }
            let resp = client.create_developer_product(universe_id, &body).await?;
            // Same as game-pass create: the endpoint returns `productId`.
            // Legacy field shapes covered for resilience.
            let new_id = resp["id"]
                .as_u64()
                .or_else(|| resp["productId"].as_u64())
                .or_else(|| resp["developerProductId"].as_u64())
                .or_else(|| resp["ProductId"].as_u64())
                .ok_or_else(|| anyhow!("Created product has no ID. Response: {}", resp))?;
            info!("  [CREATED] Developer Product '{}' (ID: {}) - created with: name, price, description{}",
                prod.name, new_id,
                if prod.icon.is_some() { ", icon" } else { "" });
            created_count += 1;
            write_back_id(config_path, "developer_products", &prod.name, new_id)?;
            did_create = true;
            new_id
        };

        // Update Remote (Idempotent PATCH) - only if has changes
        if is_new {
            // Already created above
        } else if dry_run {
            if has_changes {
                info!(
                    "  [UPDATE] Developer Product '{}' (ID: {}) - would update: {}",
                    prod.name,
                    id,
                    changes.join(", ")
                );
                updated_count += 1;
            } else {
                info!(
                    "  [SKIP] Developer Product '{}' (ID: {}) - no changes detected",
                    prod.name, id
                );
                skipped_count += 1;
            }
        } else if has_changes {
            let mut patch = serde_json::Map::new();
            patch.insert("name".to_string(), prod.name.clone().into());
            patch.insert("price".to_string(), prod.price.into());
            if let Some(d) = &prod.description {
                patch.insert("description".to_string(), d.clone().into());
            }

            // Read image file if icon changed
            let image_data = if icon_changed {
                if let Some(icon_path_str) = &prod.icon {
                    let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
                    if icon_path.exists() {
                        let data = tokio::fs::read(&icon_path).await?;
                        let filename = icon_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        Some((data, filename))
                    } else {
                        warn!("Developer product icon not found: {:?}", icon_path);
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            client
                .update_developer_product_with_icon(
                    universe_id,
                    id,
                    &serde_json::Value::Object(patch),
                    image_data,
                )
                .await?;
            info!(
                "  [UPDATED] Developer Product '{}' (ID: {}) - updated: {}",
                prod.name,
                id,
                changes.join(", ")
            );
            updated_count += 1;
        } else {
            info!(
                "  [SKIP] Developer Product '{}' (ID: {}) - no changes detected",
                prod.name, id
            );
            skipped_count += 1;
        }

        // Update State after successful sync
        if !dry_run && id != 0 {
            state.update_developer_product(
                id,
                prod.name.clone(),
                prod.description.clone(),
                Some(prod.price as u64),
                icon_hash,
                asset_id,
            );
            if did_create {
                state
                    .save(lock_root(config_path))
                    .context("Failed to save lock file after create")?;
            }
        }
    }

    info!(
        "Developer Products Summary: {} created, {} updated, {} skipped (unchanged)",
        created_count, updated_count, skipped_count
    );
    Ok(())
}

async fn sync_badges(
    universe_id: u64,
    config: &RblxSyncConfig,
    state: &mut SyncState,
    client: &RobloxClient,
    dry_run: bool,
    config_path: &Path,
) -> Result<()> {
    info!("Syncing Badges...");

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
                crate::api::ListResponse {
                    data: vec![],
                    next_page_cursor: None,
                }
            }
        }
    };

    let mut remote_map: HashMap<String, (String, u64)> = HashMap::new();
    for item in existing.data {
        if let (Some(name), Some(id)) = (item["name"].as_str(), item["id"].as_u64()) {
            remote_map.insert(name.to_lowercase(), (name.to_string(), id));
        }
    }
    log::debug!(
        "Badges: {} found remotely: {:?}",
        remote_map.len(),
        remote_map
            .values()
            .map(|(n, id)| (n, id))
            .collect::<Vec<_>>()
    );

    for badge in &config.badges {
        // Match precedence: an explicit `id` is authoritative (never create);
        // otherwise fall back to case-insensitive name matching.
        let state_lookup = match badge.id {
            Some(id) => state.badges.get(&id).map(|s| (id, s)),
            None => state.find_badge_by_name(&badge.name),
        };
        let state_entry = state_lookup.map(|(_, s)| s);
        let id_adopt = badge.id.is_some() && state_entry.is_none();
        let mut did_create = false;
        let mut changes: Vec<&str> = Vec::new();

        // Check for metadata changes (name, description, is_enabled)
        if let Some(entry) = state_entry {
            if entry.name != badge.name {
                changes.push("name");
            }
            if entry.description.as_ref() != badge.description.as_ref() {
                changes.push("description");
            }
            if entry.is_enabled != badge.is_enabled {
                changes.push("is_enabled");
            }
        } else if id_adopt || remote_map.contains_key(&badge.name.to_lowercase()) {
            // Adopting a pre-existing remote resource with no state entry: reconcile all
            // configured fields with a single PATCH since remote values are unknown.
            changes.push("name");
            if badge.description.is_some() {
                changes.push("description");
            }
            if badge.is_enabled.is_some() {
                changes.push("is_enabled");
            }
        }

        // Prepare icon data if provided
        let icon_data = if let Some(icon_path_str) = &badge.icon {
            let icon_path = Path::new(&config.assets_dir).join(icon_path_str);
            if icon_path.exists() {
                let data = tokio::fs::read(&icon_path).await?;
                let filename = icon_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

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

        // Determine ID. An explicit config `id` is authoritative and never
        // creates; otherwise state-by-name -> remote-by-name -> create.
        let state_id = state_lookup.map(|(id, _)| id);
        let remote_entry = remote_map.get(&badge.name.to_lowercase());
        let is_new = badge.id.is_none() && state_id.is_none() && remote_entry.is_none();
        let has_changes = !changes.is_empty();
        log::debug!(
            "Badge match '{}' (lowercased key {:?}): config_id={:?} lock_match={} remote_match={} -> {}",
            badge.name,
            badge.name.to_lowercase(),
            badge.id,
            state_id.is_some(),
            remote_entry.is_some(),
            if is_new { "CREATE" } else { "adopt/patch" }
        );

        let id = if let Some(cid) = badge.id {
            cid
        } else if let Some(sid) = state_id {
            sid
        } else if let Some((_, rid)) = remote_entry {
            *rid
        } else if dry_run {
            info!(
                "  [CREATE] Badge '{}' - would create with: name, description{}",
                badge.name,
                if badge.icon.is_some() { ", icon" } else { "" }
            );
            created_count += 1;
            0
        } else {
            // Roblox requires an icon to CREATE a badge: the legacy create
            // endpoint rejects a missing/empty image with code 11 ("The badge
            // icon is invalid."). Fail early with a clear, badge-named message
            // instead of forwarding the cryptic 400.
            if icon_data.is_none() {
                return Err(anyhow!(
                    "Badge '{}' cannot be created without an icon - Roblox requires one. \
                     Set its `icon:` to an existing image file under assets_dir ('{}').",
                    badge.name,
                    config.assets_dir
                ));
            }

            let image_for_create = icon_data
                .as_ref()
                .map(|(data, filename, _)| (data.clone(), filename.clone()));

            let result = client
                .create_badge(
                    universe_id,
                    &badge.name,
                    badge.description.as_deref().unwrap_or(""),
                    image_for_create,
                    config.badge_payment_source.as_deref(),
                )
                .await;

            let resp = match result {
                Ok(r) => r,
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("Payment source is invalid")
                        || err_str.contains("code\":16")
                    {
                        error!(
                            "Badge '{}' creation failed: Payment source is required.",
                            badge.name
                        );
                        error!("");
                        error!("Creating badges costs 100 Robux. Please add the following to your rblxsync.yml:");
                        error!("");
                        error!("  badge_payment_source: \"user\"   # Pay from your user account");
                        error!("  # OR");
                        error!("  badge_payment_source: \"group\"  # Pay from group funds");
                        error!("");
                        return Err(anyhow!(
                            "Badge '{}' creation requires badge_payment_source configuration",
                            badge.name
                        ));
                    }
                    if err_str.contains("badge icon is invalid") || err_str.contains("code\":11") {
                        return Err(anyhow!(
                            "Badge '{}' creation failed: Roblox rejected the icon. Provide a valid \
                             square image (PNG/JPEG, e.g. 512x512) via its `icon:` field. ({})",
                            badge.name,
                            e
                        ));
                    }
                    return Err(anyhow!("Badge '{}' creation failed: {}", badge.name, e));
                }
            };

            // Badge create response shapes: `id` historically; legacy badge
            // upload paths return `badgeId` or `assetId`. Covering all so
            // future Roblox changes don't silently leak the create.
            let new_id = resp["id"]
                .as_u64()
                .or_else(|| resp["badgeId"].as_u64())
                .or_else(|| resp["assetId"].as_u64())
                .ok_or_else(|| anyhow!("Created badge has no ID. Response: {}", resp))?;
            info!(
                "  [CREATED] Badge '{}' (ID: {}) - created with: name, description{}",
                badge.name,
                new_id,
                if badge.icon.is_some() { ", icon" } else { "" }
            );
            created_count += 1;
            // Roblox's badge create endpoint 500s when called back-to-back
            // while it's still processing the per-badge 100 Robux charge
            // and asset commits server-side. A short breather between
            // creates lets the loop keep moving instead of failing on
            // every-other badge and requiring multiple sync reruns to
            // get through a wave.
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            write_back_id(config_path, "badges", &badge.name, new_id)?;
            did_create = true;
            new_id
        };

        // Update state with icon hash
        let icon_hash = icon_data.as_ref().map(|(_, _, hash)| hash.clone());

        // Update Remote (Idempotent PATCH) - only if has changes
        if is_new {
            // Already created above
        } else if dry_run {
            if has_changes {
                info!(
                    "  [UPDATE] Badge '{}' (ID: {}) - would update: {}",
                    badge.name,
                    id,
                    changes.join(", ")
                );
                updated_count += 1;
            } else {
                info!(
                    "  [SKIP] Badge '{}' (ID: {}) - no changes detected",
                    badge.name, id
                );
                skipped_count += 1;
            }
        } else if has_changes {
            let mut patch = serde_json::Map::new();
            patch.insert("name".to_string(), badge.name.clone().into());
            if let Some(d) = &badge.description {
                patch.insert("description".to_string(), d.clone().into());
            }
            if let Some(e) = badge.is_enabled {
                patch.insert("enabled".to_string(), e.into());
            }

            client
                .update_badge(id, &serde_json::Value::Object(patch))
                .await?;

            // Update icon if it changed
            if icon_changed {
                if let Some((data, filename, _)) = &icon_data {
                    client.update_badge_icon(id, data.clone(), filename).await?;
                }
            }
            info!(
                "  [UPDATED] Badge '{}' (ID: {}) - updated: {}",
                badge.name,
                id,
                changes.join(", ")
            );
            updated_count += 1;
        } else {
            info!(
                "  [SKIP] Badge '{}' (ID: {}) - no changes detected",
                badge.name, id
            );
            skipped_count += 1;
        }

        // Update State after successful sync
        if !dry_run && id != 0 {
            state.update_badge(
                id,
                badge.name.clone(),
                badge.description.clone(),
                badge.is_enabled,
                icon_hash.clone(),
                None,
            );
            if did_create {
                state
                    .save(lock_root(config_path))
                    .context("Failed to save lock file after create")?;
            }
        }
    }

    info!(
        "Badges Summary: {} created, {} updated, {} skipped (unchanged)",
        created_count, updated_count, skipped_count
    );
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

async fn ensure_icon(
    client: &RobloxClient,
    path: &Path,
    state: Option<&ResourceState>,
    creator: &crate::config::CreatorConfig,
) -> Result<(u64, String)> {
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

/// Escape special characters for embedding in a Luau/Lua string literal.
/// Mirrors `escape_luau_string` in `src/output.rs`.
fn escape_lua_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub async fn export(
    config: RblxSyncConfig,
    client: RobloxClient,
    output: Option<String>,
    format_lua: bool,
) -> Result<()> {
    let universe_id = config.universe.id;

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
        if let Some(n) = item["name"].as_str() {
            lua.push_str(&format!("      name = \"{}\",\n", escape_lua_string(n)));
        }
        if let Some(id) = item["id"].as_u64() {
            lua.push_str(&format!("      id = {},\n", id));
        }
        if let Some(p) = item["price"].as_u64() {
            lua.push_str(&format!("      price = {},\n", p));
        }
        lua.push_str("    },\n");
    }
    lua.push_str("  },\n");

    lua.push_str("  developer_products = {\n");
    for item in products.data {
        lua.push_str("    {\n");
        if let Some(n) = item["name"].as_str() {
            lua.push_str(&format!("      name = \"{}\",\n", escape_lua_string(n)));
        }
        if let Some(id) = item["id"].as_u64() {
            lua.push_str(&format!("      id = {},\n", id));
        }
        if let Some(p) = item["price"].as_u64() {
            lua.push_str(&format!("      price = {},\n", p));
        }
        lua.push_str("    },\n");
    }
    lua.push_str("  },\n");

    lua.push_str("  badges = {\n");
    for item in badges.data {
        lua.push_str("    {\n");
        if let Some(n) = item["name"].as_str() {
            lua.push_str(&format!("      name = \"{}\",\n", escape_lua_string(n)));
        }
        if let Some(id) = item["id"].as_u64() {
            lua.push_str(&format!("      id = {},\n", id));
        }
        lua.push_str("    },\n");
    }
    lua.push_str("  },\n");

    lua.push_str("}\n");

    let out_path = output.unwrap_or_else(|| {
        if format_lua {
            "config.lua".to_string()
        } else {
            "config.luau".to_string()
        }
    });
    std::fs::write(&out_path, lua)?;
    info!("Exported to {}", out_path);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BadgeConfig, GamePassConfig, UniverseConfig};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> RobloxClient {
        RobloxClient::with_base_url("test-key".to_string(), server.uri())
    }

    fn base_config() -> RblxSyncConfig {
        RblxSyncConfig {
            assets_dir: "assets".to_string(),
            creator: None,
            universe: UniverseConfig {
                id: 1,
                name: None,
                description: None,
                genre: None,
                playable_devices: None,
                max_players: None,
                private_server_cost: None,
            },
            game_passes: vec![],
            developer_products: vec![],
            badges: vec![],
            places: vec![],
            badge_payment_source: None,
            output_path: None,
        }
    }

    fn game_pass(name: &str, price: Option<u32>) -> GamePassConfig {
        GamePassConfig {
            id: None,
            name: name.to_string(),
            description: None,
            price,
            icon: None,
            is_for_sale: None,
        }
    }

    /// Empty game-passes list endpoint (so list_game_passes returns no remote entries).
    async fn mount_empty_game_pass_list(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [], "nextPageCursor": null})),
            )
            .mount(server)
            .await;
    }

    // (a) Adopting a remotely-existing game pass with no state entry triggers a
    // reconciling PATCH (not SKIP).
    #[tokio::test]
    async fn adopt_remote_game_pass_triggers_patch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"data": [{"id": 42, "name": "VIP Pass"}], "nextPageCursor": null}),
            ))
            .mount(&server)
            .await;
        let patch = Mock::given(method("PATCH"))
            .and(path("/game-passes/v1/universes/1/game-passes/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 42})))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let mut config = base_config();
        config.game_passes = vec![game_pass("VIP Pass", Some(100))];
        let mut state = SyncState::default();

        sync_game_passes(
            1,
            &config,
            &mut state,
            &client(&server),
            false,
            Path::new("/nonexistent/rblxsync.yml"),
        )
        .await
        .unwrap();

        // The PATCH must have happened, and state now records the adopted ID.
        drop(patch);
        assert!(state.find_game_pass_by_name("VIP Pass").is_some());
        assert_eq!(state.find_game_pass_by_name("VIP Pass").unwrap().0, 42);
    }

    // (b) A no-change run (state already matches config) produces no PATCH.
    #[tokio::test]
    async fn no_change_game_pass_does_not_patch() {
        let server = MockServer::start().await;
        mount_empty_game_pass_list(&server).await;
        // Any PATCH would 500 and fail the run.
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let mut config = base_config();
        config.game_passes = vec![game_pass("VIP Pass", Some(100))];
        let mut state = SyncState::default();
        // Seed state to exactly match config.
        state.update_game_pass(7, "VIP Pass".to_string(), None, Some(100), None, None, None);

        sync_game_passes(
            1,
            &config,
            &mut state,
            &client(&server),
            false,
            Path::new("/nonexistent/rblxsync.yml"),
        )
        .await
        .unwrap();
    }

    // (c) genre/max_players-only universe config does not PATCH but persists to state.
    #[tokio::test]
    async fn universe_genre_max_players_only_persists_without_patch() {
        let cookie_client = RobloxCookieClient::new("fake-cookie".to_string());
        let mut config = base_config();
        config.universe.genre = Some("adventure".to_string());
        config.universe.max_players = Some(40);
        let mut state = SyncState::default();

        // No HTTP mock is set up; if it tried to PATCH develop.roblox.com it would error.
        sync_universe_settings(1, &config, &mut state, &cookie_client, false)
            .await
            .unwrap();

        let u = state.universe.as_ref().unwrap();
        assert_eq!(u.genre.as_deref(), Some("adventure"));
        assert_eq!(u.max_players, Some(40));
    }

    // (d) export escapes quotes and backslashes in resource names.
    #[tokio::test]
    async fn export_escapes_quotes_and_backslashes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [{"id": 1, "name": "Quote\"And\\Slash", "price": 5}],
                "nextPageCursor": null
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/developer-products/v2/universes/1/developer-products/creator",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"developerProducts": [], "nextPageToken": null})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"badges": [], "nextPageCursor": null})),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("Config.luau");
        let mut config = base_config();
        config.universe.id = 1;

        export(
            config,
            client(&server),
            Some(out.to_string_lossy().to_string()),
            false,
        )
        .await
        .unwrap();

        let contents = std::fs::read_to_string(&out).unwrap();
        assert!(
            contents.contains(r#"name = "Quote\"And\\Slash","#),
            "contents: {}",
            contents
        );
    }

    // (e) dry-run makes no mutating HTTP calls (only the GET list, no PATCH/POST).
    #[tokio::test]
    async fn dry_run_makes_no_mutating_calls() {
        let server = MockServer::start().await;
        // List badges (the only call dry-run should make).
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"badges": [{"id": 9, "name": "First Win"}], "nextPageCursor": null}),
            ))
            .mount(&server)
            .await;
        // Any mutating verb fails the test.
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let mut config = base_config();
        config.badges = vec![BadgeConfig {
            id: None,
            name: "First Win".to_string(),
            description: Some("changed".to_string()),
            icon: None,
            is_enabled: Some(true),
        }];
        let mut state = SyncState::default();

        sync_badges(
            1,
            &config,
            &mut state,
            &client(&server),
            true,
            Path::new("/nonexistent/rblxsync.yml"),
        )
        .await
        .unwrap();

        // Dry run must not persist state.
        assert!(state.badges.is_empty());
    }

    // Creating a new badge without an icon fails early with a clear message that
    // names the badge (Roblox requires an icon to create a badge).
    #[tokio::test]
    async fn create_badge_without_icon_errors_with_badge_name() {
        let server = MockServer::start().await;
        // Empty badge list -> the badge is treated as new (create path).
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"badges": [], "nextPageCursor": null})),
            )
            .mount(&server)
            .await;
        // A create POST must never be reached; if it is, fail loudly.
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let mut config = base_config();
        config.badges = vec![BadgeConfig {
            id: None,
            name: "No Icon Badge".to_string(),
            description: None,
            icon: None,
            is_enabled: None,
        }];
        let mut state = SyncState::default();

        let err = sync_badges(
            1,
            &config,
            &mut state,
            &client(&server),
            false,
            Path::new("/nonexistent/rblxsync.yml"),
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("No Icon Badge"),
            "msg should name the badge: {}",
            msg
        );
        assert!(
            msg.to_lowercase().contains("icon"),
            "msg should mention the icon requirement: {}",
            msg
        );
    }

    // Preflight aborts before any mutation when a to-be-created badge has no icon.
    #[tokio::test]
    async fn preflight_flags_new_badge_without_icon() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"badges": [], "nextPageCursor": null})),
            )
            .mount(&server)
            .await;

        let mut config = base_config();
        config.badge_payment_source = Some("user".to_string());
        config.badges = vec![BadgeConfig {
            id: None,
            name: "Newbie".to_string(),
            description: None,
            icon: None,
            is_enabled: None,
        }];
        let state = SyncState::default();

        let err = preflight(1, &config, &state, &client(&server), false)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Newbie"), "msg: {}", msg);
        assert!(msg.to_lowercase().contains("icon"), "msg: {}", msg);
    }

    // Preflight flags a pass/product icon when no creator is configured.
    #[tokio::test]
    async fn preflight_flags_icon_without_creator() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"badges": [], "nextPageCursor": null})),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vip.png"), b"img").unwrap();
        let mut config = base_config();
        config.assets_dir = dir.path().to_string_lossy().to_string();
        config.creator = None;
        config.game_passes = vec![GamePassConfig {
            id: None,
            name: "VIP".to_string(),
            description: None,
            price: Some(100),
            icon: Some("vip.png".to_string()),
            is_for_sale: None,
        }];
        let state = SyncState::default();

        let err = preflight(1, &config, &state, &client(&server), false)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("VIP"), "msg: {}", msg);
        assert!(msg.to_lowercase().contains("creator"), "msg: {}", msg);
    }

    // Preflight passes for a fully-valid config (no mutation needed).
    #[tokio::test]
    async fn preflight_passes_for_valid_config() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"badges": [], "nextPageCursor": null})),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vip.png"), b"img").unwrap();
        std::fs::write(dir.path().join("badge.png"), b"img").unwrap();
        let mut config = base_config();
        config.assets_dir = dir.path().to_string_lossy().to_string();
        config.creator = Some(crate::config::CreatorConfig {
            id: "1".to_string(),
            creator_type: "user".to_string(),
        });
        config.badge_payment_source = Some("user".to_string());
        config.game_passes = vec![GamePassConfig {
            id: None,
            name: "VIP".to_string(),
            description: None,
            price: Some(100),
            icon: Some("vip.png".to_string()),
            is_for_sale: None,
        }];
        config.badges = vec![BadgeConfig {
            id: None,
            name: "Win".to_string(),
            description: None,
            icon: Some("badge.png".to_string()),
            is_enabled: None,
        }];
        let state = SyncState::default();

        preflight(1, &config, &state, &client(&server), false)
            .await
            .unwrap();
    }

    // --- import command tests ---

    /// Mount a minimal but complete remote-universe surface for import.
    /// `passes` / `products` / `badges` are the list payloads; `detail` maps a
    /// game-pass id to its `/creator` detail body.
    async fn mount_universe(
        server: &MockServer,
        root_place: Option<&str>,
        passes: serde_json::Value,
        pass_detail: Vec<(u64, serde_json::Value)>,
        products: serde_json::Value,
        badges: serde_json::Value,
    ) {
        let mut universe = json!({
            "displayName": "Imported Game",
            "description": "Imported description"
        });
        if let Some(rp) = root_place {
            universe["rootPlace"] = json!(rp);
        }
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(universe))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(passes))
            .mount(server)
            .await;
        for (id, body) in pass_detail {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/game-passes/v1/universes/1/game-passes/{}/creator",
                    id
                )))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path(
                "/developer-products/v2/universes/1/developer-products/creator",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(products))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(ResponseTemplate::new(200).set_body_json(badges))
            .mount(server)
            .await;
    }

    // Remote entry overwrites a differing local entry in both yml and lockfile.
    #[tokio::test]
    async fn import_remote_overwrites_local() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            None,
            json!({"data": [{"id": 42, "name": "VIP Pass"}], "nextPageCursor": null}),
            vec![(
                42,
                json!({
                    "gamePassId": 42,
                    "name": "VIP Pass",
                    "description": "Remote desc",
                    "isForSale": true,
                    "priceInformation": {"defaultPriceInRobux": 200}
                }),
            )],
            json!({"developerProducts": [], "nextPageToken": null}),
            json!({"badges": [], "nextPageCursor": null}),
        )
        .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        std::fs::write(
            &cfg,
            "universe:\n  id: 1\ngame_passes:\n  - name: VIP Pass\n    price: 50\n",
        )
        .unwrap();

        import(client(&server), &cfg, None, vec![]).await.unwrap();

        let written = std::fs::read_to_string(&cfg).unwrap();
        let parsed: RblxSyncConfig = serde_yaml::from_str(&written).unwrap();
        assert_eq!(parsed.game_passes.len(), 1);
        assert_eq!(parsed.game_passes[0].id, Some(42));
        assert_eq!(parsed.game_passes[0].price, Some(200));

        let state = SyncState::load(dir.path()).unwrap();
        assert_eq!(state.game_passes.get(&42).unwrap().price, Some(200));
    }

    // A local-only yml entry (not on remote) is preserved.
    #[tokio::test]
    async fn import_preserves_local_only_entry() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            None,
            json!({"data": [], "nextPageCursor": null}),
            vec![],
            json!({"developerProducts": [], "nextPageToken": null}),
            json!({"badges": [], "nextPageCursor": null}),
        )
        .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        std::fs::write(
            &cfg,
            "universe:\n  id: 1\ngame_passes:\n  - name: Local Only\n    price: 25\n",
        )
        .unwrap();

        import(client(&server), &cfg, None, vec![]).await.unwrap();

        let parsed: RblxSyncConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(parsed.game_passes.len(), 1);
        assert_eq!(parsed.game_passes[0].name, "Local Only");
        assert_eq!(parsed.game_passes[0].id, None);
    }

    // A lockfile-only stale entry (not in yml, not remote) is dropped.
    #[tokio::test]
    async fn import_drops_stale_lockfile_entry() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            None,
            json!({"data": [], "nextPageCursor": null}),
            vec![],
            json!({"developerProducts": [], "nextPageToken": null}),
            json!({"badges": [], "nextPageCursor": null}),
        )
        .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        std::fs::write(&cfg, "universe:\n  id: 1\n").unwrap();
        // Seed a stale lock entry not present in yml or remote.
        let mut state = SyncState::default();
        state.update_game_pass(999, "Stale".to_string(), None, Some(10), None, None, None);
        state.save(dir.path()).unwrap();

        import(client(&server), &cfg, None, vec![]).await.unwrap();

        let new_state = SyncState::load(dir.path()).unwrap();
        assert!(!new_state.game_passes.contains_key(&999));
        assert!(new_state.game_passes.is_empty());
    }

    // Backup rename picks `.old`, then `.old1` when `.old` already exists.
    #[tokio::test]
    async fn import_backup_rename_increments() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            None,
            json!({"data": [], "nextPageCursor": null}),
            vec![],
            json!({"developerProducts": [], "nextPageToken": null}),
            json!({"badges": [], "nextPageCursor": null}),
        )
        .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        std::fs::write(&cfg, "universe:\n  id: 1\n").unwrap();

        // First import -> creates rblxsync.old.yml
        import(client(&server), &cfg, None, vec![]).await.unwrap();
        assert!(dir.path().join("rblxsync.old.yml").exists());

        // Second import -> .old taken, so .old1.yml
        import(client(&server), &cfg, None, vec![]).await.unwrap();
        assert!(dir.path().join("rblxsync.old1.yml").exists());
    }

    // The written yml contains `id:` for imported passes / products / badges.
    #[tokio::test]
    async fn import_writes_ids_for_all_types() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            None,
            json!({"data": [{"id": 11, "name": "Pass A"}], "nextPageCursor": null}),
            vec![(
                11,
                json!({"gamePassId": 11, "name": "Pass A", "priceInformation": {"defaultPriceInRobux": 100}}),
            )],
            json!({"developerProducts": [{"productId": 22, "name": "Prod B", "priceInformation": {"defaultPriceInRobux": 50}}], "nextPageToken": null}),
            json!({"badges": [{"id": 33, "name": "Badge C", "enabled": true}], "nextPageCursor": null}),
        )
        .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");

        import(client(&server), &cfg, Some(1), vec![])
            .await
            .unwrap();

        let parsed: RblxSyncConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(parsed.game_passes[0].id, Some(11));
        assert_eq!(parsed.developer_products[0].id, Some(22));
        assert_eq!(parsed.badges[0].id, Some(33));
    }

    // Root place imported with placeholder; existing place file_path preserved.
    #[tokio::test]
    async fn import_places_root_and_preserve_existing() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            Some("universes/1/places/777"),
            json!({"data": [], "nextPageCursor": null}),
            vec![],
            json!({"developerProducts": [], "nextPageToken": null}),
            json!({"badges": [], "nextPageCursor": null}),
        )
        .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        std::fs::write(
            &cfg,
            "universe:\n  id: 1\nplaces:\n  - place_id: 500\n    file_path: places/main.rbxl\n    publish: true\n",
        )
        .unwrap();

        import(client(&server), &cfg, None, vec![]).await.unwrap();

        let parsed: RblxSyncConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        let existing = parsed.places.iter().find(|p| p.place_id == 500).unwrap();
        assert_eq!(existing.file_path, "places/main.rbxl");
        assert!(existing.publish);
        let root = parsed.places.iter().find(|p| p.place_id == 777).unwrap();
        assert_eq!(root.file_path, "");
        assert!(!root.publish);
    }

    // Import regenerates the Luau output when output_path is set.
    #[tokio::test]
    async fn import_generates_output_when_output_path_set() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            None,
            json!({"data": [{"id": 11, "name": "Pass A"}], "nextPageCursor": null}),
            vec![(
                11,
                json!({"gamePassId": 11, "name": "Pass A", "priceInformation": {"defaultPriceInRobux": 100}}),
            )],
            json!({"developerProducts": [], "nextPageToken": null}),
            json!({"badges": [], "nextPageCursor": null}),
        )
        .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        let out = dir.path().join("Config.luau");
        std::fs::write(
            &cfg,
            format!(
                "universe:\n  id: 1\noutput_path: \"{}\"\n",
                out.to_string_lossy()
            ),
        )
        .unwrap();

        import(client(&server), &cfg, None, vec![]).await.unwrap();

        assert!(out.exists(), "Config.luau should be generated by import");
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("Id = 1"), "universe id: {}", content);
        assert!(content.contains("Id = 11"), "game pass id: {}", content);
        assert!(content.contains("Pass A"), "game pass name: {}", content);
    }

    // --place-id: a valid extra place is fetched + imported; an invalid one is
    // skipped (warned) without aborting the import.
    #[tokio::test]
    async fn import_place_id_flag_validates_and_skips_bad() {
        let server = MockServer::start().await;
        mount_universe(
            &server,
            Some("universes/1/places/777"),
            json!({"data": [], "nextPageCursor": null}),
            vec![],
            json!({"developerProducts": [], "nextPageToken": null}),
            json!({"badges": [], "nextPageCursor": null}),
        )
        .await;
        // Valid extra place 888 resolves; bogus 999 returns 404 and is skipped.
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1/places/888"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"displayName": "Lobby", "path": "universes/1/places/888"}),
                ),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1/places/999"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;
        // Place 555 returns 200 but its path reports a DIFFERENT experience -
        // the safety check must skip it even though the fetch succeeded.
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1/places/555"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"displayName": "Wrong Game", "path": "universes/2/places/555"}),
            ))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        std::fs::write(&cfg, "universe:\n  id: 1\n").unwrap();

        import(client(&server), &cfg, None, vec![888, 999, 555])
            .await
            .unwrap();

        let parsed: RblxSyncConfig =
            serde_yaml::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        let ids: Vec<u64> = parsed.places.iter().map(|p| p.place_id).collect();
        assert!(ids.contains(&777)); // root, auto-discovered
        assert!(ids.contains(&888)); // valid --place-id
        assert!(!ids.contains(&999)); // invalid id skipped, not aborted
        assert!(!ids.contains(&555)); // mismatched experience id skipped
        let extra = parsed.places.iter().find(|p| p.place_id == 888).unwrap();
        assert_eq!(extra.file_path, "");
        assert!(!extra.publish);
    }

    // run rename: config entry with id set + changed name issues a PATCH, no create.
    #[tokio::test]
    async fn run_id_rename_patches_no_create() {
        let server = MockServer::start().await;
        mount_empty_game_pass_list(&server).await;
        // A create POST would fail the test.
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let patch = Mock::given(method("PATCH"))
            .and(path("/game-passes/v1/universes/1/game-passes/77"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 77})))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let mut config = base_config();
        let mut gp = game_pass("New Name", Some(100));
        gp.id = Some(77);
        config.game_passes = vec![gp];
        // Lockfile has the old name under id 77.
        let mut state = SyncState::default();
        state.update_game_pass(
            77,
            "Old Name".to_string(),
            None,
            Some(100),
            None,
            None,
            None,
        );

        sync_game_passes(
            1,
            &config,
            &mut state,
            &client(&server),
            false,
            Path::new("/nonexistent/rblxsync.yml"),
        )
        .await
        .unwrap();

        drop(patch);
        assert_eq!(state.game_passes.get(&77).unwrap().name, "New Name");
    }

    // run adopt: config entry with id set but absent from lockfile adopts (PATCH), no create.
    #[tokio::test]
    async fn run_id_adopt_patches_no_create() {
        let server = MockServer::start().await;
        mount_empty_game_pass_list(&server).await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let patch = Mock::given(method("PATCH"))
            .and(path("/game-passes/v1/universes/1/game-passes/88"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 88})))
            .expect(1)
            .mount_as_scoped(&server)
            .await;

        let mut config = base_config();
        let mut gp = game_pass("Adopt Me", Some(100));
        gp.id = Some(88);
        config.game_passes = vec![gp];
        let mut state = SyncState::default();

        sync_game_passes(
            1,
            &config,
            &mut state,
            &client(&server),
            false,
            Path::new("/nonexistent/rblxsync.yml"),
        )
        .await
        .unwrap();

        drop(patch);
        assert!(state.game_passes.contains_key(&88));
    }

    // run write-back: creating an entry with no id inserts id: into the yml,
    // preserving an adjacent comment.
    #[tokio::test]
    async fn run_write_back_inserts_id_preserving_comment() {
        let server = MockServer::start().await;
        mount_empty_game_pass_list(&server).await;
        Mock::given(method("POST"))
            .and(path("/game-passes/v1/universes/1/game-passes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"gamePassId": 5005})))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("rblxsync.yml");
        std::fs::write(
            &cfg,
            "game_passes:\n  # the premium pass\n  - name: VIP Pass\n    price: 100\n",
        )
        .unwrap();

        let mut config = base_config();
        config.game_passes = vec![game_pass("VIP Pass", Some(100))];
        let mut state = SyncState::default();

        // Passing the real config path makes the create write the id back
        // in-place, immediately.
        sync_game_passes(1, &config, &mut state, &client(&server), false, &cfg)
            .await
            .unwrap();
        let written = std::fs::read_to_string(&cfg).unwrap();
        assert!(written.contains("# the premium pass"), "got: {}", written);
        assert!(written.contains("id: 5005"), "got: {}", written);
    }
}
