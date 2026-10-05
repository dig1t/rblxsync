//! Experience thumbnails: the game page carousel and the home page tiles.
//!
//! Each configured image is paired with an earlier upload of the same bytes
//! (the lock file keeps each upload's SHA-256), so only new and edited images
//! are uploaded, and an upload that drops out of the config is deleted.
//! Thumbnails rblxsync didn't upload are never deleted.

use crate::api::{GamePageThumbnail, HomePageThumbnail, PersonalizedConfig, RobloxClient};
use crate::commands::{calculate_file_hash, read_icon};
use crate::config::{RblxSyncConfig, ThumbnailConfig, ThumbnailsConfig};
use crate::state::{SyncState, ThumbnailState};
use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Roblox shows at most this many images on a game page.
const GAME_PAGE_MAX: usize = 10;
/// A home page personalization serves at most this many thumbnails.
const HOME_PAGE_MAX: usize = 5;

/// Checks that need no files or network.
pub fn validate(thumbnails: &ThumbnailsConfig) -> Result<()> {
    // The game page endpoints an API key reaches are Roblox's localization
    // API: they manage translated sets and refuse the source language (code
    // 26), whose set is the carousel every player without a translation sees.
    if !thumbnails.game_page.is_empty() {
        return Err(anyhow!(
            "thumbnails.game_page can't be synced: an API key can only change translated game page thumbnails, not the set everyone sees. Upload those images in the Creator Hub and remove thumbnails.game_page"
        ));
    }
    if thumbnails.game_page.len() > GAME_PAGE_MAX {
        return Err(anyhow!(
            "thumbnails.game_page lists {} images; a game page shows at most {}",
            thumbnails.game_page.len(),
            GAME_PAGE_MAX
        ));
    }
    if thumbnails.home_page.len() > HOME_PAGE_MAX {
        return Err(anyhow!(
            "thumbnails.home_page lists {} images; the home page serves at most {}",
            thumbnails.home_page.len(),
            HOME_PAGE_MAX
        ));
    }
    if let Some(t) = thumbnails.home_page.iter().find(|t| t.alt_text.is_some()) {
        return Err(anyhow!(
            "thumbnails.home_page '{}': alt_text only works on game_page thumbnails",
            t.image
        ));
    }
    Ok(())
}

/// Preflight problems: configured images that aren't files.
pub fn missing_images(config: &RblxSyncConfig) -> Vec<String> {
    let assets_dir = Path::new(&config.assets_dir);
    let wanted = &config.thumbnails;
    wanted
        .game_page
        .iter()
        .chain(&wanted.home_page)
        .map(|t| assets_dir.join(&t.image))
        .filter(|path| !path.is_file())
        .map(|path| format!("Thumbnail image not found at {}", path.display()))
        .collect()
}

/// A configured image and the upload it maps to (`None` until uploaded).
struct Slot<Id> {
    image: String,
    hash: String,
    alt_text: Option<String>,
    id: Option<Id>,
}

struct GamePagePlan {
    language: String,
    slots: Vec<Slot<u64>>,
    /// Uploads that dropped out: a removed entry, or a file that changed.
    stale: Vec<ThumbnailState<u64>>,
    /// The page as Roblox shows it now. `None` when a dry run couldn't read it.
    page: Option<Vec<GamePageThumbnail>>,
    /// Images on the page rblxsync didn't upload, in page order.
    unmanaged: Vec<u64>,
}

struct HomePagePlan {
    slots: Vec<Slot<String>>,
    stale: Vec<ThumbnailState<String>>,
    /// `None` when a dry run couldn't read them.
    remote: Option<Vec<HomePageThumbnail>>,
    active: Option<PersonalizedConfig>,
}

/// What a run does to the thumbnails, worked out before it changes anything.
pub struct ThumbnailPlan {
    universe_id: u64,
    assets_dir: PathBuf,
    dry_run: bool,
    /// The run's environment, whose lock file each change is saved to.
    env: Option<String>,
    game_page: Option<GamePagePlan>,
    home_page: Option<HomePagePlan>,
}

/// Reads what Roblox has and pairs it with the config, so a problem (no room
/// on the game page, a listing the API key can't read) stops the run before
/// it changes anything. An empty list in the config skips that set.
pub async fn plan(
    config: &RblxSyncConfig,
    state: &SyncState,
    client: &RobloxClient,
    dry_run: bool,
) -> Result<ThumbnailPlan> {
    let universe_id = config.universe.id;
    let assets_dir = PathBuf::from(&config.assets_dir);
    let wanted = &config.thumbnails;
    let locked = &state.thumbnails;

    let game_page = if wanted.game_page.is_empty() {
        None
    } else {
        let language = readable(
            client.get_source_language(universe_id).await,
            dry_run,
            "the source language",
        )?
        .unwrap_or_else(|| "en".to_string());
        let page = readable(
            client
                .list_game_page_thumbnails(universe_id, &language)
                .await,
            dry_run,
            "the game page thumbnails",
        )?;
        let on_page = |id: &u64| page.as_ref().is_none_or(|p| p.iter().any(|t| t.id == *id));
        let (slots, stale) =
            pair(&assets_dir, &wanted.game_page, &locked.game_page, on_page).await?;
        let unmanaged: Vec<u64> = page
            .iter()
            .flatten()
            .map(|t| t.id)
            .filter(|id| !locked.game_page.iter().any(|l| l.id == *id))
            .collect();
        if slots.len() + unmanaged.len() > GAME_PAGE_MAX {
            return Err(anyhow!(
                "The game page holds at most {} images, and this run would leave {}: {} from rblxsync.yml plus {} rblxsync didn't upload ({:?}). Delete some in the Creator Hub or list fewer under thumbnails.game_page. No changes were made.",
                GAME_PAGE_MAX,
                slots.len() + unmanaged.len(),
                slots.len(),
                unmanaged.len(),
                unmanaged
            ));
        }
        Some(GamePagePlan {
            language,
            slots,
            stale,
            page,
            unmanaged,
        })
    };

    let home_page = if wanted.home_page.is_empty() {
        None
    } else {
        let remote = readable(
            client.list_home_page_thumbnails(universe_id).await,
            dry_run,
            "the home page thumbnails",
        )?;
        let active = readable(
            client.get_active_personalization(universe_id).await,
            dry_run,
            "the home page personalization",
        )?
        .flatten();
        let exists = |id: &String| {
            remote
                .as_ref()
                .is_none_or(|r| r.iter().any(|t| &t.id == id))
        };
        let (slots, stale) =
            pair(&assets_dir, &wanted.home_page, &locked.home_page, exists).await?;
        Some(HomePagePlan {
            slots,
            stale,
            remote,
            active,
        })
    };

    Ok(ThumbnailPlan {
        universe_id,
        assets_dir,
        dry_run,
        env: config.env.clone(),
        game_page,
        home_page,
    })
}

/// A read the plan needs. A dry run carries on without it and trusts the
/// lock file, so a preview works offline or before the API key has the
/// thumbnail permissions.
fn readable<T>(read: Result<T>, dry_run: bool, what: &str) -> Result<Option<T>> {
    match read {
        Ok(value) => Ok(Some(value)),
        Err(e) if dry_run => {
            warn!(
                "Dry Run: could not read {} (trusting rblxsync-lock.yml): {:#}",
                what, e
            );
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// Pairs each configured image with an earlier upload of the same bytes
/// that Roblox still has. Lock entries left unpaired are stale.
async fn pair<Id: Clone>(
    assets_dir: &Path,
    wanted: &[ThumbnailConfig],
    locked: &[ThumbnailState<Id>],
    exists: impl Fn(&Id) -> bool,
) -> Result<(Vec<Slot<Id>>, Vec<ThumbnailState<Id>>)> {
    let mut unpaired: Vec<&ThumbnailState<Id>> = locked.iter().collect();
    let mut slots = Vec::with_capacity(wanted.len());
    for entry in wanted {
        let hash = calculate_file_hash(&assets_dir.join(&entry.image)).await?;
        let id = unpaired
            .iter()
            .position(|l| l.hash == hash && exists(&l.id))
            .map(|i| unpaired.remove(i).id.clone());
        slots.push(Slot {
            image: entry.image.clone(),
            hash,
            alt_text: entry.alt_text.clone(),
            id,
        });
    }
    Ok((slots, unpaired.into_iter().cloned().collect()))
}

impl ThumbnailPlan {
    /// Makes Roblox match the plan. Each upload and delete is written to the
    /// lock file as it happens, so a run that fails partway loses none.
    pub async fn apply(
        mut self,
        state: &mut SyncState,
        client: &RobloxClient,
        lock_dir: &Path,
    ) -> Result<()> {
        if let Some(plan) = self.game_page.take() {
            self.apply_game_page(plan, state, client, lock_dir).await?;
        }
        if let Some(plan) = self.home_page.take() {
            self.apply_home_page(plan, state, client, lock_dir).await?;
        }
        Ok(())
    }

    async fn apply_game_page(
        &self,
        plan: GamePagePlan,
        state: &mut SyncState,
        client: &RobloxClient,
        lock_dir: &Path,
    ) -> Result<()> {
        info!("Syncing Game Page Thumbnails...");
        let GamePagePlan {
            language,
            mut slots,
            stale,
            page,
            unmanaged,
        } = plan;
        let (universe_id, dry_run) = (self.universe_id, self.dry_run);
        let on_page = |id: u64| page.as_ref().is_none_or(|p| p.iter().any(|t| t.id == id));
        // The page's order as this run changes it. `None` is an upload a dry
        // run skipped.
        let mut order: Vec<Option<u64>> = page.iter().flatten().map(|t| Some(t.id)).collect();

        // Delete first, so new uploads fit under the cap.
        let mut deleted = 0;
        for old in &stale {
            if on_page(old.id) {
                if dry_run {
                    info!(
                        "  [DELETE] Game page thumbnail {} (from '{}') - would delete",
                        old.id, old.image
                    );
                } else {
                    client
                        .delete_game_page_thumbnail(universe_id, &language, old.id)
                        .await
                        .with_context(|| {
                            format!("Failed to delete game page thumbnail {}", old.id)
                        })?;
                    info!(
                        "  [DELETED] Game page thumbnail {} (from '{}')",
                        old.id, old.image
                    );
                }
                deleted += 1;
                order.retain(|id| *id != Some(old.id));
            }
            if !dry_run {
                state.thumbnails.game_page.retain(|t| t.id != old.id);
                state.save(lock_dir, self.env.as_deref())?;
            }
        }

        let mut uploaded = 0;
        for slot in slots.iter_mut().filter(|s| s.id.is_none()) {
            uploaded += 1;
            if dry_run {
                info!(
                    "  [UPLOAD] Game page thumbnail '{}' - would upload",
                    slot.image
                );
                order.push(None);
                continue;
            }
            let image = read_icon(&self.assets_dir.join(&slot.image)).await?;
            let id = client
                .upload_game_page_thumbnail(universe_id, &language, image)
                .await
                .with_context(|| {
                    format!("Failed to upload game page thumbnail '{}'", slot.image)
                })?;
            info!(
                "  [UPLOADED] Game page thumbnail '{}' (ID: {})",
                slot.image, id
            );
            slot.id = Some(id);
            order.push(Some(id));
            state.thumbnails.game_page.push(ThumbnailState {
                image: slot.image.clone(),
                id,
                hash: slot.hash.clone(),
            });
            state.save(lock_dir, self.env.as_deref())?;
        }

        for slot in &slots {
            let Some(want) = &slot.alt_text else {
                continue;
            };
            let have = page
                .iter()
                .flatten()
                .find(|t| Some(t.id) == slot.id)
                .map_or("", |t| t.alt_text.as_str());
            if want == have {
                continue;
            }
            match slot.id {
                Some(id) if !dry_run => {
                    client
                        .set_game_page_thumbnail_alt_text(universe_id, &language, id, want)
                        .await
                        .with_context(|| {
                            format!(
                                "Failed to set the alt text of game page thumbnail '{}'",
                                slot.image
                            )
                        })?;
                    info!(
                        "  [UPDATED] Game page thumbnail '{}' (ID: {}) - alt text",
                        slot.image, id
                    );
                }
                _ => info!(
                    "  [UPDATE] Game page thumbnail '{}' - would set alt text",
                    slot.image
                ),
            }
        }

        // The configured images in config order, then the ones rblxsync
        // didn't upload. Where Roblox puts a new upload isn't documented, so
        // any upload sends the order too.
        let want: Vec<Option<u64>> = slots
            .iter()
            .map(|s| s.id)
            .chain(unmanaged.iter().map(|id| Some(*id)))
            .collect();
        if uploaded > 0 || want != order {
            if dry_run {
                info!("  [ORDER] Game page thumbnails - would put them in config order");
            } else {
                let ids: Vec<u64> = want.into_iter().flatten().collect();
                client
                    .order_game_page_thumbnails(universe_id, &language, &ids)
                    .await?;
                info!("  [ORDERED] Game page thumbnails - config order");
            }
        }

        if !dry_run {
            state.thumbnails.game_page = slots
                .iter()
                .filter_map(|s| {
                    Some(ThumbnailState {
                        image: s.image.clone(),
                        id: s.id?,
                        hash: s.hash.clone(),
                    })
                })
                .collect();
        }

        for slot in &slots {
            let rejected = page
                .iter()
                .flatten()
                .any(|t| Some(t.id) == slot.id && t.state == "Rejected");
            if rejected {
                warn!(
                    "Game page thumbnail '{}' was rejected by Roblox moderation and won't show. Replace the image.",
                    slot.image
                );
            }
        }
        if !unmanaged.is_empty() {
            warn!(
                "{} game page thumbnail(s) rblxsync didn't upload stay after yours: {:?}. Delete them in the Creator Hub if you don't want them.",
                unmanaged.len(),
                unmanaged
            );
        }
        info!(
            "Game Page Thumbnails Summary: {} uploaded, {} deleted, {} unchanged",
            uploaded,
            deleted,
            slots.len() - uploaded
        );
        Ok(())
    }

    async fn apply_home_page(
        &self,
        plan: HomePagePlan,
        state: &mut SyncState,
        client: &RobloxClient,
        lock_dir: &Path,
    ) -> Result<()> {
        info!("Syncing Home Page Thumbnails...");
        let HomePagePlan {
            mut slots,
            stale,
            remote,
            active,
        } = plan;
        let (universe_id, dry_run) = (self.universe_id, self.dry_run);
        // Moderation and spam status by id, joined by each finished upload.
        let mut status: HashMap<String, HomePageThumbnail> = remote
            .iter()
            .flatten()
            .map(|t| (t.id.clone(), t.clone()))
            .collect();

        let mut uploaded = 0;
        for slot in slots.iter_mut().filter(|s| s.id.is_none()) {
            uploaded += 1;
            if dry_run {
                info!(
                    "  [UPLOAD] Home page thumbnail '{}' - would upload",
                    slot.image
                );
                continue;
            }
            let image = read_icon(&self.assets_dir.join(&slot.image)).await?;
            let done = async {
                let operation = client
                    .upload_home_page_thumbnail(universe_id, image)
                    .await?;
                client
                    .wait_for_home_page_upload(universe_id, &operation)
                    .await
            }
            .await
            .with_context(|| format!("Failed to upload home page thumbnail '{}'", slot.image))?;
            info!(
                "  [UPLOADED] Home page thumbnail '{}' (ID: {}, moderation: {})",
                slot.image, done.id, done.moderation_status
            );
            state.thumbnails.home_page.push(ThumbnailState {
                image: slot.image.clone(),
                id: done.id.clone(),
                hash: slot.hash.clone(),
            });
            state.save(lock_dir, self.env.as_deref())?;
            slot.id = Some(done.id.clone());
            status.insert(done.id.clone(), done);
        }

        // Warned before the personalization call, which may fail over them.
        for slot in &slots {
            let Some(t) = slot.id.as_ref().and_then(|id| status.get(id)) else {
                continue;
            };
            if t.moderation_status == "Rejected" {
                warn!(
                    "Home page thumbnail '{}' was rejected by Roblox moderation and won't show. Replace the image.",
                    slot.image
                );
            }
            if t.status == "Spammy" {
                warn!(
                    "Roblox flagged home page thumbnail '{}' as spam. Replace the image.",
                    slot.image
                );
            }
        }

        // Serve exactly these: update the active personalization (it keeps
        // its statistics), or start one.
        let want: Vec<String> = slots.iter().filter_map(|s| s.id.clone()).collect();
        let wanted: HashSet<&String> = want.iter().collect();
        let serving: Option<HashSet<&String>> =
            active.as_ref().map(|a| a.thumbnail_ids.iter().collect());
        let stops = serving
            .as_ref()
            .map_or(0, |s| s.difference(&wanted).count());
        match &active {
            _ if uploaded == 0 && serving.as_ref() == Some(&wanted) => info!(
                "  [SKIP] Home page personalization - already serves these {} thumbnail(s)",
                want.len()
            ),
            Some(config) if dry_run => info!(
                "  [UPDATE] Home page personalization {} - would serve {} thumbnail(s) and stop serving {}, keeping its statistics",
                config.id,
                slots.len(),
                stops
            ),
            Some(config) => {
                client
                    .update_personalization(universe_id, &config.id, &want)
                    .await?;
                info!(
                    "  [UPDATED] Home page personalization {} - serves {} thumbnail(s), statistics kept",
                    config.id,
                    want.len()
                );
            }
            None if dry_run => info!(
                "  [CREATE] Home page personalization - would serve {} thumbnail(s)",
                slots.len()
            ),
            None => {
                client.create_personalization(universe_id, &want).await?;
                info!(
                    "  [CREATED] Home page personalization - serves {} thumbnail(s)",
                    want.len()
                );
            }
        }

        // Delete stale uploads now that nothing serves them.
        let doomed: Vec<&ThumbnailState<String>> = stale
            .iter()
            .filter(|t| {
                remote
                    .as_ref()
                    .is_none_or(|r| r.iter().any(|r| r.id == t.id))
            })
            .collect();
        let mut failed: Vec<String> = Vec::new();
        if dry_run {
            for t in &doomed {
                info!(
                    "  [DELETE] Home page thumbnail {} (from '{}') - would delete",
                    t.id, t.image
                );
            }
        } else if !doomed.is_empty() {
            let ids: Vec<String> = doomed.iter().map(|t| t.id.clone()).collect();
            failed = client
                .delete_home_page_thumbnails(universe_id, &ids)
                .await?;
            for t in &doomed {
                if failed.contains(&t.id) {
                    warn!(
                        "Roblox didn't delete home page thumbnail {} (from '{}'); the next run tries again",
                        t.id, t.image
                    );
                } else {
                    info!(
                        "  [DELETED] Home page thumbnail {} (from '{}')",
                        t.id, t.image
                    );
                }
            }
        }
        if !dry_run {
            state.thumbnails.home_page = slots
                .iter()
                .filter_map(|s| {
                    Some(ThumbnailState {
                        image: s.image.clone(),
                        id: s.id.clone()?,
                        hash: s.hash.clone(),
                    })
                })
                .chain(stale.iter().filter(|t| failed.contains(&t.id)).cloned())
                .collect();
            state.save(lock_dir, self.env.as_deref())?;
        }

        info!(
            "Home Page Thumbnails Summary: {} uploaded, {} deleted, {} unchanged",
            uploaded,
            doomed.len() - failed.len(),
            slots.len() - uploaded
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::multipart_field;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    const GAME: &str =
        "/legacy-game-internationalization/v1/game-thumbnails/games/1/language-codes/en";
    const HOME: &str = "/thumbnail-personalization-api/v1/universes/1";

    fn client(server: &MockServer) -> RobloxClient {
        RobloxClient::with_base_url("test-key".to_string(), server.uri())
    }

    fn sha(content: &str) -> String {
        format!("{:x}", Sha256::digest(content.as_bytes()))
    }

    /// An assets dir holding `files`, each written with `<stem>-png` bytes.
    fn assets(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            let stem = file.trim_end_matches(".png");
            std::fs::write(dir.path().join(file), format!("{}-png", stem)).unwrap();
        }
        dir
    }

    fn config(assets_dir: &Path, thumbnails: &str) -> RblxSyncConfig {
        serde_yaml::from_str(&format!(
            "assets_dir: {:?}\nuniverse:\n  id: 1\nthumbnails:\n{}",
            assets_dir, thumbnails
        ))
        .unwrap()
    }

    fn lock<Id>(image: &str, id: Id, content: &str) -> ThumbnailState<Id> {
        ThumbnailState {
            image: image.to_string(),
            id,
            hash: sha(content),
        }
    }

    async fn sync(
        config: &RblxSyncConfig,
        mut state: SyncState,
        server: &MockServer,
        dry_run: bool,
        lock_dir: &Path,
    ) -> Result<SyncState> {
        let client = client(server);
        plan(config, &state, &client, dry_run)
            .await?
            .apply(&mut state, &client, lock_dir)
            .await?;
        Ok(state)
    }

    /// JSON bodies of the requests the mock server got for `verb` on `url_path`.
    async fn bodies(server: &MockServer, verb: &str, url_path: &str) -> Vec<Vec<u8>> {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.method.as_str() == verb && r.url.path() == url_path)
            .map(|r| r.body)
            .collect()
    }

    fn json_body(body: &[u8]) -> Value {
        serde_json::from_slice(body).unwrap()
    }

    /// Every request that wasn't a GET, as "METHOD path".
    async fn writes(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.method.as_str() != "GET")
            .map(|r| format!("{} {}", r.method, r.url.path()))
            .collect()
    }

    /// Source language `en`, and a listing whose `en` set is `media`. An `es`
    /// set comes first, so reading the wrong language shows.
    async fn mount_game_page(server: &MockServer, media: Value) {
        Mock::given(method("GET"))
            .and(path("/v1/source-language/games/1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"name": "English", "languageCode": "en"})),
            )
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/game-thumbnails/games/1/images"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
                {"languageCode": "es", "mediaAssets": [{"mediaAssetId": "777", "state": "Approved"}]},
                {"languageCode": "en", "mediaAssets": media},
            ]})))
            .mount(server)
            .await;
    }

    async fn mount_home_page(server: &MockServer, thumbnails: Value, configs: Value) {
        Mock::given(method("GET"))
            .and(path(format!("{}/thumbnails", HOME)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"homepageThumbnails": thumbnails, "nextCursor": null})),
            )
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{}/personalization", HOME)))
            .and(query_param("status", "Active"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"personalizedConfigs": configs, "nextCursor": null})),
            )
            .mount(server)
            .await;
    }

    /// Home page uploads answer `op-<stem>`, which finishes as `h-<stem>`.
    async fn mount_home_page_uploads(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path(format!("{}/thumbnails/uploads", HOME)))
            .respond_with(|req: &Request| {
                let body = String::from_utf8_lossy(&req.body);
                let stem = body
                    .split("filename=\"")
                    .nth(1)
                    .unwrap()
                    .split('.')
                    .next()
                    .unwrap();
                ResponseTemplate::new(200).set_body_json(
                    json!({"fileToOperationIdDict": {"upload.png": format!("op-{}", stem)}}),
                )
            })
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{}/thumbnails/uploads/status", HOME)))
            .respond_with(|req: &Request| {
                let (_, op) = req.url.query_pairs().find(|(k, _)| k == "operationIds").unwrap();
                let id = op.replace("op-", "h-");
                ResponseTemplate::new(200).set_body_json(json!({
                    "uploadStatus": 2,
                    "uploadThumbnailStatusDict": {
                        op.to_string(): {"homepageThumbnailId": id, "assetId": 5, "moderationStatus": "Reviewing"}
                    }
                }))
            })
            .mount(server)
            .await;
    }

    #[test]
    fn validate_refuses_the_game_page_and_caps_the_home_page() {
        let dir = assets(&[]);
        let one = "  game_page:\n    - image: a.png\n";
        let msg = validate(&config(dir.path(), one).thumbnails)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("game_page can't be synced"), "msg: {}", msg);

        let six = "  home_page:\n".to_string() + &"    - image: a.png\n".repeat(6);
        let msg = validate(&config(dir.path(), &six).thumbnails)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("at most 5"), "msg: {}", msg);

        let alt = "  home_page:\n    - image: a.png\n      alt_text: Hi\n";
        let msg = validate(&config(dir.path(), alt).thumbnails)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("alt_text"), "msg: {}", msg);
    }

    // New images go up in config order with their alt text. An upload the lock
    // remembers but the page no longer shows (deleted in the Creator Hub) is
    // uploaded again, not deleted.
    #[tokio::test]
    async fn game_page_uploads_new_images_then_orders_them() {
        let server = MockServer::start().await;
        mount_game_page(&server, json!([])).await;
        Mock::given(method("POST"))
            .and(path(format!("{}/image", GAME)))
            .respond_with(|req: &Request| {
                let id = if String::from_utf8_lossy(&req.body).contains("a-png") {
                    "11"
                } else {
                    "22"
                };
                ResponseTemplate::new(200).set_body_json(json!({"mediaAssetId": id}))
            })
            .mount(&server)
            .await;
        for rest in ["alt-text", "images/order"] {
            Mock::given(method("POST"))
                .and(path(format!("{}/{}", GAME, rest)))
                .respond_with(ResponseTemplate::new(200).set_body_string("\"\""))
                .mount(&server)
                .await;
        }

        let dir = assets(&["a.png", "b.png"]);
        let config = config(
            dir.path(),
            "  game_page:\n    - image: a.png\n      alt_text: Lobby\n    - image: b.png\n",
        );
        let mut state = SyncState::default();
        state.thumbnails.game_page = vec![lock("a.png", 5, "a-png")];
        let state = sync(&config, state, &server, false, dir.path())
            .await
            .unwrap();

        let uploads = bodies(&server, "POST", &format!("{}/image", GAME)).await;
        assert_eq!(uploads.len(), 2);
        assert_eq!(
            multipart_field(&uploads[0], "gameThumbnailRequest.files").as_deref(),
            Some("a-png")
        );
        let alt = bodies(&server, "POST", &format!("{}/alt-text", GAME)).await;
        assert_eq!(
            json_body(&alt[0]),
            json!({"thumbnailId": 11, "altText": "Lobby"})
        );
        let order = bodies(&server, "POST", &format!("{}/images/order", GAME)).await;
        assert_eq!(json_body(&order[0]), json!({"mediaAssetIds": [11, 22]}));
        assert!(!writes(&server)
            .await
            .iter()
            .any(|w| w.starts_with("DELETE")));

        assert_eq!(
            state.thumbnails.game_page,
            vec![lock("a.png", 11, "a-png"), lock("b.png", 22, "b-png")]
        );
        let saved = SyncState::load(dir.path(), None).unwrap();
        assert_eq!(saved.thumbnails.game_page.len(), 2);
    }

    // An unchanged image is kept, a dropped upload is deleted, and an image
    // rblxsync didn't upload is left where it is.
    #[tokio::test]
    async fn game_page_deletes_only_dropped_uploads() {
        let server = MockServer::start().await;
        mount_game_page(
            &server,
            json!([
                {"mediaAssetId": "11", "mediaAssetAltText": "Lobby", "state": "Approved"},
                {"mediaAssetId": "33", "mediaAssetAltText": "", "state": "Approved"},
                {"mediaAssetId": "99", "mediaAssetAltText": "", "state": "Approved"},
            ]),
        )
        .await;
        Mock::given(method("DELETE"))
            .and(path(format!("{}/images/33", GAME)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;

        let dir = assets(&["a.png"]);
        let config = config(
            dir.path(),
            "  game_page:\n    - image: a.png\n      alt_text: Lobby\n",
        );
        let mut state = SyncState::default();
        state.thumbnails.game_page =
            vec![lock("a.png", 11, "a-png"), lock("old.png", 33, "old-png")];
        let state = sync(&config, state, &server, false, dir.path())
            .await
            .unwrap();

        assert_eq!(
            writes(&server).await,
            vec![format!("DELETE {}/images/33", GAME)]
        );
        assert_eq!(state.thumbnails.game_page, vec![lock("a.png", 11, "a-png")]);
    }

    #[tokio::test]
    async fn game_page_cap_counts_images_rblxsync_did_not_upload() {
        let server = MockServer::start().await;
        let by_hand: Vec<Value> = (1..=8)
            .map(|id| json!({"mediaAssetId": id.to_string(), "state": "Approved"}))
            .collect();
        mount_game_page(&server, json!(by_hand)).await;

        let dir = assets(&["a.png", "b.png", "c.png"]);
        let config = config(
            dir.path(),
            "  game_page:\n    - image: a.png\n    - image: b.png\n    - image: c.png\n",
        );
        let msg = plan(&config, &SyncState::default(), &client(&server), false)
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(msg.contains("would leave 11"), "msg: {}", msg);
    }

    #[tokio::test]
    async fn home_page_uploads_then_starts_a_personalization() {
        let server = MockServer::start().await;
        mount_home_page(&server, json!([]), json!([])).await;
        mount_home_page_uploads(&server).await;
        Mock::given(method("POST"))
            .and(path(format!("{}/personalization/create", HOME)))
            .respond_with(ResponseTemplate::new(200).set_body_string("\"cfg-1\""))
            .mount(&server)
            .await;

        let dir = assets(&["a.png", "b.png"]);
        let config = config(
            dir.path(),
            "  home_page:\n    - image: a.png\n    - image: b.png\n",
        );
        let state = sync(&config, SyncState::default(), &server, false, dir.path())
            .await
            .unwrap();

        let uploads = bodies(&server, "POST", &format!("{}/thumbnails/uploads", HOME)).await;
        assert_eq!(
            multipart_field(&uploads[0], "files").as_deref(),
            Some("a-png")
        );
        let create = bodies(&server, "POST", &format!("{}/personalization/create", HOME)).await;
        assert_eq!(
            json_body(&create[0]),
            json!({"homepageThumbnailIds": ["h-a", "h-b"]})
        );
        assert_eq!(
            state.thumbnails.home_page,
            vec![
                lock("a.png", "h-a".to_string(), "a-png"),
                lock("b.png", "h-b".to_string(), "b-png")
            ]
        );
    }

    // The active personalization is updated in place (keeping its
    // statistics), and the upload it stopped serving is deleted after.
    #[tokio::test]
    async fn home_page_updates_the_active_set_then_deletes_the_dropped_upload() {
        let server = MockServer::start().await;
        let thumb = |id: &str| json!({"homepageThumbnailId": id, "moderationStatus": "Approved", "homepageThumbnailStatus": "Active"});
        let serving =
            |id: &str| json!({"homepageThumbnailId": id, "personalizedThumbnailStatus": "Active"});
        mount_home_page(
            &server,
            json!([thumb("h-a"), thumb("h-old")]),
            json!([{"id": "cfg-1", "personalizedConfigStatus": "Active", "thumbnails": [serving("h-a"), serving("h-old")]}]),
        )
        .await;
        mount_home_page_uploads(&server).await;
        Mock::given(method("POST"))
            .and(path(format!("{}/personalization/update", HOME)))
            .respond_with(ResponseTemplate::new(200).set_body_string("cfg-1"))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(format!("{}/thumbnails", HOME)))
            .and(query_param("homepageThumbnailIds", "h-old"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"h-old": true})))
            .mount(&server)
            .await;

        let dir = assets(&["a.png", "b.png"]);
        let config = config(
            dir.path(),
            "  home_page:\n    - image: a.png\n    - image: b.png\n",
        );
        let mut state = SyncState::default();
        state.thumbnails.home_page = vec![
            lock("a.png", "h-a".to_string(), "a-png"),
            lock("old.png", "h-old".to_string(), "old-png"),
        ];
        let state = sync(&config, state, &server, false, dir.path())
            .await
            .unwrap();

        assert_eq!(
            writes(&server).await,
            vec![
                format!("POST {}/thumbnails/uploads", HOME),
                format!("POST {}/personalization/update", HOME),
                format!("DELETE {}/thumbnails", HOME),
            ]
        );
        let update = bodies(&server, "POST", &format!("{}/personalization/update", HOME)).await;
        assert_eq!(
            json_body(&update[0]),
            json!({"id": "cfg-1", "homepageThumbnailIds": ["h-a", "h-b"]})
        );
        assert_eq!(
            state.thumbnails.home_page,
            vec![
                lock("a.png", "h-a".to_string(), "a-png"),
                lock("b.png", "h-b".to_string(), "b-png")
            ]
        );
    }

    // Serving the same set in another order is no change.
    #[tokio::test]
    async fn home_page_already_serving_the_set_is_left_alone() {
        let server = MockServer::start().await;
        let thumb = |id: &str| json!({"homepageThumbnailId": id, "moderationStatus": "Approved", "homepageThumbnailStatus": "Active"});
        let serving =
            |id: &str| json!({"homepageThumbnailId": id, "personalizedThumbnailStatus": "Active"});
        mount_home_page(
            &server,
            json!([thumb("h-a"), thumb("h-b")]),
            json!([{"id": "cfg-1", "thumbnails": [serving("h-b"), serving("h-a")]}]),
        )
        .await;

        let dir = assets(&["a.png", "b.png"]);
        let config = config(
            dir.path(),
            "  home_page:\n    - image: a.png\n    - image: b.png\n",
        );
        let mut state = SyncState::default();
        state.thumbnails.home_page = vec![
            lock("a.png", "h-a".to_string(), "a-png"),
            lock("b.png", "h-b".to_string(), "b-png"),
        ];
        sync(&config, state, &server, false, dir.path())
            .await
            .unwrap();

        assert!(writes(&server).await.is_empty());
    }

    #[tokio::test]
    async fn dry_run_only_reads() {
        let server = MockServer::start().await;
        mount_game_page(
            &server,
            json!([{"mediaAssetId": "33", "state": "Approved"}]),
        )
        .await;
        mount_home_page(&server, json!([]), json!([])).await;
        for verb in ["POST", "DELETE"] {
            Mock::given(method(verb))
                .respond_with(ResponseTemplate::new(500))
                .mount(&server)
                .await;
        }

        let dir = assets(&["a.png"]);
        let config = config(
            dir.path(),
            "  game_page:\n    - image: a.png\n      alt_text: Lobby\n  home_page:\n    - image: a.png\n",
        );
        let mut state = SyncState::default();
        state.thumbnails.game_page = vec![lock("old.png", 33, "old-png")];
        let after = sync(&config, state.clone(), &server, true, dir.path())
            .await
            .unwrap();

        assert!(writes(&server).await.is_empty());
        assert_eq!(after.thumbnails, state.thumbnails);
        assert!(!dir.path().join("rblxsync-lock.yml").exists());
    }
}
