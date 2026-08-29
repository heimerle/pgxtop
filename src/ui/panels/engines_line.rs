//! One-line engine status strip, with the active filter and sort.
//!
//! Replaces the old three-row ENGINES panel: on a workstation with one or two
//! engines that panel was 90 % empty box.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::collectors::inference::InferenceSnapshot;
use crate::format;
use crate::ui::state::ModelsUiState;
use crate::ui::theme;

fn host_port(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let hostport = rest.split('/').next().unwrap_or(rest);
    match hostport.rsplit_once(':') {
        // Local endpoints are the common case; ":11434" is enough to identify.
        Some((host, port)) if host == "localhost" || host == "127.0.0.1" => format!(":{port}"),
        _ => hostport.to_string(),
    }
}

pub fn line(snapshot: &InferenceSnapshot, ui: &ModelsUiState) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    if snapshot.engines.is_empty() {
        spans.push(Span::styled(
            " no inference engines configured",
            Style::default().fg(theme::MUTED),
        ));
        return Line::from(spans);
    }

    for e in &snapshot.engines {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            e.engine.status.glyph(),
            Style::default().fg(theme::engine_status_color(e.engine.status)),
        ));
        spans.push(Span::raw(format!(" {} ", e.engine.name)));
        spans.push(Span::styled(
            host_port(&e.engine.url),
            Style::default().fg(theme::MUTED),
        ));

        if let Some(err) = &e.last_error {
            spans.push(Span::styled(
                format!(" ({})", err.short()),
                Style::default().fg(theme::CRIT),
            ));
        } else if e.is_stale(snapshot.stale_after) {
            if let Some(age) = e.age() {
                spans.push(Span::styled(
                    format!(" (stale {}s)", age.as_secs()),
                    Style::default().fg(theme::WARN),
                ));
            }
        }
        spans.push(Span::raw("  "));
    }

    spans.push(Span::styled(
        format!("   sort {}", ui.sort.label()),
        Style::default().fg(theme::MUTED),
    ));
    spans.push(Span::styled(
        if ui.sort_desc { " ▼" } else { " ▲" },
        Style::default().fg(theme::MUTED),
    ));

    if ui.multi_engine {
        let filter = ui.filter_engine.as_deref().unwrap_or("ALL");
        spans.push(Span::styled(
            format!("   filter {}", format::truncate(filter, 12)),
            Style::default().fg(theme::MUTED),
        ));
    }

    Line::from(spans)
}

pub fn render(f: &mut Frame, area: Rect, snapshot: &InferenceSnapshot, ui: &ModelsUiState) {
    if area.height == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(line(snapshot, ui)).style(Style::default().add_modifier(Modifier::empty())),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::inference::EngineSnapshot;
    use crate::engines::{EngineStatus, EngineType, InferenceEngine, ProbeError};
    use std::time::{Duration, Instant};

    fn snap(engines: Vec<EngineSnapshot>) -> InferenceSnapshot {
        InferenceSnapshot {
            engines,
            generation: 1,
            stale_after: Duration::from_millis(10_000),
        }
    }

    fn engine(name: &str, url: &str, status: EngineStatus) -> EngineSnapshot {
        EngineSnapshot {
            engine: InferenceEngine {
                id: name.to_lowercase(),
                name: name.into(),
                engine_type: EngineType::Ollama,
                url: url.into(),
                status,
            },
            models: Vec::new(),
            metrics: None,
            last_ok: None,
            consecutive_failures: 0,
            last_error: None,
        }
    }

    fn text(l: &Line<'_>) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn local_urls_collapse_to_a_port() {
        assert_eq!(host_port("http://localhost:11434"), ":11434");
        assert_eq!(host_port("http://127.0.0.1:8888"), ":8888");
        assert_eq!(host_port("http://10.0.0.5:11434"), "10.0.0.5:11434");
    }

    #[test]
    fn status_glyphs_are_coloured() {
        let s = snap(vec![
            engine("Ollama", "http://localhost:11434", EngineStatus::Connected),
            engine("vLLM", "http://localhost:8888", EngineStatus::Unavailable),
        ]);
        let l = line(&s, &ModelsUiState::default());
        let colours: Vec<_> = l
            .spans
            .iter()
            .filter(|sp| sp.content == "●" || sp.content == "○")
            .map(|sp| sp.style.fg)
            .collect();
        assert_eq!(colours, vec![Some(theme::OK), Some(theme::CRIT)]);
        let t = text(&l);
        assert!(t.contains(":11434"), "{t}");
        assert!(t.contains(":8888"), "{t}");
    }

    /// A failure must say *why* — that is the whole point of the typed error.
    #[test]
    fn the_failure_reason_is_shown() {
        let mut e = engine("Ollama", "http://localhost:11434", EngineStatus::Unavailable);
        e.last_error = Some(ProbeError::Status(404));
        let t = text(&line(&snap(vec![e]), &ModelsUiState::default()));
        assert!(t.contains("HTTP 404"), "{t}");
    }

    #[test]
    fn stale_engines_report_their_age() {
        let mut e = engine("Ollama", "http://localhost:11434", EngineStatus::Connected);
        e.last_ok = Some(Instant::now() - Duration::from_secs(42));
        let t = text(&line(&snap(vec![e]), &ModelsUiState::default()));
        assert!(t.contains("stale 42s"), "{t}");
    }

    #[test]
    fn filter_is_only_advertised_when_there_is_more_than_one_engine() {
        let s = snap(vec![engine("Ollama", "http://localhost:11434", EngineStatus::Connected)]);
        let single = ModelsUiState::default();
        assert!(!text(&line(&s, &single)).contains("filter"));

        let multi = ModelsUiState { multi_engine: true, ..Default::default() };
        assert!(text(&line(&s, &multi)).contains("filter ALL"));
    }

    #[test]
    fn no_engines_says_so() {
        let t = text(&line(&snap(vec![]), &ModelsUiState::default()));
        assert!(t.contains("no inference engines configured"), "{t}");
    }
}
