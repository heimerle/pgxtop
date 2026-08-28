use std::time::Instant;

use crate::engines::{InferenceEngine, ModelInstance, InferenceMetrics, EngineStatus};

pub struct InferenceCollector {
    engines: Vec<InferenceEngine>,
}

impl InferenceCollector {
    pub fn new() -> Self {
        Self {
            engines: Vec::new(),
        }
    }

    pub fn add_engine(&mut self, engine: InferenceEngine) {
        self.engines.push(engine);
    }

    pub async fn collect(&mut self) -> Vec<(InferenceEngine, Vec<ModelInstance>, InferenceMetrics)> {
        let mut results = Vec::new();

        for engine in &mut self.engines {
            let (models, status) = Self::collect_models(engine).await;
            engine.status = status;
            let metrics = Self::collect_metrics(engine).await;
            results.push((engine.clone(), models, metrics));
        }

        results
    }

    async fn collect_models(engine: &InferenceEngine) -> (Vec<ModelInstance>, EngineStatus) {
        match engine.engine_type {
            crate::engines::EngineType::Ollama => {
                crate::engines::ollama::fetch_models(&engine.url).await
            }
            crate::engines::EngineType::Vllm => {
                crate::engines::vllm::fetch_models(&engine.url).await
            }
            _ => (Vec::new(), EngineStatus::Unavailable),
        }
    }

    async fn collect_metrics(engine: &InferenceEngine) -> InferenceMetrics {
        match engine.engine_type {
            crate::engines::EngineType::Ollama => {
                crate::engines::ollama::fetch_metrics(&engine.url).await
            }
            crate::engines::EngineType::Vllm => {
                crate::engines::vllm::fetch_metrics(&engine.url).await
            }
            _ => InferenceMetrics {
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
            },
        }
    }
}