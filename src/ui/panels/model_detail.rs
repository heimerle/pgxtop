//! The per-model detail body, shown in the `Enter` overlay.
//!
//! Pure: returns lines, so it is fully testable without a terminal backend.

use chrono::{DateTime, Utc};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::format;
use crate::models::{GpuProcess, MappingConfidence};
use crate::ui::state::ModelRow;
use crate::ui::theme;

const LABEL_W: usize = 11;

fn kv(label: &str, value: String, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {:<w$} ", label, w = LABEL_W - 2),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(value, style),
    ])
}

fn plain(label: &str, value: String) -> Line<'static> {
    kv(label, value, Style::default().fg(theme::TEXT))
}

fn opt(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| format::NA.to_string())
}

/// Processes attributed to this model, most memory first.
fn attributed<'a>(r: &ModelRow, procs: &'a [GpuProcess]) -> Vec<&'a GpuProcess> {
    let mut out: Vec<&GpuProcess> = procs
        .iter()
        .filter(|p| p.model.as_deref() == Some(r.name.as_str()))
        .collect();
    if out.is_empty() {
        // Fall back to processes of the same engine that could not be pinned
        // to a specific model — shown as `?`, never as a fact.
        out = procs
            .iter()
            .filter(|p| {
                p.model.is_none()
                    && p.engine.as_deref().map(str::to_lowercase)
                        == Some(r.engine_label.to_lowercase())
            })
            .collect();
    }
    out.sort_by_key(|p| std::cmp::Reverse(p.used_memory.unwrap_or(0)));
    out
}

pub fn lines(
    r: &ModelRow,
    procs: &[GpuProcess],
    width: u16,
    now: DateTime<Utc>,
    detail_loading: bool,
) -> Vec<Line<'static>> {
    let val_w = width.saturating_sub(LABEL_W as u16 + 1).max(8) as usize;
    let mut out = Vec::new();

    out.push(kv(
        "NAME",
        format::truncate(&r.name, val_w).into_owned(),
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD),
    ));
    out.push(plain(
        "ENGINE",
        format::truncate(&format!("{} · {}", r.engine_label, r.engine_url), val_w).into_owned(),
    ));
    out.push(kv(
        "ID",
        r.digest
            .as_deref()
            .map(|d| format::digest_short(d).to_string())
            .unwrap_or_else(|| format::NA.to_string()),
        Style::default().fg(theme::MUTED),
    ));

    // --- identity ---
    let arch = r
        .detail
        .as_ref()
        .and_then(|d| d.architecture.clone())
        .or_else(|| r.family.clone());
    out.push(plain("FAMILY", opt(&arch)));
    let params = r
        .parameter_size
        .clone()
        .or_else(|| r.detail.as_ref().and_then(|d| d.size_label.clone()));
    out.push(plain("PARAMS", opt(&params)));
    out.push(plain("QUANT", opt(&r.quantization)));
    out.push(plain("FORMAT", opt(&r.format)));
    if let Some(parent) = &r.parent_model {
        out.push(plain("PARENT", parent.clone()));
    }

    // --- footprint ---
    out.push(plain(
        "SIZE",
        match r.size_total {
            Some(t) => format::bytes_si(t),
            None => format::NA.to_string(),
        },
    ));
    out.push(plain(
        "VRAM",
        match (r.size_vram, r.size_cpu) {
            (Some(v), Some(c)) if c > 0 => {
                format!("{} + {} on CPU", format::bytes_si(v), format::bytes_si(c))
            }
            (Some(v), _) => format::bytes_si(v),
            (None, _) => format::NA.to_string(),
        },
    ));

    // --- placement, the headline signal ---
    match r.gpu_fraction() {
        Some(g) => {
            let bar_w = val_w.saturating_sub(18).clamp(4, 24);
            out.push(Line::from(vec![
                Span::styled(
                    format!(" {:<w$} ", "PLACEMENT", w = LABEL_W - 2),
                    Style::default().fg(theme::MUTED),
                ),
                Span::styled(format::bar(g * 100.0, bar_w), theme::processor_style(Some(g))),
                Span::raw(" "),
                Span::styled(
                    r.processor.map(|p| p.label()).unwrap_or_default(),
                    theme::processor_style(Some(g)),
                ),
            ]));
        }
        None => out.push(kv(
            "PLACEMENT",
            if r.is_resident() {
                format::NA.to_string()
            } else {
                "not loaded".to_string()
            },
            Style::default().fg(theme::MUTED),
        )),
    }

    // --- context: loaded vs the model's maximum ---
    out.push(plain(
        "CONTEXT",
        match (r.context_size, r.context_max.or(r.detail.as_ref().and_then(|d| d.context_length)))
        {
            (Some(loaded), Some(max)) if max > loaded => {
                format!("{} / {} max", format::context(loaded), format::context(max))
            }
            (Some(loaded), _) => format!("{} ({loaded})", format::context(loaded)),
            (None, Some(max)) => format!("max {}", format::context(max)),
            (None, None) => format::NA.to_string(),
        },
    ));

    out.push(plain("UNTIL", format::until(&r.expiry, now)));
    out.push(kv(
        "STATUS",
        if r.stale {
            format!("{} (stale)", r.status.label())
        } else {
            r.status.label().to_string()
        },
        Style::default().fg(if r.stale { theme::WARN } else { theme::TEXT }),
    ));

    if !r.capabilities.is_empty() {
        out.push(kv(
            "CAPS",
            format::truncate(&r.capabilities.join(", "), val_w).into_owned(),
            Style::default().fg(theme::ACCENT),
        ));
    }

    // --- deep metadata, lazily fetched from /api/show on first open ---
    out.push(Line::raw(""));
    match (&r.detail, detail_loading) {
        (Some(d), _) => {
            if let Some(n) = d.parameter_count {
                out.push(plain("PARAM CNT", format!("{n}")));
            }
            let shape: Vec<String> = [
                d.block_count.map(|v| format!("{v} layers")),
                d.embedding_length.map(|v| format!("d_model {v}")),
                d.head_count_kv.map(|v| format!("{v} kv heads")),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !shape.is_empty() {
                out.push(plain("SHAPE", shape.join("  ")));
            }
            if let (Some(total), Some(used)) = (d.expert_count, d.expert_used_count) {
                out.push(plain("EXPERTS", format!("{used} of {total} active")));
            }
            for (k, v) in d.parameters.iter().take(6) {
                out.push(kv(
                    k,
                    format::truncate(v, val_w).into_owned(),
                    Style::default().fg(theme::MUTED),
                ));
            }
        }
        (None, _) if !r.supports_detail => out.push(kv(
            "DETAIL",
            format!("{} exposes no per-model metadata", r.engine_label),
            Style::default().fg(theme::MUTED),
        )),
        (None, true) => out.push(kv(
            "DETAIL",
            "loading from /api/show…".to_string(),
            Style::default().fg(theme::MUTED),
        )),
        (None, false) => {}
    }

    // --- GPU processes, with explicit confidence ---
    out.push(Line::raw(""));
    let procs = attributed(r, procs);
    if procs.is_empty() {
        out.push(kv(
            "GPU PROCS",
            "no correlation available".to_string(),
            Style::default().fg(theme::MUTED),
        ));
    } else {
        out.push(Line::from(Span::styled(
            " GPU PROCESSES   ✓ confirmed  ~ inferred  ? unknown",
            Style::default().fg(theme::MUTED),
        )));
        for p in procs {
            let colour = match p.confidence {
                MappingConfidence::Confirmed => theme::OK,
                MappingConfidence::Inferred => theme::WARN,
                MappingConfidence::Unknown => theme::MUTED,
            };
            out.push(Line::from(vec![
                Span::styled(format!("  {} ", p.confidence.marker()), Style::default().fg(colour)),
                Span::raw(format!(
                    "{:>7}  {:<16} {}",
                    p.pid,
                    format::truncate(&p.name, 16),
                    p.used_memory
                        .map(format::bytes_iec)
                        .unwrap_or_else(|| format::NA.to_string())
                )),
            ]));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::inference::{Expiry, ModelDetail, ModelStatus, ProcessorSplit};
    use chrono::TimeZone;
    use std::sync::Arc;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    fn text(l: &[Line<'_>]) -> String {
        l.iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn row() -> ModelRow {
        ModelRow {
            key: "ollama/example-moe:q8_0".into(),
            name: "example-moe:q8_0".into(),
            engine_id: "ollama".into(),
            engine_label: "Ollama".into(),
            engine_url: "http://localhost:11434".into(),
            digest: Some("aaaa1111bbbb2222cccc3333dddd4444eeee5555ff".into()),
            family: Some("examplemoe".into()),
            families: None,
            parameter_size: Some("120B".into()),
            quantization: Some("Q8_0".into()),
            format: Some("gguf".into()),
            parent_model: None,
            capabilities: vec!["completion".into(), "tools".into(), "thinking".into()],
            size_total: Some(96_261_027_921),
            size_vram: Some(96_261_027_921),
            size_cpu: Some(0),
            processor: Some(ProcessorSplit::AllGpu),
            context_size: Some(262_144),
            context_max: Some(262_144),
            expiry: Expiry::At(now() + chrono::TimeDelta::minutes(4)),
            status: ModelStatus::Loaded,
            detail: None,
            supports_detail: true,
            stale: false,
        }
    }

    fn gproc(model: Option<&str>, confidence: MappingConfidence) -> GpuProcess {
        GpuProcess {
            pid: 245034,
            name: "llama-server".into(),
            cmdline: None,
            gpu_index: 0,
            used_memory: Some(104_367 * 1024 * 1024),
            graphics: false,
            engine: Some("Ollama".into()),
            model: model.map(str::to_string),
            confidence,
        }
    }

    #[test]
    fn shows_every_field_the_watch_command_does_and_more() {
        let t = text(&lines(&row(), &[], 60, now(), false));
        assert!(t.contains("example-moe:q8_0"), "{t}");
        assert!(t.contains("aaaa1111bbbb"), "{t}");
        assert!(t.contains("120B"), "{t}");
        assert!(t.contains("Q8_0"), "{t}");
        assert!(t.contains("96 GB"), "{t}");
        assert!(t.contains("100% GPU"), "{t}");
        assert!(t.contains("256K"), "{t}");
        assert!(t.contains("4 minutes from now"), "{t}");
        assert!(t.contains("completion, tools, thinking"), "{t}");
    }

    #[test]
    fn a_partially_offloaded_model_shows_the_cpu_share() {
        let mut r = row();
        r.processor = Some(ProcessorSplit::Split { cpu_pct: 37, gpu_pct: 63 });
        r.size_vram = Some(630);
        r.size_cpu = Some(370);
        r.size_total = Some(1000);
        let t = text(&lines(&r, &[], 60, now(), false));
        assert!(t.contains("37%/63% CPU/GPU"), "{t}");
        assert!(t.contains("on CPU"), "{t}");
    }

    #[test]
    fn loaded_context_is_shown_against_the_models_maximum() {
        let mut r = row();
        r.context_size = Some(65536);
        r.context_max = Some(262_144);
        let t = text(&lines(&r, &[], 60, now(), false));
        assert!(t.contains("64K / 256K max"), "{t}");
    }

    /// A row with nothing known must render dashes, never invented values.
    #[test]
    fn missing_data_renders_placeholders_not_zeros() {
        let mut r = row();
        r.digest = None;
        r.family = None;
        r.parameter_size = None;
        r.quantization = None;
        r.format = None;
        r.size_total = None;
        r.size_vram = None;
        r.size_cpu = None;
        r.processor = None;
        r.context_size = None;
        r.context_max = None;
        r.expiry = Expiry::Unknown;
        r.capabilities.clear();

        let t = text(&lines(&r, &[], 60, now(), false));
        assert!(t.contains(format::NA), "{t}");
        assert!(!t.contains("0 B"), "{t}");
        assert!(!t.contains("100% GPU"), "{t}");
        assert!(!t.contains("0%"), "{t}");
    }

    #[test]
    fn show_detail_enriches_the_body_once_loaded() {
        let mut r = row();
        r.detail = Some(Arc::new(ModelDetail {
            architecture: Some("examplemoe".into()),
            parameter_count: Some(120_000_000_000),
            block_count: Some(48),
            embedding_length: Some(3072),
            head_count_kv: Some(8),
            expert_count: Some(256),
            expert_used_count: Some(10),
            ..Default::default()
        }));
        let t = text(&lines(&r, &[], 60, now(), false));
        assert!(t.contains("120000000000"), "{t}");
        assert!(t.contains("48 layers"), "{t}");
        assert!(t.contains("10 of 256 active"), "{t}");
    }

    #[test]
    fn detail_is_marked_as_loading_before_it_arrives() {
        let t = text(&lines(&row(), &[], 60, now(), true));
        assert!(t.contains("loading from /api/show"), "{t}");
    }

    /// Only Ollama has `/api/show`. For anything else the overlay must say so
    /// rather than sit on "loading…" forever.
    #[test]
    fn an_engine_without_a_detail_endpoint_says_so() {
        let mut r = row();
        r.supports_detail = false;
        r.engine_label = "vLLM (local)".into();
        r.detail = None;
        let t = text(&lines(&r, &[], 60, now(), true));
        assert!(t.contains("exposes no per-model metadata"), "{t}");
        assert!(!t.contains("loading"), "{t}");
    }

    /// impl-plan.md §6: never present a guess as a confirmed fact.
    #[test]
    fn process_confidence_is_always_marked() {
        let confirmed = text(&lines(
            &row(),
            &[gproc(Some("example-moe:q8_0"), MappingConfidence::Confirmed)],
            60,
            now(),
            false,
        ));
        assert!(confirmed.contains("✓ "), "{confirmed}");
        assert!(confirmed.contains("245034"), "{confirmed}");
        assert!(confirmed.contains("101.9 GiB"), "{confirmed}");
        assert!(confirmed.contains("confirmed"), "legend missing: {confirmed}");

        let inferred = text(&lines(
            &row(),
            &[gproc(Some("example-moe:q8_0"), MappingConfidence::Inferred)],
            60,
            now(),
            false,
        ));
        assert!(inferred.contains("~ "), "{inferred}");
    }

    #[test]
    fn an_unattributed_process_is_not_claimed_for_the_model() {
        let t = text(&lines(&row(), &[gproc(None, MappingConfidence::Unknown)], 60, now(), false));
        // Shown as a same-engine candidate, but marked unknown.
        assert!(t.contains("? "), "{t}");
    }

    #[test]
    fn no_gpu_processes_says_so() {
        let t = text(&lines(&row(), &[], 60, now(), false));
        assert!(t.contains("no correlation available"), "{t}");
    }
}
