use crate::config::Config;
use anyhow::{anyhow, Result};
use reqwest::{Client, Method, RequestBuilder};
use serde::de::DeserializeOwned;
use serde::Serialize;

const BASE_URL: &str = "https://apis.roblox.com";

#[derive(Clone)]
pub struct RobloxClient {
    client: Client,
    api_key: String,
}

impl RobloxClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            api_key: config.api_key.clone(),
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", BASE_URL, path);
        self.client
            .request(method, &url)
            .header("x-api-key", &self.api_key)
            .header("Content-Type", "application/json")
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self.request(Method::GET, path).send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("API request failed: {} - {}", status, text));
        }

        response.json::<T>().await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let response = self.request(Method::POST, path)
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("API request failed: {} - {}", status, text));
        }

        response.json::<T>().await.map_err(|e| anyhow::anyhow!(e))
    }

    // Example method: List Datastores (requires universe_id)
    pub async fn list_datastores(&self, universe_id: u64, cursor: Option<String>, limit: Option<u32>) -> Result<serde_json::Value> {
        let mut path = format!("/datastores/v1/universes/{}/standard-datastores", universe_id);
        
        let mut params = Vec::new();
        if let Some(c) = cursor {
            params.push(format!("cursor={}", c));
        }
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }
        
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        self.get(&path).await
    }

    pub async fn ping(&self, universe_id: u64) -> Result<()> {
        // Minimal request to verify connectivity and auth
        self.list_datastores(universe_id, None, Some(1)).await.map(|_| ())
    }
}

