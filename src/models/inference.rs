use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceEngine {
    pub id: String,
    pub name: String,
    pub engine_type: EngineType,
    pub url: String,
    pub status: EngineStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineType {
    Ollama,
    Vllm,
    LlamaCpp,
    TensorRTLlm,
    Sglang,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineStatus {
    Connected,
    Unavailable,
    Connecting,
}

#[derive(Debug, Clone)]
pub struct ModelInstance {
    pub id: String,
    pub name: String,
    pub engine_id: String,
    pub vram_usage: Option<u64>,
    pub cpu_usage: Option<f32>,
    pub gpu_usage: Option<f32>,
    pub quantization: Option<String>,
    pub context_size: Option<u64>,
    pub status: ModelStatus,
    pub digest: Option<String>,
    pub size: Option<u64>,
    pub expires_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelStatus {
    Loaded,
    Active,
    Idle,
    Unloading,
}

#[derive(Debug, Clone)]
pub struct InferenceMetrics {
    pub timestamp: Instant,
    pub active_requests: Option<u32>,
    pub waiting_requests: Option<u32>,
    pub request_throughput: Option<f32>,
    pub prompt_tokens_per_sec: Option<f32>,
    pub generation_tokens_per_sec: Option<f32>,
    pub kv_cache_utilization: Option<f32>,
    pub request_latency_ms: Option<f32>,
    pub time_to_first_token_ms: Option<f32>,
    pub avg_tokens_per_request: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct InferenceHistory {
    pub prompt_tok_s: Vec<f32>,
    pub gen_tok_s: Vec<f32>,
    pub active_requests: Vec<u32>,
    pub max_points: usize,
}

impl InferenceHistory {
    pub fn new(max_points: usize) -> Self {
        Self {
            prompt_tok_s: Vec::with_capacity(max_points),
            gen_tok_s: Vec::with_capacity(max_points),
            active_requests: Vec::with_capacity(max_points),
            max_points,
        }
    }

    pub fn push(&mut self, metrics: &InferenceMetrics) {
        if let Some(v) = metrics.prompt_tokens_per_sec {
            self.prompt_tok_s.push(v);
        }
        if let Some(v) = metrics.generation_tokens_per_sec {
            self.gen_tok_s.push(v);
        }
        if let Some(v) = metrics.active_requests {
            self.active_requests.push(v);
        }

        if self.prompt_tok_s.len() > self.max_points {
            let excess = self.prompt_tok_s.len() - self.max_points;
            self.prompt_tok_s.drain(0..excess);
            self.gen_tok_s.drain(0..excess);
            self.active_requests.drain(0..excess);
        }
    }
}