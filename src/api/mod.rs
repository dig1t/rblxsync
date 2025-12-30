use anyhow::{anyhow, Context, Result};
use reqwest::{Client, Method, RequestBuilder};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;

const BASE_URL: &str = "https://apis.roblox.com";

#[derive(Clone)]
pub struct RobloxClient {
    client: Client,
    api_key: String,
}

impl RobloxClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    fn request(&self, method: Method, url: &str) -> RequestBuilder {
        self.client
            .request(method, url)
            .header("x-api-key", &self.api_key)
    }

    async fn execute<T: DeserializeOwned>(&self, builder: RequestBuilder) -> Result<T> {
        let response = builder.send().await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        
        log::debug!("API response status: {}, body: {}", status, text);
        
        if !status.is_success() {
            return Err(anyhow!("API request failed: {} - {}", status, text));
        }

        let text = text;
        
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

    // --- Universe Settings ---

    pub async fn update_universe_settings(&self, universe_id: u64, settings: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/cloud/v2/universes/{}", BASE_URL, universe_id);
        log::debug!("Making PATCH request to: {}", url);
        log::debug!("Request body: {}", settings);
        self.execute(self.request(Method::PATCH, &url).json(settings)).await
    }

    // --- Game Passes ---

    pub async fn list_game_passes(&self, universe_id: u64, cursor: Option<String>) -> Result<ListResponse<serde_json::Value>> {
        let url = format!("{}/game-passes/v1/universes/{}/game-passes", BASE_URL, universe_id);
        let mut req = self.request(Method::GET, &url).query(&[("limit", "100")]);
        if let Some(c) = cursor {
            req = req.query(&[("cursor", &c)]);
        }
        self.execute(req).await
    }

    pub async fn create_game_pass(&self, universe_id: u64, data: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/game-passes/v1/universes/{}/game-passes", BASE_URL, universe_id);
        let form = json_to_multipart(data);
        log::debug!("Creating game pass at: {}", url);
        let result: serde_json::Value = self.execute(self.request(Method::POST, &url).multipart(form)).await?;
        log::info!("Create game pass response: {}", result);
        Ok(result)
    }

    pub async fn update_game_pass(&self, universe_id: u64, game_pass_id: u64, data: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/game-passes/v1/universes/{}/game-passes/{}", BASE_URL, universe_id, game_pass_id);
        log::debug!("Updating game pass at URL: {} with data: {}", url, data);
        let form = json_to_multipart(data);
        self.execute(self.request(Method::PATCH, &url).multipart(form)).await
    }

    // --- Developer Products ---

    pub async fn list_developer_products(&self, universe_id: u64, page_token: Option<String>) -> Result<ListResponse<serde_json::Value>> {
        let url = format!("{}/developer-products/v2/universes/{}/developer-products/creator", BASE_URL, universe_id);
        let mut req = self.request(Method::GET, &url).query(&[("pageSize", "50")]);
        if let Some(token) = page_token {
            req = req.query(&[("pageToken", &token)]);
        }
        self.execute(req).await
    }

    pub async fn create_developer_product(&self, universe_id: u64, data: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/developer-products/v2/universes/{}/developer-products", BASE_URL, universe_id);
        log::debug!("Creating developer product at: {}", url);
        let form = json_to_multipart(data);
        let result: serde_json::Value = self.execute(self.request(Method::POST, &url).multipart(form)).await?;
        log::info!("Create developer product response: {}", result);
        Ok(result)
    }

    pub async fn update_developer_product(&self, universe_id: u64, product_id: u64, data: &serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/developer-products/v2/universes/{}/developer-products/{}", BASE_URL, universe_id, product_id);
        log::debug!("Updating developer product at URL: {} with data: {}", url, data);
        let form = json_to_multipart(data);
        self.execute(self.request(Method::PATCH, &url).multipart(form)).await
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

    pub async fn list_badges(&self, universe_id: u64, cursor: Option<String>) -> Result<ListResponse<serde_json::Value>> {
        // List badges uses badges.roblox.com, not apis.roblox.com
        let url = format!("https://badges.roblox.com/v1/universes/{}/badges", universe_id);
        let mut req = self.request(Method::GET, &url).query(&[("limit", "100")]);
        if let Some(c) = cursor {
            req = req.query(&[("cursor", &c)]);
        }
        self.execute(req).await
    }

    pub async fn create_badge(
        &self, 
        universe_id: u64, 
        name: &str, 
        description: &str, 
        image_data: Option<(Vec<u8>, String)>,
        payment_source_type: Option<&str>
    ) -> Result<serde_json::Value> {
        let url = format!("{}/legacy-badges/v1/universes/{}/badges", BASE_URL, universe_id);
        log::debug!("Creating badge at: {}", url);
        
        let mut form = reqwest::multipart::Form::new()
            .text("name", name.to_string())
            .text("description", description.to_string());
        
        // Add payment source type if provided (1 = User, 2 = Group)
        if let Some(source_type) = payment_source_type {
            let type_id = match source_type.to_lowercase().as_str() {
                "user" => "1",
                "group" => "2",
                _ => "1", // Default to user
            };
            form = form.text("paymentSourceType", type_id.to_string());
        }
        
        // Add image file if provided
        if let Some((data, filename)) = image_data {
            let file_part = reqwest::multipart::Part::bytes(data)
                .file_name(filename)
                .mime_str("image/png")?;
            form = form.part("request.files", file_part);
        }
        
        self.execute(self.request(Method::POST, &url).multipart(form)).await
    }

    pub async fn update_badge(&self, badge_id: u64, data: &serde_json::Value) -> Result<serde_json::Value> {
        // Update badge config
        let url = format!("{}/legacy-badges/v1/badges/{}", BASE_URL, badge_id);
        log::debug!("Updating badge at URL: {} with data: {}", url, data);
        self.execute(self.request(Method::PATCH, &url).json(data)).await
    }

    pub async fn update_badge_icon(&self, badge_id: u64, image_data: Vec<u8>, filename: &str) -> Result<serde_json::Value> {
        // Update badge icon uses legacy-publish endpoint
        let url = format!("{}/legacy-publish/v1/badges/{}/icon", BASE_URL, badge_id);
        log::debug!("Updating badge icon at URL: {}", url);
        
        let file_part = reqwest::multipart::Part::bytes(image_data)
            .file_name(filename.to_string())
            .mime_str("image/png")?;
        
        let form = reqwest::multipart::Form::new()
            .part("request.files", file_part);
        
        self.execute(self.request(Method::POST, &url).multipart(form)).await
    }

    // --- Assets (Images) ---

    pub async fn upload_asset(&self, file_path: &Path, name: &str, creator: &crate::config::CreatorConfig) -> Result<String> {
        // 1. Prepare Multipart
        let url = format!("{}/assets/v1/assets", BASE_URL);
        
        // Check file extension for content type
        let extension = file_path.extension().and_then(|s| s.to_str()).unwrap_or("png");
        let content_type = match extension {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "bmp" => "image/bmp",
            "tga" => "image/tga",
            _ => "image/png", // Default fallback
        };

        let file_content = tokio::fs::read(file_path).await?;
        let filename = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();

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
            description: format!("Uploaded by rbxsync from {}", filename),
            creation_context: WebAssetRequestCreationContext {
                creator: creator_web,
                expected_price: None, // Not used for image assets
            },
        };

        let request_json = serde_json::to_string(&request)?;

        // Try Part::bytes instead of stream_with_length
        // Use stream_with_length like Asphalt does
        let len = file_content.len() as u64;
        let file_part = reqwest::multipart::Part::stream_with_length(
            reqwest::Body::from(file_content),
            len,
        )
        .file_name(filename.clone())
        .mime_str(content_type)?;

        let form = reqwest::multipart::Form::new()
            .text("request", request_json.clone())
            .part("fileContent", file_part);

        log::debug!("Asset upload URL: {}", url);
        log::debug!("Asset upload request JSON: {}", request_json);

        let response = self.client
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

            let operation: OperationResponse = serde_json::from_str(&text)
                .context("Failed to parse operation response")?;

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
            let operation_path = operation.path
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

        let url = format!("{}/assets/v1/{}", BASE_URL, operation_path);
        let max_attempts = 30;
        let poll_interval = std::time::Duration::from_secs(2);

        for attempt in 1..=max_attempts {
            log::debug!("Polling operation (attempt {}): {}", attempt, url);

            let response = self.request(Method::GET, &url).send().await?;
            let status = response.status();
            let text = response.text().await?;

            if !status.is_success() {
                return Err(anyhow!("Failed to poll operation: {} - {}", status, text));
            }

            log::debug!("Poll response: {}", text);

            let operation: OperationResponse = serde_json::from_str(&text)
                .context("Failed to parse operation poll response")?;

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

        Err(anyhow!("Operation polling timed out after {} attempts", max_attempts))
    }

    // --- Places ---

    pub async fn publish_place(&self, universe_id: u64, place_id: u64, file_path: &Path) -> Result<serde_json::Value> {
        let url = format!("{}/v1/universes/{}/places/{}/versions", BASE_URL, universe_id, place_id);
        
        let file_content = tokio::fs::read(file_path).await?;
        let _version_type = "Published"; // or Saved
        
        self.client.post(&url)
            .header("x-api-key", &self.api_key)
            .query(&[("versionType", "Published")])
            .header("Content-Type", "application/octet-stream")
            .body(file_content)
            .send()
            .await?
            .json().await.map_err(|e| anyhow::anyhow!(e))
    }
}

/// Converts a JSON object to a HashMap suitable for form encoding
fn json_to_form(json: &serde_json::Value) -> std::collections::HashMap<String, String> {
    let mut form = std::collections::HashMap::new();
    if let Some(obj) = json.as_object() {
        for (key, value) in obj {
            let str_value = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::new(),
                // For arrays/objects, serialize to JSON string
                _ => value.to_string(),
            };
            form.insert(key.clone(), str_value);
        }
    }
    form
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
