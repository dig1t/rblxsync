use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Method, RequestBuilder};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;
use std::sync::RwLock;

const BASE_URL: &str = "https://apis.roblox.com";
const BADGES_BASE_URL: &str = "https://badges.roblox.com";

/// Truncates a response body for logging to reduce sensitive data spill in CI logs.
fn truncate_body(text: &str) -> String {
    const MAX_LEN: usize = 1000;
    if text.len() > MAX_LEN {
        format!(
            "{}... [truncated, {} bytes total]",
            &text[..MAX_LEN],
            text.len()
        )
    } else {
        text.to_string()
    }
}

/// Sends a request, retrying on HTTP 429 honoring the Retry-After header
/// (falling back to exponential backoff, each delay capped at 30s, up to
/// 10 retries — total ~3 minutes of patience before bailing).
///
/// The previous 3-retry / 1-2-4s budget was too short for Roblox's
/// monetization endpoints, which can stay rate-limited for tens of seconds
/// per bucket during a sync that touches many resources.
async fn send_with_retry(build: impl Fn() -> RequestBuilder) -> Result<reqwest::Response> {
    const MAX_RETRIES: u32 = 10;
    const MAX_DELAY_SECS: u64 = 30;
    let mut attempt = 0;
    loop {
        let response = build().send().await?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let backoff = std::cmp::min(1u64 << attempt, MAX_DELAY_SECS);
            let delay = retry_after.unwrap_or(backoff).min(MAX_DELAY_SECS);
            log::warn!(
                "Rate limited (429), retrying in {}s (attempt {}/{})",
                delay,
                attempt + 1,
                MAX_RETRIES
            );
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            attempt += 1;
            continue;
        }
        return Ok(response);
    }
}

#[derive(Clone)]
pub struct RobloxClient {
    client: Client,
    api_key: String,
    base_url: String,
    badges_base_url: String,
}

impl RobloxClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: BASE_URL.to_string(),
            badges_base_url: BADGES_BASE_URL.to_string(),
        }
    }

    /// Construct a client pointed at custom base URLs (used by tests to target a mock server).
    #[cfg(test)]
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.clone(),
            badges_base_url: base_url,
        }
    }

    fn request(&self, method: Method, url: &str) -> RequestBuilder {
        self.client
            .request(method, url)
            .header("x-api-key", &self.api_key)
    }

    async fn execute<T: DeserializeOwned>(&self, builder: RequestBuilder) -> Result<T> {
        let response = match builder.try_clone() {
            Some(_) => {
                send_with_retry(|| builder.try_clone().expect("request is cloneable")).await?
            }
            // Multipart/stream bodies can't be cloned; send once without 429
            // retry. Hot endpoints (game pass create / dev product update /
            // badge create) should use `execute_factory` instead.
            None => builder.send().await?,
        };
        Self::handle_response(response).await
    }

    /// Like `execute` but takes a factory closure that builds a fresh request
    /// on every retry attempt. Use this for multipart / stream bodies where
    /// `reqwest::RequestBuilder::try_clone()` returns `None` and the plain
    /// `execute` would fail on the first 429 instead of waiting through it.
    async fn execute_factory<T: DeserializeOwned, F>(&self, build: F) -> Result<T>
    where
        F: Fn() -> RequestBuilder,
    {
        let response = send_with_retry(build).await?;
        Self::handle_response(response).await
    }

    async fn handle_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        log::debug!(
            "Open Cloud API response status: {}, body: {}",
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

        // Handle empty response (common for PATCH/PUT endpoints)
        if text.is_empty() || text.trim().is_empty() {
            // Try to deserialize from empty JSON object or null
            if let Ok(val) = serde_json::from_str::<T>("{}") {
                return Ok(val);
            }
            if let Ok(val) = serde_json::from_str::<T>("null") {
                return Ok(val);
            }
            // If both fail, return an empty JSON value if T is serde_json::Value
            if std::any::type_name::<T>() == "serde_json::value::Value" {
                return serde_json::from_str("{}").context("Failed to create empty response");
            }
        }

        serde_json::from_str(&text).context(format!("Failed to parse response: {}", text))
    }

    // --- Universe (Open Cloud v2, read-only) ---

    /// Fetches a universe's metadata via the Open Cloud v2 endpoint
    /// `GET {base_url}/cloud/v2/universes/{universe_id}`.
    ///
    /// Returns the raw JSON object. Fields the import command reads:
    /// - `displayName` (string): the universe name
    /// - `description` (string)
    /// - `rootPlace` (string): resource path `universes/{u}/places/{placeId}`,
    ///   used by `list_places` to recover the root place ID.
    ///
    /// Authenticated with `x-api-key` (no cookie required for reads).
    pub async fn get_universe(&self, universe_id: u64) -> Result<serde_json::Value> {
        let url = format!("{}/cloud/v2/universes/{}", self.base_url, universe_id);
        log::debug!("Fetching universe at: {}", url);
        self.execute(self.request(Method::GET, &url))
            .await
            .context("Failed to fetch universe")
    }

    // --- Game Passes ---

    /// Lists game passes for a universe (thin list endpoint).
    ///
    /// Note: this list does NOT reliably include `description` / `isForSale`.
    /// For import, fetch each pass's full detail with [`get_game_pass`].
    pub async fn list_game_passes(
        &self,
        universe_id: u64,
        cursor: Option<String>,
    ) -> Result<ListResponse<serde_json::Value>> {
        let url = format!(
            "{}/game-passes/v1/universes/{}/game-passes",
            self.base_url, universe_id
        );
        let mut all_data = Vec::new();
        let mut cursor = cursor;
        loop {
            let mut req = self.request(Method::GET, &url).query(&[("limit", "100")]);
            if let Some(c) = &cursor {
                req = req.query(&[("cursor", c)]);
            }
            let page: ListResponse<serde_json::Value> = self.execute(req).await?;
            all_data.extend(page.data);
            match page.next_page_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }
        Ok(ListResponse {
            data: all_data,
            next_page_cursor: None,
        })
    }

    pub async fn create_game_pass(
        &self,
        universe_id: u64,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/game-passes/v1/universes/{}/game-passes",
            self.base_url, universe_id
        );
        log::debug!("Creating game pass at: {}", url);
        let result: serde_json::Value = self
            .execute_factory(|| {
                self.request(Method::POST, &url)
                    .multipart(json_to_multipart(data))
            })
            .await?;
        log::info!("Create game pass response: {}", result);
        Ok(result)
    }

    pub async fn update_game_pass(
        &self,
        universe_id: u64,
        game_pass_id: u64,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/game-passes/v1/universes/{}/game-passes/{}",
            self.base_url, universe_id, game_pass_id
        );
        log::debug!("Updating game pass at URL: {} with data: {}", url, data);
        self.execute_factory(|| {
            self.request(Method::PATCH, &url)
                .multipart(json_to_multipart(data))
        })
        .await
    }

    /// Update a game pass with an optional image file upload
    pub async fn update_game_pass_with_icon(
        &self,
        universe_id: u64,
        game_pass_id: u64,
        data: &serde_json::Value,
        image_data: Option<(Vec<u8>, String)>,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/game-passes/v1/universes/{}/game-passes/{}",
            self.base_url, universe_id, game_pass_id
        );
        log::debug!(
            "Updating game pass with icon at URL: {} with data: {}",
            url,
            data
        );

        self.execute_factory(|| {
            let mut form = json_to_multipart(data);
            // The image bytes have to be cloned per attempt — reqwest's
            // multipart bodies are stream-once and the retry layer builds
            // a fresh request each iteration.
            if let Some((file_bytes, filename)) = &image_data {
                let file_part = reqwest::multipart::Part::bytes(file_bytes.clone())
                    .file_name(filename.clone())
                    .mime_str("image/png")
                    .expect("image/png is a valid MIME type");
                form = form.part("file", file_part);
            }
            self.request(Method::PATCH, &url).multipart(form)
        })
        .await
    }

    /// Fetches a single game pass's full detail via
    /// `GET {base_url}/game-passes/v1/universes/{universe_id}/game-passes/{game_pass_id}/creator`.
    ///
    /// Unlike the thin [`list_game_passes`] response, this is documented to
    /// include the fields the import command needs:
    /// - `gamePassId` (integer)
    /// - `name` (string)
    /// - `description` (string)
    /// - `isForSale` (boolean)
    /// - `iconAssetId` (integer)
    /// - `priceInformation` (object, nullable): `{ defaultPriceInRobux, enabledFeatures }`
    pub async fn get_game_pass(
        &self,
        universe_id: u64,
        game_pass_id: u64,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/game-passes/v1/universes/{}/game-passes/{}/creator",
            self.base_url, universe_id, game_pass_id
        );
        log::debug!("Fetching game pass detail at: {}", url);
        self.execute(self.request(Method::GET, &url))
            .await
            .context("Failed to fetch game pass detail")
    }

    // --- Developer Products ---

    pub async fn list_developer_products(
        &self,
        universe_id: u64,
        page_token: Option<String>,
    ) -> Result<ListResponse<serde_json::Value>> {
        let url = format!(
            "{}/developer-products/v2/universes/{}/developer-products/creator",
            self.base_url, universe_id
        );
        let mut all_data = Vec::new();
        let mut page_token = page_token;
        loop {
            let mut req = self.request(Method::GET, &url).query(&[("pageSize", "50")]);
            if let Some(token) = &page_token {
                req = req.query(&[("pageToken", token)]);
            }
            let page: ListResponse<serde_json::Value> = self.execute(req).await?;
            all_data.extend(page.data);
            match page.next_page_cursor {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
        }
        Ok(ListResponse {
            data: all_data,
            next_page_cursor: None,
        })
    }

    pub async fn create_developer_product(
        &self,
        universe_id: u64,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/developer-products/v2/universes/{}/developer-products",
            self.base_url, universe_id
        );
        log::debug!("Creating developer product at: {}", url);
        let result: serde_json::Value = self
            .execute_factory(|| {
                self.request(Method::POST, &url)
                    .multipart(json_to_multipart(data))
            })
            .await?;
        log::info!("Create developer product response: {}", result);
        Ok(result)
    }

    pub async fn update_developer_product(
        &self,
        universe_id: u64,
        product_id: u64,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/developer-products/v2/universes/{}/developer-products/{}",
            self.base_url, universe_id, product_id
        );
        log::debug!(
            "Updating developer product at URL: {} with data: {}",
            url,
            data
        );
        self.execute_factory(|| {
            self.request(Method::PATCH, &url)
                .multipart(json_to_multipart(data))
        })
        .await
    }

    /// Update a developer product with an optional image file upload
    pub async fn update_developer_product_with_icon(
        &self,
        universe_id: u64,
        product_id: u64,
        data: &serde_json::Value,
        image_data: Option<(Vec<u8>, String)>,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/developer-products/v2/universes/{}/developer-products/{}",
            self.base_url, universe_id, product_id
        );
        log::debug!(
            "Updating developer product with icon at URL: {} with data: {}",
            url,
            data
        );

        self.execute_factory(|| {
            let mut form = json_to_multipart(data);
            if let Some((file_bytes, filename)) = &image_data {
                let file_part = reqwest::multipart::Part::bytes(file_bytes.clone())
                    .file_name(filename.clone())
                    .mime_str("image/png")
                    .expect("image/png is a valid MIME type");
                form = form.part("imageFile", file_part);
            }
            self.request(Method::PATCH, &url).multipart(form)
        })
        .await
    }

    // --- Badges ---
    // Note: Badges API is on badges.roblox.com for v1? The user query says:
    // https://badges.roblox.com/v1/universes/{universeId}/badges
    // Actually, Open Cloud might be apis.roblox.com now?
    // User query explicitly says: https://badges.roblox.com/v1/universes/{universeId}/badges
    // Wait, the new Open Cloud APIs for badges are usually apis.roblox.com/badges/v1...
    // Checking references... User provided: "New Monetization APIs (Dec 2025)..."
    // But for Badges, they listed: https://badges.roblox.com/v1/universes/{universeId}/badges
    // I will use the URL provided by the user.

    pub async fn list_badges(
        &self,
        universe_id: u64,
        cursor: Option<String>,
    ) -> Result<ListResponse<serde_json::Value>> {
        // List badges uses badges.roblox.com, not apis.roblox.com
        let url = format!(
            "{}/v1/universes/{}/badges",
            self.badges_base_url, universe_id
        );
        let mut all_data = Vec::new();
        let mut cursor = cursor;
        loop {
            let mut req = self.request(Method::GET, &url).query(&[("limit", "100")]);
            if let Some(c) = &cursor {
                req = req.query(&[("cursor", c)]);
            }
            let page: ListResponse<serde_json::Value> = self.execute(req).await?;
            all_data.extend(page.data);
            match page.next_page_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }
        Ok(ListResponse {
            data: all_data,
            next_page_cursor: None,
        })
    }

    pub async fn create_badge(
        &self,
        universe_id: u64,
        name: &str,
        description: &str,
        image_data: Option<(Vec<u8>, String)>,
        payment_source_type: Option<&str>,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/legacy-badges/v1/universes/{}/badges",
            self.base_url, universe_id
        );
        log::debug!("Creating badge at: {}", url);

        self.execute_factory(|| {
            let mut form = reqwest::multipart::Form::new()
                .text("name", name.to_string())
                .text("description", description.to_string());

            // Payment source type (1 = User, 2 = Group)
            if let Some(source_type) = payment_source_type {
                let type_id = match source_type.to_lowercase().as_str() {
                    "user" => "1",
                    "group" => "2",
                    _ => "1",
                };
                form = form.text("paymentSourceType", type_id.to_string());
            }

            if let Some((data, filename)) = &image_data {
                let file_part = reqwest::multipart::Part::bytes(data.clone())
                    .file_name(filename.clone())
                    .mime_str("image/png")
                    .expect("image/png is a valid MIME type");
                form = form.part("request.files", file_part);
            }

            self.request(Method::POST, &url).multipart(form)
        })
        .await
    }

    pub async fn update_badge(
        &self,
        badge_id: u64,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        // Update badge config
        let url = format!("{}/legacy-badges/v1/badges/{}", self.base_url, badge_id);
        log::debug!("Updating badge at URL: {} with data: {}", url, data);
        self.execute(self.request(Method::PATCH, &url).json(data))
            .await
    }

    pub async fn update_badge_icon(
        &self,
        badge_id: u64,
        image_data: Vec<u8>,
        filename: &str,
    ) -> Result<serde_json::Value> {
        // Update badge icon uses legacy-publish endpoint
        let url = format!(
            "{}/legacy-publish/v1/badges/{}/icon",
            self.base_url, badge_id
        );
        log::debug!("Updating badge icon at URL: {}", url);

        self.execute_factory(|| {
            let file_part = reqwest::multipart::Part::bytes(image_data.clone())
                .file_name(filename.to_string())
                .mime_str("image/png")
                .expect("image/png is a valid MIME type");
            let form = reqwest::multipart::Form::new().part("request.files", file_part);
            self.request(Method::POST, &url).multipart(form)
        })
        .await
    }

    // --- Assets (Images) ---

    pub async fn upload_asset(
        &self,
        file_path: &Path,
        name: &str,
        creator: &crate::config::CreatorConfig,
    ) -> Result<String> {
        // 1. Prepare Multipart
        let url = format!("{}/assets/v1/assets", self.base_url);

        // Check file extension for content type
        let extension = file_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png");
        let content_type = match extension {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "bmp" => "image/bmp",
            "tga" => "image/tga",
            _ => "image/png", // Default fallback
        };

        let file_content = tokio::fs::read(file_path).await?;
        let filename = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Create the request struct following Asphalt's approach
        let creator_web = if creator.creator_type == "group" {
            WebAssetCreator::Group(WebAssetGroupCreator {
                group_id: creator.id.clone(),
            })
        } else {
            WebAssetCreator::User(WebAssetUserCreator {
                user_id: creator.id.clone(),
            })
        };

        let request = WebAssetRequest {
            asset_type: "Image".to_string(),
            display_name: name.to_string(),
            description: format!("Uploaded by rblxsync from {}", filename),
            creation_context: WebAssetRequestCreationContext {
                creator: creator_web,
                expected_price: None, // Not used for image assets
            },
        };

        let request_json = serde_json::to_string(&request)?;

        // Try Part::bytes instead of stream_with_length
        // Use stream_with_length like Asphalt does
        let len = file_content.len() as u64;
        let file_part =
            reqwest::multipart::Part::stream_with_length(reqwest::Body::from(file_content), len)
                .file_name(filename.clone())
                .mime_str(content_type)?;

        let form = reqwest::multipart::Form::new()
            .text("request", request_json.clone())
            .part("fileContent", file_part);

        log::debug!("Asset upload URL: {}", url);
        log::debug!("Asset upload request JSON: {}", request_json);

        let response = self
            .client
            .request(Method::POST, &url)
            .header("x-api-key", &self.api_key)
            .multipart(form)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            // Parse operation response
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct OperationResponse {
                path: Option<String>,
                done: Option<bool>,
                response: Option<OperationResult>,
            }

            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct OperationResult {
                asset_id: Option<String>,
            }

            let operation: OperationResponse =
                serde_json::from_str(&text).context("Failed to parse operation response")?;

            log::debug!("Initial operation response: {}", text);

            // If the operation is already done, extract the asset ID
            if operation.done.unwrap_or(false) {
                if let Some(resp) = operation.response {
                    if let Some(asset_id) = resp.asset_id {
                        return Ok(asset_id);
                    }
                }
            }

            // Extract operation path for polling
            let operation_path = operation
                .path
                .ok_or_else(|| anyhow!("Operation response missing 'path' field"))?;

            // Poll the operation until it completes
            self.poll_operation(&operation_path).await
        } else {
            Err(anyhow!("Asset upload failed: {} - {}", status, text))
        }
    }

    /// Polls an asset operation until it completes and returns the asset ID
    async fn poll_operation(&self, operation_path: &str) -> Result<String> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct OperationResponse {
            done: Option<bool>,
            response: Option<OperationResult>,
            error: Option<OperationError>,
        }

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct OperationResult {
            asset_id: Option<String>,
        }

        #[derive(serde::Deserialize)]
        struct OperationError {
            message: Option<String>,
        }

        let url = format!("{}/assets/v1/{}", self.base_url, operation_path);
        let max_attempts = 30;
        let poll_interval = std::time::Duration::from_secs(2);

        for attempt in 1..=max_attempts {
            log::debug!("Polling operation (attempt {}): {}", attempt, url);

            let response = send_with_retry(|| self.request(Method::GET, &url)).await?;
            let status = response.status();
            let text = response.text().await?;

            if !status.is_success() {
                return Err(anyhow!("Failed to poll operation: {} - {}", status, text));
            }

            log::debug!("Poll response: {}", text);

            let operation: OperationResponse =
                serde_json::from_str(&text).context("Failed to parse operation poll response")?;

            if let Some(error) = operation.error {
                let msg = error.message.unwrap_or_else(|| "Unknown error".to_string());
                return Err(anyhow!("Asset operation failed: {}", msg));
            }

            if operation.done.unwrap_or(false) {
                if let Some(resp) = operation.response {
                    if let Some(asset_id) = resp.asset_id {
                        log::info!("Asset uploaded successfully with ID: {}", asset_id);
                        return Ok(asset_id);
                    }
                }
                return Err(anyhow!("Operation completed but no asset ID found"));
            }

            tokio::time::sleep(poll_interval).await;
        }

        Err(anyhow!(
            "Operation polling timed out after {} attempts",
            max_attempts
        ))
    }

    // --- Places ---

    /// Returns the place IDs belonging to a universe.
    ///
    /// Endpoint surprise: Open Cloud (x-api-key) has **no** "list all places in
    /// a universe" endpoint. `cloud/v2/.../places/{place_id}` only fetches a
    /// single known place, and `develop.roblox.com/v1/universes/{id}/places`
    /// requires cookie auth (not the API key this client holds). The only
    /// place data reachable with the API key is the universe's `rootPlace`
    /// (see [`get_universe`]), so this returns the root place ID (a single-
    /// element vec, or empty if the universe has no `rootPlace`).
    ///
    /// The `rootPlace` is a resource path `universes/{u}/places/{placeId}`;
    /// the place ID is the final path segment.
    pub async fn list_places(&self, universe_id: u64) -> Result<Vec<u64>> {
        let universe = self.get_universe(universe_id).await?;
        let root_place = universe.get("rootPlace").and_then(|v| v.as_str());
        let Some(path) = root_place else {
            return Ok(Vec::new());
        };
        let place_id = path
            .rsplit('/')
            .next()
            .and_then(|seg| seg.parse::<u64>().ok())
            .ok_or_else(|| anyhow!("Could not parse place ID from rootPlace path: {}", path))?;
        Ok(vec![place_id])
    }

    /// Fetch a single place via Open Cloud (`cloud/v2`). Universe-scoped, so a
    /// place id that does not belong to `universe_id` returns a 404 — this is
    /// how `import` validates user-supplied `--place-id` values. Returns the
    /// `Place` resource (`displayName`, `description`, `root`, ...).
    pub async fn get_place(&self, universe_id: u64, place_id: u64) -> Result<serde_json::Value> {
        let url = format!(
            "{}/cloud/v2/universes/{}/places/{}",
            self.base_url, universe_id, place_id
        );
        self.execute(self.request(Method::GET, &url)).await
    }

    pub async fn publish_place(
        &self,
        universe_id: u64,
        place_id: u64,
        file_path: &Path,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "{}/v1/universes/{}/places/{}/versions",
            self.base_url, universe_id, place_id
        );

        let file_content = tokio::fs::read(file_path).await?;
        let _version_type = "Published"; // or Saved

        let response = send_with_retry(|| {
            self.client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .query(&[("versionType", "Published")])
                .header("Content-Type", "application/octet-stream")
                .body(file_content.clone())
        })
        .await?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        log::debug!(
            "Publish place response status: {}, body: {}",
            status,
            truncate_body(&text)
        );

        if !status.is_success() {
            return Err(anyhow!(
                "Open Cloud publish place request failed: {} - {}",
                status,
                text
            ));
        }

        serde_json::from_str(&text)
            .context(format!("Failed to parse publish place response: {}", text))
    }
}

/// Client for develop.roblox.com API using .ROBLOSECURITY cookie authentication
/// This is required for updating universe settings like name and description
pub struct RobloxCookieClient {
    client: Client,
    cookie: String,
    csrf_token: RwLock<Option<String>>,
}

impl RobloxCookieClient {
    pub fn new(cookie: String) -> Self {
        Self {
            client: Client::new(),
            cookie,
            csrf_token: RwLock::new(None),
        }
    }

    /// Make a request with cookie authentication and CSRF token handling
    async fn request_with_csrf<T: DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T> {
        // First attempt
        let response = self.send_request(method.clone(), url, body).await?;

        // Check if we got a CSRF token error (403 with x-csrf-token header)
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            // Get the CSRF token from the response header
            if let Some(token) = response.headers().get("x-csrf-token") {
                let token_str = token.to_str().unwrap_or_default().to_string();
                log::debug!("Got CSRF token from 403 response");

                // Store the token
                if let Ok(mut csrf) = self.csrf_token.write() {
                    *csrf = Some(token_str);
                }

                // Retry the request with the token
                let retry_response = self.send_request(method, url, body).await?;
                if retry_response.status() == reqwest::StatusCode::FORBIDDEN {
                    let text = retry_response.text().await.unwrap_or_default();
                    return Err(anyhow!(
                        "develop.roblox.com (cookie) request failed with 403 after CSRF token refresh - \
                         the .ROBLOSECURITY cookie may be invalid/expired or lack permission for this universe: {}",
                        text
                    ));
                }
                return self.handle_response(retry_response).await;
            }
        }

        self.handle_response(response).await
    }

    async fn send_request(
        &self,
        method: Method,
        url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response> {
        // Snapshot the CSRF token so the request can be rebuilt for 429 retries.
        let csrf_token = self.csrf_token.read().ok().and_then(|c| c.clone());

        send_with_retry(|| {
            let mut req = self
                .client
                .request(method.clone(), url)
                .header("Cookie", format!(".ROBLOSECURITY={}", self.cookie))
                .header("Content-Type", "application/json");

            if let Some(token) = &csrf_token {
                req = req.header("x-csrf-token", token);
            }

            if let Some(json_body) = body {
                req = req.json(json_body);
            }

            req
        })
        .await
    }

    async fn handle_response<T: DeserializeOwned>(&self, response: reqwest::Response) -> Result<T> {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        log::debug!(
            "develop.roblox.com (cookie) API response status: {}, body: {}",
            status,
            truncate_body(&text)
        );

        if !status.is_success() {
            return Err(anyhow!(
                "develop.roblox.com (cookie) request failed: {} - {}",
                status,
                text
            ));
        }

        if text.is_empty() || text.trim().is_empty() {
            if let Ok(val) = serde_json::from_str::<T>("{}") {
                return Ok(val);
            }
        }

        serde_json::from_str(&text).context(format!("Failed to parse response: {}", text))
    }

    /// Update universe configuration via develop.roblox.com API
    /// Endpoint: PATCH https://develop.roblox.com/v2/universes/{universeId}/configuration
    pub async fn update_universe_configuration(
        &self,
        universe_id: u64,
        settings: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = format!(
            "https://develop.roblox.com/v2/universes/{}/configuration",
            universe_id
        );
        log::debug!("Making PATCH request to: {}", url);
        log::debug!("Request body: {}", settings);

        self.request_with_csrf(Method::PATCH, &url, Some(settings))
            .await
    }
}

/// Converts a JSON object to multipart form data
fn json_to_multipart(json: &serde_json::Value) -> reqwest::multipart::Form {
    let mut form = reqwest::multipart::Form::new();
    if let Some(obj) = json.as_object() {
        for (key, value) in obj {
            let str_value = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::new(),
                _ => value.to_string(),
            };
            form = form.text(key.clone(), str_value);
        }
    }
    form
}

#[derive(Debug, Deserialize)]
pub struct ListResponse<T> {
    #[serde(alias = "gamePasses")]
    #[serde(alias = "developerProducts")]
    #[serde(alias = "badges")]
    pub data: Vec<T>,
    #[serde(alias = "nextPageCursor")]
    #[serde(alias = "nextPageToken")]
    pub next_page_cursor: Option<String>,
}

// Asset upload structs following Asphalt's implementation
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebAssetRequest {
    asset_type: String,
    display_name: String,
    description: String,
    creation_context: WebAssetRequestCreationContext,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebAssetRequestCreationContext {
    creator: WebAssetCreator,
    expected_price: Option<u32>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum WebAssetCreator {
    User(WebAssetUserCreator),
    Group(WebAssetGroupCreator),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebAssetUserCreator {
    user_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebAssetGroupCreator {
    group_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> RobloxClient {
        RobloxClient::with_base_url("test-key".to_string(), server.uri())
    }

    #[tokio::test]
    async fn update_game_pass_success() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/game-passes/v1/universes/1/game-passes/2"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id": 2, "name": "Pass"})),
            )
            .mount(&server)
            .await;

        let result = client(&server)
            .update_game_pass(1, 2, &json!({"name": "Pass"}))
            .await
            .unwrap();
        assert_eq!(result["id"], 2);
    }

    #[tokio::test]
    async fn execute_non_2xx_returns_err_with_status() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/game-passes/v1/universes/1/game-passes/2"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let err = client(&server)
            .update_game_pass(1, 2, &json!({"name": "Pass"}))
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Open Cloud"), "msg: {}", msg);
        assert!(msg.contains("404"), "msg: {}", msg);
    }

    #[tokio::test]
    async fn execute_429_retries_then_succeeds() {
        // update_badge uses a JSON (cloneable) body, so send_with_retry applies.
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/legacy-badges/v1/badges/5"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("PATCH"))
            .and(path("/legacy-badges/v1/badges/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 5})))
            .mount(&server)
            .await;

        let result = client(&server)
            .update_badge(5, &json!({"name": "Badge"}))
            .await
            .unwrap();
        assert_eq!(result["id"], 5);
    }

    #[tokio::test]
    async fn execute_429_exhausts_retries_fails() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/legacy-badges/v1/badges/5"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .mount(&server)
            .await;

        let err = client(&server)
            .update_badge(5, &json!({"name": "Badge"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("429"), "msg: {}", err);
    }

    #[tokio::test]
    async fn empty_body_patch_deserializes_to_default() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/game-passes/v1/universes/1/game-passes/2"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&server)
            .await;

        let result = client(&server)
            .update_game_pass(1, 2, &json!({"name": "Pass"}))
            .await
            .unwrap();
        assert!(result.is_object());
        assert_eq!(result, json!({}));
    }

    #[tokio::test]
    async fn list_game_passes_paginates() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes"))
            .and(query_param("cursor", "page2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": 3}], "nextPageCursor": null})),
            )
            .mount(&server)
            .await;
        // Page 1 has no cursor query param; return cursor pointing to page 2.
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"data": [{"id": 1}, {"id": 2}], "nextPageCursor": "page2"}),
                ),
            )
            .mount(&server)
            .await;

        let result = client(&server).list_game_passes(1, None).await.unwrap();
        assert_eq!(result.data.len(), 3);
        assert!(result.next_page_cursor.is_none());
    }

    #[tokio::test]
    async fn list_developer_products_paginates() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/developer-products/v2/universes/1/developer-products/creator",
            ))
            .and(query_param("pageToken", "tok2"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"developerProducts": [{"id": 3}], "nextPageToken": null}),
                ),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/developer-products/v2/universes/1/developer-products/creator",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"developerProducts": [{"id": 1}, {"id": 2}], "nextPageToken": "tok2"}),
            ))
            .mount(&server)
            .await;

        let result = client(&server)
            .list_developer_products(1, None)
            .await
            .unwrap();
        assert_eq!(result.data.len(), 3);
    }

    #[tokio::test]
    async fn list_badges_paginates() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .and(query_param("cursor", "b2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"badges": [{"id": 3}], "nextPageCursor": null})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/universes/1/badges"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"badges": [{"id": 1}, {"id": 2}], "nextPageCursor": "b2"}),
                ),
            )
            .mount(&server)
            .await;

        let result = client(&server).list_badges(1, None).await.unwrap();
        assert_eq!(result.data.len(), 3);
    }

    #[tokio::test]
    async fn publish_place_non_2xx_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/universes/1/places/2/versions"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad place"))
            .mount(&server)
            .await;

        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"placedata").unwrap();

        let err = client(&server)
            .publish_place(1, 2, file.path())
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("publish place"), "msg: {}", msg);
        assert!(msg.contains("400"), "msg: {}", msg);
    }

    #[tokio::test]
    async fn get_universe_returns_name_and_description() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "path": "universes/1",
                "displayName": "My Game",
                "description": "A great game",
                "rootPlace": "universes/1/places/42"
            })))
            .mount(&server)
            .await;

        let result = client(&server).get_universe(1).await.unwrap();
        assert_eq!(result["displayName"], "My Game");
        assert_eq!(result["description"], "A great game");
        assert_eq!(result["rootPlace"], "universes/1/places/42");
    }

    #[tokio::test]
    async fn get_universe_non_2xx_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1"))
            .respond_with(ResponseTemplate::new(404).set_body_string("no universe"))
            .mount(&server)
            .await;

        let err = client(&server).get_universe(1).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("fetch universe"), "msg: {}", msg);
    }

    #[tokio::test]
    async fn get_game_pass_returns_detail_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes/2/creator"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "gamePassId": 2,
                "name": "VIP",
                "description": "VIP perks",
                "isForSale": true,
                "iconAssetId": 999,
                "priceInformation": {"defaultPriceInRobux": 100, "enabledFeatures": []}
            })))
            .mount(&server)
            .await;

        let result = client(&server).get_game_pass(1, 2).await.unwrap();
        assert_eq!(result["gamePassId"], 2);
        assert_eq!(result["description"], "VIP perks");
        assert_eq!(result["isForSale"], true);
        assert_eq!(result["priceInformation"]["defaultPriceInRobux"], 100);
    }

    #[tokio::test]
    async fn get_game_pass_non_2xx_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/game-passes/v1/universes/1/game-passes/2/creator"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let err = client(&server).get_game_pass(1, 2).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("game pass detail"), "msg: {}", msg);
    }

    #[tokio::test]
    async fn list_places_returns_root_place_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "displayName": "My Game",
                "description": "desc",
                "rootPlace": "universes/1/places/42"
            })))
            .mount(&server)
            .await;

        let result = client(&server).list_places(1).await.unwrap();
        assert_eq!(result, vec![42]);
    }

    #[tokio::test]
    async fn list_places_empty_when_no_root_place() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/cloud/v2/universes/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "displayName": "My Game",
                "description": "desc"
            })))
            .mount(&server)
            .await;

        let result = client(&server).list_places(1).await.unwrap();
        assert!(result.is_empty());
    }
}
