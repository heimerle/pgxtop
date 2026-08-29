pub mod api;
pub mod map;
pub mod show_cache;

use std::time::Duration;

use chrono::Utc;

use super::ProbeError;
use crate::models::inference::{ModelDetail, ModelInstance};

/// A client bound to one Ollama endpoint.
///
/// Owns its `reqwest::Client` so connections are pooled across polls — the
/// previous code built a fresh client on every 500 ms tick.
#[derive(Clone)]
pub struct OllamaClient {
    client: reqwest::Client,
    base: String,
    engine_id: String,
    timeout: Duration,
    show_timeout: Duration,
}

impl OllamaClient {
    pub fn new(
        client: reqwest::Client,
        base: impl Into<String>,
        engine_id: impl Into<String>,
        timeout: Duration,
        show_timeout: Duration,
    ) -> Self {
        Self {
            client,
            base: base.into().trim_end_matches('/').to_string(),
            engine_id: engine_id.into(),
            timeout,
            show_timeout,
        }
    }

    async fn get(&self, path: &str, timeout: Duration) -> Result<String, ProbeError> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .client
            .get(&url)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| ProbeError::Transport(e.to_string()))?;

        // The old code never checked this, so a 404 from a non-Ollama service
        // on :11434 was reported as "Connected" with zero models.
        if !resp.status().is_success() {
            return Err(ProbeError::Status(resp.status().as_u16()));
        }

        resp.text()
            .await
            .map_err(|e| ProbeError::Transport(e.to_string()))
    }

    /// `GET /api/ps` — the loaded runners.
    pub async fn fetch_ps(&self) -> Result<Vec<ModelInstance>, ProbeError> {
        let body = self.get("/api/ps", self.timeout).await?;
        let resp = api::parse_ps(&body).map_err(|e| {
            tracing::debug!(target: "pgxtop::ollama", "/api/ps parse failed: {e}; body starts: {:.200}", body);
            ProbeError::Malformed(e.to_string())
        })?;
        Ok(map::map_ps(&resp, &self.engine_id, Utc::now()))
    }

    /// `GET /api/tags` — the on-disk catalogue. Polled on a slow cadence.
    pub async fn fetch_tags(&self) -> Result<Vec<ModelInstance>, ProbeError> {
        let body = self.get("/api/tags", self.timeout).await?;
        let resp = api::parse_tags(&body).map_err(|e| {
            tracing::debug!(target: "pgxtop::ollama", "/api/tags parse failed: {e}");
            ProbeError::Malformed(e.to_string())
        })?;
        Ok(map::map_tags(&resp, &self.engine_id))
    }

    /// `POST /api/show` — deep metadata for one model.
    ///
    /// Always sent without `verbose`, otherwise the response carries
    /// megabyte-sized tokenizer arrays. Gets its own, longer timeout because
    /// Ollama reads GGUF metadata off disk to answer it.
    pub async fn fetch_show(&self, model: &str) -> Result<ModelDetail, ProbeError> {
        let url = format!("{}/api/show", self.base);
        let resp = self
            .client
            .post(&url)
            .timeout(self.show_timeout)
            .json(&serde_json::json!({ "model": model }))
            .send()
            .await
            .map_err(|e| ProbeError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProbeError::Status(resp.status().as_u16()));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| ProbeError::Transport(e.to_string()))?;

        let parsed = api::parse_show(&body).map_err(|e| {
            tracing::debug!(target: "pgxtop::ollama", "/api/show parse failed for {model}: {e}");
            ProbeError::Malformed(e.to_string())
        })?;
        Ok(map::project_show(&parsed))
    }
}
