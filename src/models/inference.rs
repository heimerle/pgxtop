use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Datelike, Utc};

use crate::models::series::Series;
use serde::{Deserialize, Serialize};

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

impl EngineStatus {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Connected => "●",
            Self::Unavailable => "○",
            Self::Connecting => "◐",
        }
    }
}

// ---------------------------------------------------------------------------
// CPU/GPU placement
// ---------------------------------------------------------------------------

/// How a loaded model is split between VRAM and host RAM.
///
/// Exact port of ollama's `ListRunningHandler` (`cmd/cmd.go`), including the
/// branch *order*: `size_vram == 0` is tested before the `size == 0` guard, so
/// `(0, 0)` yields `AllCpu` rather than `Unknown`. Do not "tidy" that into a
/// match on both values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessorSplit {
    AllCpu,
    AllGpu,
    /// Indeterminate — `size_vram > size`, or `size == 0` with VRAM in use.
    Unknown,
    /// Invariant: `cpu_pct + gpu_pct == 100`.
    Split { cpu_pct: u8, gpu_pct: u8 },
}

impl ProcessorSplit {
    pub fn from_sizes(size: u64, size_vram: u64) -> Self {
        if size_vram == 0 {
            Self::AllCpu
        } else if size_vram == size {
            Self::AllGpu
        } else if size_vram > size || size == 0 {
            Self::Unknown
        } else {
            let size_cpu = size - size_vram;
            // Go's math.Round and Rust's f64::round both round half away from
            // zero, so this matches the original exactly.
            let cpu_pct = (((size_cpu as f64 / size as f64) * 100.0).round() as u8).min(100);
            Self::Split { cpu_pct, gpu_pct: 100 - cpu_pct }
        }
    }

    /// Fraction resident in VRAM, `0.0..=1.0`. `None` when indeterminate.
    pub fn gpu_fraction(self) -> Option<f32> {
        match self {
            Self::AllCpu => Some(0.0),
            Self::AllGpu => Some(1.0),
            Self::Unknown => None,
            Self::Split { gpu_pct, .. } => Some(gpu_pct as f32 / 100.0),
        }
    }

    /// Renders exactly like the `ollama ps` PROCESSOR column.
    pub fn label(self) -> String {
        match self {
            Self::AllCpu => "100% CPU".to_string(),
            Self::AllGpu => "100% GPU".to_string(),
            Self::Unknown => "Unknown".to_string(),
            Self::Split { cpu_pct, gpu_pct } => format!("{cpu_pct}%/{gpu_pct}% CPU/GPU"),
        }
    }
}

// ---------------------------------------------------------------------------
// keep-alive / expiry
// ---------------------------------------------------------------------------

/// Time-*independent* classification of Ollama's `expires_at`.
///
/// Deliberately excludes "expired": that depends on `now` and is decided at
/// render time by `format::until`, so a cached snapshot can never go stale in
/// a way that makes it wrong.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Expiry {
    /// Field absent, or the timestamp did not parse.
    #[default]
    Unknown,
    /// Go's zero time (`0001-01-01T00:00:00Z`). `ollama ps` prints "Never".
    Never,
    /// `keep_alive = -1`. Ollama clamps a negative keep-alive to
    /// `time.Duration(math.MaxInt64)` (~292 years) and sends `now + 292y`.
    Forever,
    /// A real deadline.
    At(DateTime<Utc>),
}

impl Expiry {
    /// `now` is injected so this is deterministic under test.
    pub fn classify(expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Self {
        let Some(t) = expires_at else {
            return Self::Unknown;
        };

        // Go's time.Time zero value. Compare on the year so this also catches
        // the value re-encoded with a non-UTC offset.
        if t.year() <= 1 {
            return Self::Never;
        }

        // Mirrors ollama's format.humanTime:
        //     if int(delta.Hours())/24/365 < -20 { return "Forever" }
        // chrono's num_hours() truncates toward zero exactly like Go's
        // int(float64) conversion, and Rust integer division truncates toward
        // zero exactly like Go's. The effective threshold is a deadline more
        // than 183_960 h (~20.99 years) out.
        let delta = now.signed_duration_since(t);
        if delta.num_hours() / 24 / 365 < -20 {
            return Self::Forever;
        }

        Self::At(t)
    }

    pub fn deadline(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::At(t) => Some(*t),
            _ => None,
        }
    }
}

/// Parses Ollama's RFC3339(-nano, offset-bearing) timestamp.
///
/// Never propagates an error: a garbage timestamp costs one model its UNTIL
/// column instead of blanking the whole list.
pub fn parse_expires_at(raw: Option<&str>) -> Option<DateTime<Utc>> {
    raw.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

// ---------------------------------------------------------------------------
// models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelStatus {
    /// Resident in a runner (from `/api/ps`).
    Loaded,
    /// Actively serving a request.
    Active,
    Idle,
    Unloading,
    /// An OpenAI-compatible endpoint lists this name under `/v1/models`.
    ///
    /// That says the endpoint *offers* the model, not that anything is
    /// resident: on the target host `:8888` is a vllm-semantic-router whose
    /// entries are virtual routes (`"routing":{"resolution":"virtual"}`).
    /// Calling those "loaded" would be exactly the overstatement the spec
    /// forbids.
    Served,
    /// Present on disk but not loaded (from `/api/tags` only).
    Installed,
}

impl ModelStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Unloading => "unloading",
            Self::Served => "served",
            Self::Installed => "installed",
        }
    }

    /// True only when the model actually occupies memory right now.
    pub fn is_resident(self) -> bool {
        matches!(self, Self::Loaded | Self::Active | Self::Idle | Self::Unloading)
    }

    /// Sort rank: what is resident first, then what is merely offered, then
    /// what is only on disk.
    pub fn rank(self) -> u8 {
        match self {
            Self::Loaded | Self::Active | Self::Idle | Self::Unloading => 0,
            Self::Served => 1,
            Self::Installed => 2,
        }
    }
}

/// Deep per-model metadata, fetched lazily from `/api/show` and cached by
/// digest. Kept small on purpose: the raw `model_info` map, `modelfile`,
/// `license` and `tensors` are projected away at parse time.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelDetail {
    pub architecture: Option<String>,
    pub parameter_count: Option<u64>,
    pub size_label: Option<String>,
    /// `<arch>.context_length` — the architectural maximum.
    pub context_length: Option<u32>,
    pub embedding_length: Option<u32>,
    pub block_count: Option<u32>,
    pub head_count: Option<u32>,
    pub head_count_kv: Option<u32>,
    pub expert_count: Option<u32>,
    pub expert_used_count: Option<u32>,
    /// Modelfile parameters. `stop` may legitimately appear many times, so
    /// this is a Vec of pairs rather than a map.
    pub parameters: Vec<(String, String)>,
}

/// Mirrors what the engines actually report. A few fields are carried for the
/// detail overlay and for engines other than Ollama.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ModelInstance {
    /// Stable selection key: `"{engine_id}/{name}"`. Must survive a refresh.
    pub id: String,
    pub name: String,
    /// `/api/ps` `.model`; usually equal to `name`.
    pub model_ref: Option<String>,
    pub engine_id: String,

    // ---- sizes, all BYTES ----
    /// `/api/ps` `.size` — total runner footprint (VRAM + host RAM).
    /// This is the `ollama ps` SIZE column.
    pub size_total: Option<u64>,
    /// `/api/ps` `.size_vram` — the part resident in VRAM.
    pub size_vram: Option<u64>,
    /// Derived: `size_total - size_vram`.
    pub size_cpu: Option<u64>,

    pub processor: Option<ProcessorSplit>,

    /// Full sha256 hex of the manifest.
    pub digest: Option<String>,

    // ---- details, from /api/ps and /api/tags ----
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
    pub family: Option<String>,
    pub families: Option<Vec<String>>,
    pub format: Option<String>,
    pub parent_model: Option<String>,
    /// From `/api/tags`: "completion", "tools", "vision", "thinking", ...
    pub capabilities: Vec<String>,

    // ---- context ----
    /// num_ctx this runner was actually loaded with (`ollama ps` CONTEXT).
    pub context_size: Option<u32>,
    /// The model's architectural maximum context.
    pub context_max: Option<u32>,

    // ---- lifecycle ----
    pub expiry: Expiry,
    pub status: ModelStatus,

    /// Lazy `/api/show` enrichment. `None` means "not fetched yet", which the
    /// UI renders as blank rather than as "unavailable".
    pub detail: Option<Arc<ModelDetail>>,
}

impl Default for ModelInstance {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            model_ref: None,
            engine_id: String::new(),
            size_total: None,
            size_vram: None,
            size_cpu: None,
            processor: None,
            digest: None,
            parameter_size: None,
            quantization: None,
            family: None,
            families: None,
            format: None,
            parent_model: None,
            capabilities: Vec::new(),
            context_size: None,
            context_max: None,
            expiry: Expiry::Unknown,
            status: ModelStatus::Loaded,
            detail: None,
        }
    }
}

// ---------------------------------------------------------------------------
// metrics + history
// ---------------------------------------------------------------------------

/// Not every field has a renderer yet; they are kept because the vLLM scrape
/// already produces them and the spec lists them as targets.
#[allow(dead_code)]
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

impl Default for InferenceMetrics {
    fn default() -> Self {
        Self {
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
}

#[derive(Debug, Clone)]
pub struct InferenceHistory {
    pub prompt_tok_s: Series,
    pub gen_tok_s: Series,
    pub active_requests: Series,
    pub max_points: usize,
}

impl InferenceHistory {
    pub fn new(max_points: usize) -> Self {
        Self {
            prompt_tok_s: Series::with_capacity(max_points),
            gen_tok_s: Series::with_capacity(max_points),
            active_requests: Series::with_capacity(max_points),
            max_points,
        }
    }

    pub fn push(&mut self, metrics: &InferenceMetrics) {
        self.prompt_tok_s.push(metrics.prompt_tokens_per_sec, self.max_points);
        self.gen_tok_s.push(metrics.generation_tokens_per_sec, self.max_points);
        self.active_requests
            .push(metrics.active_requests.map(|v| v as f32), self.max_points);
    }

    /// True when no engine has ever reported a throughput sample — the UI uses
    /// this to collapse the (otherwise permanently empty) history panel.
    pub fn has_data(&self) -> bool {
        !self.prompt_tok_s.is_all_missing() || !self.gen_tok_s.is_all_missing()
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, TimeZone};

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    #[test]
    fn processor_split_ports_ollama_exactly() {
        use ProcessorSplit::*;
        // (size, size_vram) -> expected
        let cases: &[(u64, u64, ProcessorSplit)] = &[
            // size_vram == 0 is tested FIRST, so this is AllCpu, not Unknown.
            (0, 0, AllCpu),
            (1000, 0, AllCpu),
            (1000, 1000, AllGpu),
            (1000, 1500, Unknown),
            (0, 500, Unknown),
            (1000, 500, Split { cpu_pct: 50, gpu_pct: 50 }),
            (1000, 630, Split { cpu_pct: 37, gpu_pct: 63 }),
            // round(0.1) == 0
            (1000, 999, Split { cpu_pct: 0, gpu_pct: 100 }),
            // round(99.9) == 100 — ollama reports "100%/0% CPU/GPU" even though
            // one byte really is on the GPU. Faithful port keeps the quirk.
            (1000, 1, Split { cpu_pct: 100, gpu_pct: 0 }),
        ];
        for &(size, vram, expected) in cases {
            assert_eq!(
                ProcessorSplit::from_sizes(size, vram),
                expected,
                "size={size} size_vram={vram}"
            );
        }
    }

    #[test]
    fn processor_split_labels() {
        assert_eq!(ProcessorSplit::AllGpu.label(), "100% GPU");
        assert_eq!(ProcessorSplit::AllCpu.label(), "100% CPU");
        assert_eq!(ProcessorSplit::Unknown.label(), "Unknown");
        assert_eq!(
            ProcessorSplit::Split { cpu_pct: 37, gpu_pct: 63 }.label(),
            "37%/63% CPU/GPU"
        );
    }

    #[test]
    fn processor_split_gpu_fraction_cannot_drift() {
        assert_eq!(ProcessorSplit::AllGpu.gpu_fraction(), Some(1.0));
        assert_eq!(ProcessorSplit::AllCpu.gpu_fraction(), Some(0.0));
        assert_eq!(ProcessorSplit::Unknown.gpu_fraction(), None);
        assert_eq!(
            ProcessorSplit::Split { cpu_pct: 37, gpu_pct: 63 }.gpu_fraction(),
            Some(0.63)
        );
    }

    #[test]
    fn expiry_classification() {
        let now = t0();
        assert_eq!(Expiry::classify(None, now), Expiry::Unknown);

        let zero = Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(Expiry::classify(Some(zero), now), Expiry::Never);

        // keep_alive = -1 => now + ~292 years.
        let forever = now + TimeDelta::days(292 * 365);
        assert_eq!(Expiry::classify(Some(forever), now), Expiry::Forever);

        // Comfortably past the ~20.99y threshold.
        let far = now + TimeDelta::days(21 * 365 + 10);
        assert_eq!(Expiry::classify(Some(far), now), Expiry::Forever);

        // Comfortably below it.
        let near = now + TimeDelta::days(19 * 365);
        assert_eq!(Expiry::classify(Some(near), now), Expiry::At(near));

        let soon = now + TimeDelta::minutes(4);
        assert_eq!(Expiry::classify(Some(soon), now), Expiry::At(soon));
    }

    #[test]
    fn parse_expires_at_handles_real_and_broken_input() {
        // The exact value measured on the PGX: RFC3339 with nanos and offset.
        let parsed = parse_expires_at(Some("2026-08-29T11:46:18.476933021+02:00"));
        // +02:00 on the wire, normalised to UTC.
        assert!(parsed
            .expect("must parse")
            .to_rfc3339()
            .starts_with("2026-08-29T09:46:18"));

        assert_eq!(parse_expires_at(Some("not-a-date")), None);
        assert_eq!(parse_expires_at(Some("")), None);
        assert_eq!(parse_expires_at(None), None);
    }

    /// Regression: the previous conditional-push / unconditional-trim scheme
    /// panicked here with "end drain index 3 should be <= len 0".
    #[test]
    fn history_push_with_partial_metrics_does_not_panic() {
        let mut h = InferenceHistory::new(2);
        for _ in 0..5 {
            h.push(&InferenceMetrics {
                prompt_tokens_per_sec: Some(1.0),
                generation_tokens_per_sec: None,
                ..Default::default()
            });
        }
        assert_eq!(h.prompt_tok_s.len(), 2);
        // Index-aligned: the missing series is padded, not left empty.
        assert_eq!(h.gen_tok_s.len(), 2);
        assert_eq!(h.active_requests.len(), 2);
        assert!(h.gen_tok_s.is_all_missing());
        assert!(!h.prompt_tok_s.is_all_missing());
        assert!(h.has_data());
    }

    #[test]
    fn history_with_no_data_reports_no_data() {
        let mut h = InferenceHistory::new(8);
        for _ in 0..4 {
            h.push(&InferenceMetrics::default());
        }
        assert!(!h.has_data());
    }
}
