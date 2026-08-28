use std::time::Instant;

use crate::models::inference::{
    EngineStatus, EngineType, InferenceEngine, InferenceMetrics, ModelInstance, ModelStatus,
};

pub async fn fetch_models(url: &str) -> Vec<ModelInstance> {
    let client = reqwest::Client::new();
    let endpoint = format!("{}/api/ps", url);

    match client.get(&endpoint).timeout(std::time::Duration::from_secs(5)).send().await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                parse_models(&data)
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    }
}

fn parse_models(data: &serde_json::Value) -> Vec<ModelInstance> {
    let mut models = Vec::new();

    if let Some(models_array) = data.get("models").and_then(|m| m.as_array()) {
        for model in models_array {
            let name = model.get("name").and_then(|n| n.as_str()).unwrap_or("unknown").to_string();
            let size = model.get("size").and_then(|s| s.as_u64());
            let vram = model.get("size").and_then(|s| s.as_u64());

            models.push(ModelInstance {
                id: name.clone(),
                name,
                engine_id: "ollama".to_string(),
                vram_usage: vram,
                cpu_usage: None,
                gpu_usage: None,
                quantization: None,
                context_size: None,
                status: ModelStatus::Loaded,
                digest: model.get("digest").and_then(|d| d.as_str()).map(|s| s.to_string()),
                size,
                expires_at: None,
            });
        }
    }

    models
}

pub async fn fetch_metrics(url: &str) -> InferenceMetrics {
    InferenceMetrics {
        timestamp: Instant::now(),
        active_requests: None,
        waiting_requests: None,
        request_throughput: None,
        prompt_tokens_per_sec: None,
        generation_tokens_per_sec: None,
        kv_cache_utilization: None,
        request_latency_ms: None,
        time_to_first_token_ms: None,
        avg_tokens_per_request: None,
    }
}