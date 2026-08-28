use std::time::Instant;

use crate::models::inference::{
    EngineStatus, EngineType, InferenceEngine, InferenceMetrics, ModelInstance, ModelStatus,
};

pub async fn fetch_models(url: &str) -> (Vec<ModelInstance>, EngineStatus) {
    let client = reqwest::Client::new();
    let endpoint = format!("{}/v1/models", url);

    match client.get(&endpoint).timeout(std::time::Duration::from_secs(5)).send().await {
        Ok(response) => {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                (parse_models(&data), EngineStatus::Connected)
            } else {
                (Vec::new(), EngineStatus::Unavailable)
            }
        }
        Err(_) => (Vec::new(), EngineStatus::Unavailable),
    }
}

fn parse_models(data: &serde_json::Value) -> Vec<ModelInstance> {
    let mut models = Vec::new();

    if let Some(models_array) = data.get("data").and_then(|m| m.as_array()) {
        for model in models_array {
            let name = model.get("id").and_then(|n| n.as_str()).unwrap_or("unknown").to_string();

            models.push(ModelInstance {
                id: name.clone(),
                name,
                engine_id: "vllm".to_string(),
                vram_usage: None,
                cpu_usage: None,
                gpu_usage: None,
                quantization: None,
                context_size: None,
                status: ModelStatus::Active,
                digest: None,
                size: None,
                expires_at: None,
            });
        }
    }

    models
}

pub async fn fetch_metrics(url: &str) -> InferenceMetrics {
    // Try to fetch Prometheus metrics
    let client = reqwest::Client::new();
    let endpoint = format!("{}/metrics", url);

    match client.get(&endpoint).timeout(std::time::Duration::from_secs(5)).send().await {
        Ok(response) => {
            if let Ok(text) = response.text().await {
                parse_metrics(&text)
            } else {
                default_metrics()
            }
        }
        Err(_) => default_metrics(),
    }
}

fn parse_metrics(text: &str) -> InferenceMetrics {
    let mut metrics = default_metrics();

    for line in text.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some((key, value)) = line.split_once(' ') {
            let value = value.trim();
            if let Ok(val) = value.parse::<f32>() {
                match key {
                    "vllm:num_requests_running" => metrics.active_requests = Some(val as u32),
                    "vllm:num_requests_waiting" => metrics.waiting_requests = Some(val as u32),
                    "vllm:request_throughput" => metrics.request_throughput = Some(val),
                    "vllm:prompt_tokens_per_second" => metrics.prompt_tokens_per_sec = Some(val),
                    "vllm:generation_tokens_per_second" => metrics.generation_tokens_per_sec = Some(val),
                    _ => {}
                }
            }
        }
    }

    metrics
}

fn default_metrics() -> InferenceMetrics {
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