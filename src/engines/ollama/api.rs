//! Wire types for the Ollama HTTP API. Deserialize only — no logic, no chrono.
//!
//! Mirrors `ollama/api/types.go`. Two rules that are load-bearing here:
//!
//! * Never add `deny_unknown_fields`. Ollama adds fields between minor
//!   releases and pgxtop must keep parsing across upgrades.
//! * Every `Vec` that Go might marshal from a nil slice must be
//!   `Option<Vec<_>>`. `#[serde(default)]` only covers a *missing* field; an
//!   explicit JSON `null` against `Vec<String>` is a hard error.

use serde::Deserialize;

/// `GET /api/ps` — mirrors `api.ProcessResponse`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PsResponse {
    #[serde(default)]
    pub models: Vec<PsModel>,
}

/// Mirrors `api.ProcessModelResponse`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PsModel {
    /// Always present. The `ollama ps` NAME column.
    #[serde(default)]
    pub name: String,
    /// Always present. Underlying model ref; in practice equal to `name`.
    #[serde(default)]
    pub model: String,
    /// Always present. Total runner footprint in BYTES (VRAM + host RAM).
    /// This is the `ollama ps` SIZE column — it is NOT the VRAM figure.
    #[serde(default)]
    pub size: u64,
    /// Always present. Full sha256 hex of the manifest.
    #[serde(default)]
    pub digest: String,
    /// Always present in practice (Go's `omitempty` is a no-op on structs).
    #[serde(default)]
    pub details: ModelDetails,
    /// Always present, RFC3339Nano with an offset.
    ///
    /// Deliberately a raw `String`: chrono's `serde` feature is not enabled in
    /// Cargo.toml, and keeping it a string means a malformed timestamp costs
    /// one model its UNTIL column instead of failing the whole response.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Always present. Bytes resident in VRAM; 0 means a pure-CPU runner.
    #[serde(default)]
    pub size_vram: u64,
    /// Newer Ollama only (verified present in 0.32.14). The num_ctx the runner
    /// was actually loaded with.
    #[serde(default)]
    pub context_length: Option<u32>,
}

/// Mirrors `api.ModelDetails`. Shared by /api/ps, /api/tags and /api/show.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModelDetails {
    #[serde(default)]
    pub parent_model: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub family: String,
    /// Go marshals a nil slice as `null`, not `[]` — must stay an Option.
    #[serde(default)]
    pub families: Option<Vec<String>>,
    #[serde(default)]
    pub parameter_size: String,
    #[serde(default)]
    pub quantization_level: String,
    /// Newer Ollama only. The model's architectural max context — distinct
    /// from `PsModel::context_length`, which is the loaded num_ctx.
    #[serde(default)]
    pub context_length: Option<u32>,
    /// Newer Ollama only.
    #[serde(default)]
    pub embedding_length: Option<u32>,
}

/// `GET /api/tags` — the on-disk catalogue.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TagsResponse {
    #[serde(default)]
    pub models: Vec<TagsModel>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TagsModel {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub modified_at: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub digest: String,
    #[serde(default)]
    pub details: ModelDetails,
    /// Verified present in 0.32.14: ["completion", "tools", "thinking"].
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
}

/// `POST /api/show` — always sent without `verbose`, otherwise the response
/// carries megabyte-sized tokenizer arrays.
///
/// `model_info` stays an untyped map here and is projected into a small typed
/// struct immediately (see `map::project_show`); `modelfile`, `license`,
/// `template` and `tensors` are never deserialized at all.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShowResponse {
    #[serde(default)]
    pub details: ModelDetails,
    #[serde(default)]
    pub model_info: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    /// Whitespace-formatted Modelfile parameter blob, not JSON.
    #[serde(default)]
    pub parameters: Option<String>,
}

/// Parses a `/api/ps` body. Pure — unit-testable with no network.
pub fn parse_ps(body: &str) -> Result<PsResponse, serde_json::Error> {
    serde_json::from_str(body)
}

pub fn parse_tags(body: &str) -> Result<TagsResponse, serde_json::Error> {
    serde_json::from_str(body)
}

pub fn parse_show(body: &str) -> Result<ShowResponse, serde_json::Error> {
    serde_json::from_str(body)
}
