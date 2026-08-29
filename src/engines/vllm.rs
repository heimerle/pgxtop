//! vLLM adapter: `GET /v1/models` and `GET /metrics`.

use std::time::{Duration, Instant};

use super::ProbeError;
use crate::models::inference::{InferenceMetrics, ModelInstance, ModelStatus};

#[derive(Clone)]
pub struct VllmClient {
    client: reqwest::Client,
    base: String,
    engine_id: String,
    timeout: Duration,
}

impl VllmClient {
    pub fn new(
        client: reqwest::Client,
        base: impl Into<String>,
        engine_id: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            client,
            base: base.into().trim_end_matches('/').to_string(),
            engine_id: engine_id.into(),
            timeout,
        }
    }

    async fn get(&self, path: &str) -> Result<String, ProbeError> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| ProbeError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ProbeError::Status(resp.status().as_u16()));
        }
        resp.text()
            .await
            .map_err(|e| ProbeError::Transport(e.to_string()))
    }

    pub async fn fetch_models(&self) -> Result<Vec<ModelInstance>, ProbeError> {
        let body = self.get("/v1/models").await?;
        let data: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ProbeError::Malformed(e.to_string()))?;
        Ok(parse_models(&data, &self.engine_id))
    }

    pub async fn fetch_metrics(&self, prev: Option<&Counters>) -> Result<(InferenceMetrics, Counters), ProbeError> {
        let body = self.get("/metrics").await?;
        Ok(parse_metrics(&body, prev, Instant::now()))
    }
}

pub fn parse_models(data: &serde_json::Value, engine_id: &str) -> Vec<ModelInstance> {
    data.get("data")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|n| n.as_str()))
                .map(|name| ModelInstance {
                    id: format!("{engine_id}/{name}"),
                    name: name.to_string(),
                    engine_id: engine_id.to_string(),
                    // /v1/models says the endpoint offers this name. It says
                    // nothing about residency — on the target host these are
                    // virtual router routes — so neither Active nor Loaded
                    // would be honest.
                    status: ModelStatus::Served,
                    ..Default::default()
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Prometheus
// ---------------------------------------------------------------------------

/// Monotonic counters carried between polls so throughput can be derived.
///
/// vLLM exposes `vllm:prompt_tokens_total` / `vllm:generation_tokens_total` as
/// counters; the per-second figures the old code looked for
/// (`vllm:prompt_tokens_per_second`) do not exist in any vLLM release.
#[derive(Debug, Clone, Copy)]
pub struct Counters {
    pub at: Instant,
    pub prompt_tokens: Option<f64>,
    pub generation_tokens: Option<f64>,
}

/// Splits a Prometheus sample line into its key (name plus labels) and value.
///
/// Splitting on the first space is wrong: a label value may legitimately
/// contain one (`model_name="my model"`). This scans for the first space that
/// is outside both braces and quotes.
fn split_sample(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut in_quotes = false;
    let mut depth = 0usize;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match b {
            b'\\' if in_quotes => escaped = true,
            b'"' => in_quotes = !in_quotes,
            b'{' if !in_quotes => depth += 1,
            b'}' if !in_quotes => depth = depth.saturating_sub(1),
            b' ' if !in_quotes && depth == 0 => {
                return Some((line[..i].trim(), line[i + 1..].trim()));
            }
            _ => {}
        }
    }
    None
}

/// `vllm:num_requests_running{model_name="x"}` -> `vllm:num_requests_running`.
fn metric_name(key: &str) -> &str {
    match key.find('{') {
        Some(i) => &key[..i],
        None => key,
    }
}

pub fn parse_metrics(
    text: &str,
    prev: Option<&Counters>,
    now: Instant,
) -> (InferenceMetrics, Counters) {
    let mut m = InferenceMetrics {
        timestamp: now,
        ..Default::default()
    };
    let mut counters = Counters {
        at: now,
        prompt_tokens: None,
        generation_tokens: None,
    };

    // Histogram accumulators for latency / TTFT.
    let (mut ttft_sum, mut ttft_count) = (None::<f64>, None::<f64>);
    let (mut e2e_sum, mut e2e_count) = (None::<f64>, None::<f64>);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = split_sample(line) else {
            continue;
        };
        let Ok(v) = raw.split_whitespace().next().unwrap_or(raw).parse::<f64>() else {
            continue;
        };

        match metric_name(key) {
            "vllm:num_requests_running" => {
                m.active_requests = Some(m.active_requests.unwrap_or(0) + v as u32)
            }
            "vllm:num_requests_waiting" => {
                m.waiting_requests = Some(m.waiting_requests.unwrap_or(0) + v as u32)
            }
            // 0.0..=1.0 on the wire; the UI wants a percentage.
            "vllm:gpu_cache_usage_perc" => m.kv_cache_utilization = Some(v as f32 * 100.0),
            "vllm:prompt_tokens_total" => {
                counters.prompt_tokens = Some(counters.prompt_tokens.unwrap_or(0.0) + v)
            }
            "vllm:generation_tokens_total" => {
                counters.generation_tokens = Some(counters.generation_tokens.unwrap_or(0.0) + v)
            }
            // Older vLLM exposes these directly; prefer them when present.
            "vllm:avg_prompt_throughput_toks_per_s" => m.prompt_tokens_per_sec = Some(v as f32),
            "vllm:avg_generation_throughput_toks_per_s" => {
                m.generation_tokens_per_sec = Some(v as f32)
            }
            "vllm:time_to_first_token_seconds_sum" => {
                ttft_sum = Some(ttft_sum.unwrap_or(0.0) + v)
            }
            "vllm:time_to_first_token_seconds_count" => {
                ttft_count = Some(ttft_count.unwrap_or(0.0) + v)
            }
            "vllm:e2e_request_latency_seconds_sum" => e2e_sum = Some(e2e_sum.unwrap_or(0.0) + v),
            "vllm:e2e_request_latency_seconds_count" => {
                e2e_count = Some(e2e_count.unwrap_or(0.0) + v)
            }
            _ => {}
        }
    }

    // Derive throughput from the counters when the gauges are absent.
    if let Some(p) = prev {
        let dt = now.duration_since(p.at).as_secs_f64();
        if dt > 0.05 {
            if m.prompt_tokens_per_sec.is_none() {
                m.prompt_tokens_per_sec = rate(p.prompt_tokens, counters.prompt_tokens, dt);
            }
            if m.generation_tokens_per_sec.is_none() {
                m.generation_tokens_per_sec =
                    rate(p.generation_tokens, counters.generation_tokens, dt);
            }
        }
    }

    m.time_to_first_token_ms = mean(ttft_sum, ttft_count).map(|s| (s * 1000.0) as f32);
    m.request_latency_ms = mean(e2e_sum, e2e_count).map(|s| (s * 1000.0) as f32);

    (m, counters)
}

/// Counter delta per second. `None` on a counter reset (a vLLM restart), so a
/// restart shows a gap rather than a fabricated spike.
fn rate(prev: Option<f64>, now: Option<f64>, dt: f64) -> Option<f32> {
    let (a, b) = (prev?, now?);
    if b < a {
        return None;
    }
    Some(((b - a) / dt) as f32)
}

fn mean(sum: Option<f64>, count: Option<f64>) -> Option<f64> {
    let (s, c) = (sum?, count?);
    if c <= 0.0 {
        None
    } else {
        Some(s / c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: &str = r#"
# HELP vllm:num_requests_running Number of requests currently running.
# TYPE vllm:num_requests_running gauge
vllm:num_requests_running{model_name="meta-llama/Llama-3.1-8B"} 3.0
vllm:num_requests_waiting{model_name="meta-llama/Llama-3.1-8B"} 1.0
vllm:gpu_cache_usage_perc{model_name="meta-llama/Llama-3.1-8B"} 0.42
vllm:prompt_tokens_total{model_name="meta-llama/Llama-3.1-8B"} 1000.0
vllm:generation_tokens_total{model_name="meta-llama/Llama-3.1-8B"} 500.0
vllm:e2e_request_latency_seconds_sum{model_name="meta-llama/Llama-3.1-8B"} 12.0
vllm:e2e_request_latency_seconds_count{model_name="meta-llama/Llama-3.1-8B"} 4.0
vllm:time_to_first_token_seconds_sum{model_name="meta-llama/Llama-3.1-8B"} 0.8
vllm:time_to_first_token_seconds_count{model_name="meta-llama/Llama-3.1-8B"} 4.0
"#;

    /// The old parser matched bare metric names and therefore never matched a
    /// real vLLM line, which is why the inference graphs were always empty.
    #[test]
    fn labelled_metrics_are_matched() {
        let (m, c) = parse_metrics(METRICS, None, Instant::now());
        assert_eq!(m.active_requests, Some(3));
        assert_eq!(m.waiting_requests, Some(1));
        assert_eq!(m.kv_cache_utilization, Some(42.0));
        assert_eq!(c.prompt_tokens, Some(1000.0));
        assert_eq!(c.generation_tokens, Some(500.0));
        assert_eq!(m.request_latency_ms, Some(3000.0));
        assert_eq!(m.time_to_first_token_ms, Some(200.0));
        // No previous sample yet, so no rate can be honestly derived.
        assert_eq!(m.prompt_tokens_per_sec, None);
    }

    #[test]
    fn throughput_is_derived_from_counter_deltas() {
        let t0 = Instant::now();
        let (_, c0) = parse_metrics(METRICS, None, t0);

        let later = METRICS
            .replace("vllm:prompt_tokens_total{model_name=\"meta-llama/Llama-3.1-8B\"} 1000.0",
                     "vllm:prompt_tokens_total{model_name=\"meta-llama/Llama-3.1-8B\"} 3000.0")
            .replace("vllm:generation_tokens_total{model_name=\"meta-llama/Llama-3.1-8B\"} 500.0",
                     "vllm:generation_tokens_total{model_name=\"meta-llama/Llama-3.1-8B\"} 900.0");

        let t1 = t0 + Duration::from_secs(2);
        let (m, _) = parse_metrics(&later, Some(&c0), t1);
        assert_eq!(m.prompt_tokens_per_sec, Some(1000.0));
        assert_eq!(m.generation_tokens_per_sec, Some(200.0));
    }

    #[test]
    fn counter_reset_yields_a_gap_not_a_spike() {
        let t0 = Instant::now();
        let (_, c0) = parse_metrics(METRICS, None, t0);
        let restarted = METRICS
            .replace("} 1000.0", "} 5.0")
            .replace("} 500.0", "} 2.0");
        let (m, _) = parse_metrics(&restarted, Some(&c0), t0 + Duration::from_secs(2));
        assert_eq!(m.prompt_tokens_per_sec, None);
        assert_eq!(m.generation_tokens_per_sec, None);
    }

    #[test]
    fn label_values_containing_spaces_are_handled() {
        let line = r#"vllm:num_requests_running{model_name="my model v2"} 7.0"#;
        let (key, value) = split_sample(line).expect("split");
        assert_eq!(metric_name(key), "vllm:num_requests_running");
        assert_eq!(value, "7.0");
    }

    #[test]
    fn multiple_model_series_are_summed() {
        let text = concat!(
            "vllm:num_requests_running{model_name=\"a\"} 2.0\n",
            "vllm:num_requests_running{model_name=\"b\"} 3.0\n"
        );
        let (m, _) = parse_metrics(text, None, Instant::now());
        assert_eq!(m.active_requests, Some(5));
    }

    #[test]
    fn models_endpoint_claims_only_that_the_name_is_served() {
        let data: serde_json::Value =
            serde_json::from_str(r#"{"object":"list","data":[{"id":"glm-5","object":"model"}]}"#)
                .unwrap();
        let models = parse_models(&data, "vllm-local");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "glm-5");
        assert_eq!(models[0].id, "vllm-local/glm-5");
        assert_eq!(models[0].status, ModelStatus::Served);
        assert_eq!(models[0].size_vram, None);
    }
}
