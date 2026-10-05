//! Experience thumbnails: the game page carousel (the legacy
//! game-internationalization endpoints, one set per language) and the home
//! page tiles (the thumbnail personalization API).

use super::{send_with_retry, truncate_body, RobloxClient};
use anyhow::{anyhow, Context, Result};
use reqwest::multipart::{Form, Part};
use reqwest::{Method, RequestBuilder};
use serde_json::{json, Value};

/// A thumbnail on the game page.
#[derive(Debug, Clone, PartialEq)]
pub struct GamePageThumbnail {
    /// Media asset id.
    pub id: u64,
    pub alt_text: String,
    /// Moderation state: `Approved`, `PendingReview`, `Rejected`, ...
    pub state: String,
}

/// A home page thumbnail.
#[derive(Debug, Clone, PartialEq)]
pub struct HomePageThumbnail {
    /// Homepage thumbnail id.
    pub id: String,
    /// `Reviewing`, `Approved`, `Rejected` or `Unspecified`.
    pub moderation_status: String,
    /// `Active`, or `Spammy` once Roblox flags it as spam.
    pub status: String,
}

/// The home page personalization Roblox is serving.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonalizedConfig {
    pub id: String,
    /// The thumbnails it serves.
    pub thumbnail_ids: Vec<String>,
}

/// A scalar these endpoints send as a string in some places and a number in
/// others (ids, statuses), as text.
fn as_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Content type of a thumbnail image, by file extension.
fn image_mime(filename: &str) -> &'static str {
    let ext = filename.rsplit('.').next().unwrap_or_default();
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tga" => "image/tga",
        _ => "image/png",
    }
}

/// An image file as one multipart part.
fn image_part((bytes, filename): &(Vec<u8>, String)) -> Part {
    Part::bytes(bytes.clone())
        .file_name(filename.clone())
        .mime_str(image_mime(filename))
        .expect("image MIME types are valid")
}

impl RobloxClient {
    /// Sends a request, retrying 429s, and returns the body of a 2xx
    /// response. Some of these endpoints answer with a bare string, which
    /// `execute` would fail to parse after the change went through.
    async fn send_text(&self, build: impl Fn() -> RequestBuilder) -> Result<String> {
        let response = send_with_retry(build).await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        log::debug!(
            "Thumbnail API response status: {}, body: {}",
            status,
            truncate_body(&text)
        );
        if !status.is_success() {
            return Err(anyhow!(
                "Open Cloud API request failed: {} - {}",
                status,
                text
            ));
        }
        Ok(text)
    }

    /// [`send_text`](Self::send_text), parsed as JSON. An empty body is `null`.
    async fn send_json(&self, build: impl Fn() -> RequestBuilder) -> Result<Value> {
        let text = self.send_text(build).await?;
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).with_context(|| format!("Failed to parse response: {}", text))
    }

    // --- Game page ---

    fn game_thumbnails_url(&self, universe_id: u64, language: &str, rest: &str) -> String {
        format!(
            "{}/legacy-game-internationalization/v1/game-thumbnails/games/{}/language-codes/{}/{}",
            self.base_url, universe_id, language, rest
        )
    }

    /// The experience's source language code (`en`, ...). The game page shows
    /// that language's thumbnail set. Public endpoint: no API key is sent.
    pub async fn get_source_language(&self, universe_id: u64) -> Result<String> {
        let url = format!(
            "{}/v1/source-language/games/{}",
            self.game_intl_base_url, universe_id
        );
        let body = self
            .send_json(|| self.client.get(&url))
            .await
            .context("Failed to read the experience's source language")?;
        as_text(&body["languageCode"])
            .ok_or_else(|| anyhow!("Source language response has no languageCode: {}", body))
    }

    /// The game page thumbnails for `language`, in display order, in every
    /// moderation state. Public endpoint: no API key is sent.
    pub async fn list_game_page_thumbnails(
        &self,
        universe_id: u64,
        language: &str,
    ) -> Result<Vec<GamePageThumbnail>> {
        let url = format!(
            "{}/v1/game-thumbnails/games/{}/images",
            self.game_intl_base_url, universe_id
        );
        let body = self
            .send_json(|| self.client.get(&url))
            .await
            .context("Failed to list game page thumbnails")?;
        let set = body["data"].as_array().into_iter().flatten().find(|set| {
            set["languageCode"]
                .as_str()
                .is_some_and(|code| code.eq_ignore_ascii_case(language))
        });
        Ok(set
            .and_then(|set| set["mediaAssets"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|asset| {
                Some(GamePageThumbnail {
                    id: as_text(&asset["mediaAssetId"])?.parse().ok()?,
                    alt_text: as_text(&asset["mediaAssetAltText"]).unwrap_or_default(),
                    state: as_text(&asset["state"]).unwrap_or_default(),
                })
            })
            .collect())
    }

    /// Adds an image to the game page and returns its media asset id.
    pub async fn upload_game_page_thumbnail(
        &self,
        universe_id: u64,
        language: &str,
        image: (Vec<u8>, String),
    ) -> Result<u64> {
        let url = self.game_thumbnails_url(universe_id, language, "image");
        // The endpoint binds the file from `gameThumbnailRequest.files`. The
        // spec's `Files` is the same model property without its prefix.
        let body = self
            .send_json(|| {
                let form = Form::new().part("gameThumbnailRequest.files", image_part(&image));
                self.request(Method::POST, &url).multipart(form)
            })
            .await?;
        as_text(&body["mediaAssetId"])
            .and_then(|id| id.parse().ok())
            .ok_or_else(|| anyhow!("Uploaded game page thumbnail has no id. Response: {}", body))
    }

    pub async fn set_game_page_thumbnail_alt_text(
        &self,
        universe_id: u64,
        language: &str,
        id: u64,
        alt_text: &str,
    ) -> Result<()> {
        let url = self.game_thumbnails_url(universe_id, language, "alt-text");
        let body = json!({ "thumbnailId": id, "altText": alt_text });
        self.send_text(|| self.request(Method::POST, &url).json(&body))
            .await?;
        Ok(())
    }

    /// Sets the game page order. `ids` lists every thumbnail on the page.
    pub async fn order_game_page_thumbnails(
        &self,
        universe_id: u64,
        language: &str,
        ids: &[u64],
    ) -> Result<()> {
        let url = self.game_thumbnails_url(universe_id, language, "images/order");
        let body = json!({ "mediaAssetIds": ids });
        self.send_text(|| self.request(Method::POST, &url).json(&body))
            .await
            .context("Failed to order game page thumbnails")?;
        Ok(())
    }

    pub async fn delete_game_page_thumbnail(
        &self,
        universe_id: u64,
        language: &str,
        id: u64,
    ) -> Result<()> {
        let url = self.game_thumbnails_url(universe_id, language, &format!("images/{}", id));
        self.send_text(|| self.request(Method::DELETE, &url))
            .await?;
        Ok(())
    }

    // --- Home page (thumbnail personalization, experimental) ---

    fn personalization_url(&self, universe_id: u64, rest: &str) -> String {
        format!(
            "{}/thumbnail-personalization-api/v1/universes/{}/{}",
            self.base_url, universe_id, rest
        )
    }

    /// Every home page thumbnail the experience has, active or not.
    pub async fn list_home_page_thumbnails(
        &self,
        universe_id: u64,
    ) -> Result<Vec<HomePageThumbnail>> {
        let url = self.personalization_url(universe_id, "thumbnails");
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = self
                .send_json(|| {
                    let req = self.request(Method::GET, &url);
                    match &cursor {
                        Some(c) => req.query(&[("nextCursor", c)]),
                        None => req,
                    }
                })
                .await
                .context("Failed to list home page thumbnails")?;
            all.extend(
                page["homepageThumbnails"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| {
                        Some(HomePageThumbnail {
                            id: as_text(&t["homepageThumbnailId"])?,
                            moderation_status: as_text(&t["moderationStatus"]).unwrap_or_default(),
                            status: as_text(&t["homepageThumbnailStatus"]).unwrap_or_default(),
                        })
                    }),
            );
            match as_text(&page["nextCursor"]) {
                Some(next) if cursor.as_ref() != Some(&next) => cursor = Some(next),
                _ => break,
            }
        }
        Ok(all)
    }

    /// Starts processing one home page thumbnail and returns the operation id
    /// to wait on with [`wait_for_home_page_upload`](Self::wait_for_home_page_upload).
    pub async fn upload_home_page_thumbnail(
        &self,
        universe_id: u64,
        image: (Vec<u8>, String),
    ) -> Result<String> {
        let url = self.personalization_url(universe_id, "thumbnails/uploads");
        let body = self
            .send_json(|| {
                let form = Form::new().part("files", image_part(&image));
                self.request(Method::POST, &url).multipart(form)
            })
            .await?;
        // `fileToOperationIdDict` maps each file name to its operation; a
        // null operation means Roblox didn't take the file.
        body["fileToOperationIdDict"]
            .as_object()
            .and_then(|ops| ops.values().find_map(as_text))
            .ok_or_else(|| anyhow!("Roblox did not accept the image. Response: {}", body))
    }

    /// Waits for a home page upload to finish processing (up to a minute) and
    /// returns the new thumbnail.
    pub async fn wait_for_home_page_upload(
        &self,
        universe_id: u64,
        operation_id: &str,
    ) -> Result<HomePageThumbnail> {
        let url = self.personalization_url(universe_id, "thumbnails/uploads/status");
        let mut last = Value::Null;
        for _ in 0..30 {
            last = self
                .send_json(|| {
                    self.request(Method::GET, &url)
                        .query(&[("operationIds", operation_id)])
                })
                .await
                .context("Failed to read the home page upload status")?;
            // The dict only holds finished operations. `uploadStatus` is an
            // undocumented 1/2 enum, so it isn't read.
            let done = &last["uploadThumbnailStatusDict"][operation_id];
            if let Some(id) = as_text(&done["homepageThumbnailId"]) {
                return Ok(HomePageThumbnail {
                    id,
                    moderation_status: as_text(&done["moderationStatus"]).unwrap_or_default(),
                    status: String::new(),
                });
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        Err(anyhow!(
            "Home page upload {} didn't finish within a minute. Last status: {}",
            operation_id,
            last
        ))
    }

    /// The personalization Roblox is serving on the home page, if any.
    pub async fn get_active_personalization(
        &self,
        universe_id: u64,
    ) -> Result<Option<PersonalizedConfig>> {
        let url = self.personalization_url(universe_id, "personalization");
        let body = self
            .send_json(|| {
                self.request(Method::GET, &url)
                    .query(&[("status", "Active")])
            })
            .await
            .context("Failed to read the home page personalization")?;
        let active = |v: &Value, key: &str| v[key].as_str() != Some("Inactive");
        Ok(body["personalizedConfigs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| active(c, "personalizedConfigStatus"))
            .find_map(|c| {
                Some(PersonalizedConfig {
                    id: as_text(&c["id"])?,
                    thumbnail_ids: c["thumbnails"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|t| active(t, "personalizedThumbnailStatus"))
                        .filter_map(|t| as_text(&t["homepageThumbnailId"]))
                        .collect(),
                })
            }))
    }

    /// Starts a new personalization serving `ids`. Roblox deactivates the
    /// current one; the new one starts its own statistics.
    pub async fn create_personalization(&self, universe_id: u64, ids: &[String]) -> Result<()> {
        let url = self.personalization_url(universe_id, "personalization/create");
        let body = json!({ "homepageThumbnailIds": ids });
        self.send_text(|| self.request(Method::POST, &url).json(&body))
            .await
            .context("Failed to create the home page personalization")?;
        Ok(())
    }

    /// Swaps the thumbnails of the active personalization `config_id`,
    /// keeping its id and statistics.
    pub async fn update_personalization(
        &self,
        universe_id: u64,
        config_id: &str,
        ids: &[String],
    ) -> Result<()> {
        let url = self.personalization_url(universe_id, "personalization/update");
        let body = json!({ "id": config_id, "homepageThumbnailIds": ids });
        self.send_text(|| self.request(Method::POST, &url).json(&body))
            .await
            .context("Failed to update the home page personalization")?;
        Ok(())
    }

    /// Deletes home page thumbnails and returns the ids Roblox answered
    /// `false` for.
    pub async fn delete_home_page_thumbnails(
        &self,
        universe_id: u64,
        ids: &[String],
    ) -> Result<Vec<String>> {
        let url = self.personalization_url(universe_id, "thumbnails");
        let query: Vec<(&str, &str)> = ids
            .iter()
            .map(|id| ("homepageThumbnailIds", id.as_str()))
            .collect();
        let body = self
            .send_json(|| self.request(Method::DELETE, &url).query(&query))
            .await
            .context("Failed to delete home page thumbnails")?;
        Ok(ids
            .iter()
            .filter(|id| body[id.as_str()] == Value::Bool(false))
            .cloned()
            .collect())
    }
}
