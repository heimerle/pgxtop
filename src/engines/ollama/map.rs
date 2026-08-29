//! Wire types -> domain types. Pure functions, no I/O, `now` always injected.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::api;
use crate::models::inference::{
    parse_expires_at, Expiry, ModelDetail, ModelInstance, ModelStatus, ProcessorSplit,
};

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn details_common(d: &api::ModelDetails, m: &mut ModelInstance) {
    m.parameter_size = non_empty(&d.parameter_size);
    m.quantization = non_empty(&d.quantization_level);
    m.family = non_empty(&d.family);
    m.families = d.families.clone().filter(|v| !v.is_empty());
    m.format = non_empty(&d.format);
    m.parent_model = non_empty(&d.parent_model);
}

/// `/api/ps` -> the loaded models.
pub fn map_ps(resp: &api::PsResponse, engine_id: &str, now: DateTime<Utc>) -> Vec<ModelInstance> {
    resp.models
        .iter()
        .map(|m| {
            let mut out = ModelInstance {
                id: format!("{engine_id}/{}", m.name),
                name: m.name.clone(),
                model_ref: non_empty(&m.model),
                engine_id: engine_id.to_string(),
                size_total: Some(m.size),
                size_vram: Some(m.size_vram),
                size_cpu: Some(m.size.saturating_sub(m.size_vram)),
                processor: Some(ProcessorSplit::from_sizes(m.size, m.size_vram)),
                digest: non_empty(&m.digest),
                context_size: m.context_length,
                context_max: m.details.context_length,
                expiry: Expiry::classify(parse_expires_at(m.expires_at.as_deref()), now),
                // /api/ps only ever lists loaded runners. Promoting to Active
                // would need request-level data Ollama does not expose here.
                status: ModelStatus::Loaded,
                ..Default::default()
            };
            details_common(&m.details, &mut out);
            out
        })
        .collect()
}

/// `/api/tags` -> the on-disk catalogue, all marked `Installed`.
pub fn map_tags(resp: &api::TagsResponse, engine_id: &str) -> Vec<ModelInstance> {
    resp.models
        .iter()
        .map(|m| {
            let mut out = ModelInstance {
                id: format!("{engine_id}/{}", m.name),
                name: m.name.clone(),
                model_ref: non_empty(&m.model),
                engine_id: engine_id.to_string(),
                size_total: Some(m.size),
                digest: non_empty(&m.digest),
                context_max: m.details.context_length,
                capabilities: m.capabilities.clone().unwrap_or_default(),
                status: ModelStatus::Installed,
                ..Default::default()
            };
            details_common(&m.details, &mut out);
            out
        })
        .collect()
}

/// Merge the loaded set with the catalogue.
///
/// Loaded models win and are enriched with the catalogue's `capabilities` and
/// architectural max context (which older Ollama omits from `/api/ps`).
/// Catalogue entries that are not loaded are appended as `Installed`.
pub fn merge(loaded: Vec<ModelInstance>, installed: Vec<ModelInstance>) -> Vec<ModelInstance> {
    let by_name: HashMap<&str, &ModelInstance> =
        installed.iter().map(|m| (m.name.as_str(), m)).collect();

    let mut out: Vec<ModelInstance> = loaded
        .into_iter()
        .map(|mut m| {
            if let Some(cat) = by_name.get(m.name.as_str()) {
                if m.capabilities.is_empty() {
                    m.capabilities = cat.capabilities.clone();
                }
                m.context_max = m.context_max.or(cat.context_max);
                m.parameter_size = m.parameter_size.take().or_else(|| cat.parameter_size.clone());
                m.quantization = m.quantization.take().or_else(|| cat.quantization.clone());
                m.family = m.family.take().or_else(|| cat.family.clone());
            }
            m
        })
        .collect();

    let loaded_names: Vec<String> = out.iter().map(|m| m.name.clone()).collect();
    for m in installed {
        if !loaded_names.contains(&m.name) {
            out.push(m);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// /api/show projection
// ---------------------------------------------------------------------------

fn as_u64(v: Option<&serde_json::Value>) -> Option<u64> {
    v.and_then(|v| v.as_u64())
}

fn as_u32(v: Option<&serde_json::Value>) -> Option<u32> {
    as_u64(v).and_then(|n| u32::try_from(n).ok())
}

/// Projects the ~60-key `model_info` map into a small typed struct and drops
/// the map. Architecture-prefixed keys are resolved via `general.architecture`.
pub fn project_show(resp: &api::ShowResponse) -> ModelDetail {
    let mi = resp.model_info.clone().unwrap_or_default();
    let arch = mi
        .get("general.architecture")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let k = |suffix: &str| -> Option<&serde_json::Value> {
        if arch.is_empty() {
            None
        } else {
            mi.get(&format!("{arch}.{suffix}"))
        }
    };

    ModelDetail {
        architecture: if arch.is_empty() { None } else { Some(arch.clone()) },
        parameter_count: as_u64(mi.get("general.parameter_count")),
        size_label: mi
            .get("general.size_label")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        context_length: as_u32(k("context_length")),
        embedding_length: as_u32(k("embedding_length")),
        block_count: as_u32(k("block_count")),
        head_count: as_u32(k("attention.head_count")),
        head_count_kv: as_u32(k("attention.head_count_kv")),
        expert_count: as_u32(k("expert_count")),
        expert_used_count: as_u32(k("expert_used_count")),
        parameters: parse_parameters(resp.parameters.as_deref().unwrap_or_default()),
    }
}

/// The `parameters` field is a whitespace-formatted Modelfile blob, not JSON.
/// `stop` may legitimately repeat, so the result is a Vec of pairs.
pub fn parse_parameters(blob: &str) -> Vec<(String, String)> {
    blob.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (k, v) = line.split_once(char::is_whitespace)?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format;
    use chrono::TimeZone;

    const PS_NEW: &str = include_str!("fixtures/ps_new.json");
    const PS_OLD: &str = include_str!("fixtures/ps_old.json");
    const PS_EMPTY: &str = include_str!("fixtures/ps_empty.json");
    const PS_FOREVER: &str = include_str!("fixtures/ps_forever.json");
    const PS_NEVER: &str = include_str!("fixtures/ps_never.json");
    const PS_EXPIRED: &str = include_str!("fixtures/ps_expired.json");
    const PS_MALFORMED_TIME: &str = include_str!("fixtures/ps_malformed_time.json");
    const PS_UNKNOWN_FIELDS: &str = include_str!("fixtures/ps_unknown_fields.json");
    const TAGS: &str = include_str!("fixtures/tags.json");
    const SHOW: &str = include_str!("fixtures/show_moe.json");
    const SHOW_NO_INFO: &str = include_str!("fixtures/show_no_model_info.json");

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    fn ps(body: &str) -> Vec<ModelInstance> {
        map_ps(&api::parse_ps(body).expect("parse"), "ollama", now())
    }

    /// The real response measured on the PGX (Ollama 0.32.14).
    #[test]
    fn parses_real_pgx_response() {
        let models = ps(PS_NEW);
        assert_eq!(models.len(), 2);

        let m = &models[0];
        assert_eq!(m.name, "example-moe:q8_0");
        assert_eq!(m.id, "ollama/example-moe:q8_0");
        assert_eq!(m.size_total, Some(96_261_027_921));
        // The bug this replaces: VRAM used to be filled from `size`.
        assert_eq!(m.size_vram, Some(96_261_027_921));
        assert_eq!(m.size_cpu, Some(0));
        assert_eq!(m.processor, Some(ProcessorSplit::AllGpu));
        assert_eq!(m.context_size, Some(262_144));
        assert_eq!(m.parameter_size.as_deref(), Some("120B"));
        assert_eq!(m.quantization.as_deref(), Some("Q8_0"));
        assert_eq!(m.family.as_deref(), Some("examplemoe"));
        assert_eq!(m.format.as_deref(), Some("gguf"));
        // parent_model is "" on the wire and must not become Some("").
        assert_eq!(m.parent_model, None);
        assert_eq!(m.status, ModelStatus::Loaded);

        // Renders exactly like `ollama ps`.
        assert_eq!(format::bytes_si(m.size_total.unwrap()), "96 GB");
        assert_eq!(m.processor.unwrap().label(), "100% GPU");
        assert_eq!(format::context(m.context_size.unwrap()), "256K");
    }

    #[test]
    fn mixed_cpu_gpu_model_reports_the_split() {
        let models = ps(PS_NEW);
        let m = &models[1];
        assert_eq!(m.size_total, Some(1000));
        assert_eq!(m.size_vram, Some(630));
        assert_eq!(m.size_cpu, Some(370));
        assert_eq!(
            m.processor,
            Some(ProcessorSplit::Split { cpu_pct: 37, gpu_pct: 63 })
        );
        assert_eq!(m.processor.unwrap().label(), "37%/63% CPU/GPU");
        assert_eq!(m.context_max, Some(131_072));
    }

    /// Older Ollama: no `context_length` anywhere, and `"families": null`,
    /// which a bare `Vec<String>` would reject outright.
    #[test]
    fn parses_older_ollama_without_context_length_and_null_families() {
        let models = ps(PS_OLD);
        assert_eq!(models.len(), 1);
        let m = &models[0];
        assert_eq!(m.context_size, None);
        assert_eq!(m.context_max, None);
        assert_eq!(m.families, None);
        assert_eq!(m.processor, Some(ProcessorSplit::AllGpu));
        assert_eq!(format::context_opt(m.context_size), format::NA);
    }

    #[test]
    fn empty_response_yields_no_models() {
        assert!(ps(PS_EMPTY).is_empty());
    }

    #[test]
    fn expiry_sentinels_round_trip_to_labels() {
        assert_eq!(ps(PS_FOREVER)[0].expiry, Expiry::Forever);
        assert_eq!(format::until(&ps(PS_FOREVER)[0].expiry, now()), "Forever");

        assert_eq!(ps(PS_NEVER)[0].expiry, Expiry::Never);
        assert_eq!(format::until(&ps(PS_NEVER)[0].expiry, now()), "Never");

        let expired = &ps(PS_EXPIRED)[0];
        assert!(matches!(expired.expiry, Expiry::At(_)));
        // What `ollama ps` prints for a runner past its keep-alive.
        assert_eq!(format::until(&expired.expiry, now()), "Stopping...");
        assert_eq!(expired.processor, Some(ProcessorSplit::AllCpu));
    }

    /// A broken timestamp must cost one model its UNTIL column, not the list.
    #[test]
    fn malformed_timestamp_degrades_one_field_only() {
        let models = ps(PS_MALFORMED_TIME);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].expiry, Expiry::Unknown);
        assert_eq!(models[0].name, "broken-clock:latest");
        assert_eq!(models[0].context_size, Some(4096));
        assert_eq!(format::until(&models[0].expiry, now()), format::NA);
    }

    /// Guards against anyone adding `deny_unknown_fields`.
    #[test]
    fn unknown_fields_from_a_future_release_are_ignored() {
        let models = ps(PS_UNKNOWN_FIELDS);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "future:latest");
    }

    #[test]
    fn tags_supply_capabilities_and_max_context() {
        let tags = map_tags(&api::parse_tags(TAGS).expect("parse"), "ollama");
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].capabilities, vec!["completion", "tools", "thinking"]);
        assert_eq!(tags[0].context_max, Some(262_144));
        assert_eq!(tags[0].status, ModelStatus::Installed);
        assert_eq!(tags[1].name, "qwen3-coder:30b");
    }

    #[test]
    fn merge_enriches_loaded_and_appends_installed_only() {
        let loaded = ps(PS_NEW);
        let installed = map_tags(&api::parse_tags(TAGS).expect("parse"), "ollama");
        let merged = merge(loaded, installed);

        // examplemoe (loaded), glm-5 (loaded), qwen3-coder (installed only)
        assert_eq!(merged.len(), 3);

        let examplemoe = merged.iter().find(|m| m.name.starts_with("example-moe")).unwrap();
        assert_eq!(examplemoe.status, ModelStatus::Loaded);
        // Capabilities come from /api/tags; /api/ps does not carry them.
        assert_eq!(examplemoe.capabilities, vec!["completion", "tools", "thinking"]);

        let qwen = merged.iter().find(|m| m.name == "qwen3-coder:30b").unwrap();
        assert_eq!(qwen.status, ModelStatus::Installed);
        assert_eq!(qwen.size_vram, None);
        assert_eq!(qwen.processor, None);
    }

    #[test]
    fn show_projection_keeps_only_what_the_detail_panel_needs() {
        let d = project_show(&api::parse_show(SHOW).expect("parse"));
        assert_eq!(d.architecture.as_deref(), Some("examplemoe"));
        assert_eq!(d.parameter_count, Some(120_000_000_000));
        assert_eq!(d.size_label.as_deref(), Some("128x2B"));
        assert_eq!(d.context_length, Some(262_144));
        assert_eq!(d.embedding_length, Some(3072));
        assert_eq!(d.block_count, Some(48));
        assert_eq!(d.head_count_kv, Some(8));
        assert_eq!(d.expert_count, Some(256));
        assert_eq!(d.expert_used_count, Some(10));
        // head_count was absent in the real response.
        assert_eq!(d.head_count, None);
    }

    #[test]
    fn show_without_model_info_still_parses_and_reads_parameters() {
        let resp = api::parse_show(SHOW_NO_INFO).expect("parse");
        let d = project_show(&resp);
        assert_eq!(d.architecture, None);
        assert_eq!(d.context_length, None);
        // `stop` repeats, so the pairs must not be collapsed into a map.
        assert_eq!(
            d.parameters,
            vec![
                ("num_ctx".to_string(), "8192".to_string()),
                ("stop".to_string(), "\"<|im_end|>\"".to_string()),
                ("stop".to_string(), "\"<|endoftext|>\"".to_string()),
                ("temperature".to_string(), "0.7".to_string()),
            ]
        );
    }
}
