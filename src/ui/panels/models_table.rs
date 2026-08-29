//! The loaded-models table — a real `ratatui::widgets::Table` with selection,
//! scrolling and width-aware column degradation.

use chrono::{DateTime, TimeDelta, Utc};
use ratatui::layout::{Constraint, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Cell, HighlightSpacing, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
};
use ratatui::Frame;

use crate::format;
use crate::models::inference::ModelStatus;
use crate::ui::state::{ModelRow, ModelsUiState};
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Col {
    Name,
    Engine,
    Id,
    Size,
    Processor,
    Context,
    Until,
    Status,
}

pub const COL_SPACING: u16 = 1;
/// Width of the `"▸ "` highlight symbol, always reserved so rows do not shift.
pub const HIGHLIGHT_W: u16 = 2;
pub const MIN_NAME_W: u16 = 12;

pub fn header_label(col: Col) -> &'static str {
    match col {
        Col::Name => "NAME",
        Col::Engine => "ENGINE",
        Col::Id => "ID",
        Col::Size => "SIZE",
        Col::Processor => "PROCESSOR",
        Col::Context => "CTX",
        Col::Until => "UNTIL",
        Col::Status => "STATUS",
    }
}

/// Fixed width, or `None` for the one elastic column.
pub fn fixed_width(col: Col) -> Option<u16> {
    match col {
        Col::Name => None,
        Col::Engine => Some(8),
        Col::Id => Some(12),
        Col::Size => Some(9),      // "107.7 GB"
        Col::Processor => Some(15), // "37%/63% CPU/GPU"
        Col::Context => Some(6),
        Col::Until => Some(8),
        Col::Status => Some(9),
    }
}

/// Which columns fit, in display order.
///
/// `PROCESSOR` survives longest after NAME and SIZE on purpose: CPU offload is
/// the one thing on this screen that changes what the user does next.
pub fn pick_columns(inner_width: u16, multi_engine: bool) -> Vec<Col> {
    let candidates: [(Col, Option<u16>); 8] = [
        (Col::Name, Some(0)),
        (Col::Engine, if multi_engine { Some(60) } else { None }),
        (Col::Id, Some(104)),
        (Col::Size, Some(0)),
        (Col::Processor, Some(44)),
        (Col::Context, Some(74)),
        (Col::Until, Some(88)),
        (Col::Status, Some(120)),
    ];
    candidates
        .iter()
        .filter_map(|(c, min)| match min {
            Some(m) if inner_width >= *m => Some(*c),
            _ => None,
        })
        .collect()
}

/// Width left for NAME once every fixed column and the gaps are paid for.
pub fn name_width(inner_width: u16, cols: &[Col]) -> u16 {
    let fixed: u16 = cols.iter().filter_map(|c| fixed_width(*c)).sum();
    let spacing = COL_SPACING * cols.len().saturating_sub(1) as u16;
    inner_width
        .saturating_sub(HIGHLIGHT_W + fixed + spacing)
        .max(MIN_NAME_W)
}

fn right(s: String) -> Cell<'static> {
    Cell::from(Line::from(s).right_aligned())
}

fn cell(col: Col, r: &ModelRow, name_w: u16, now: DateTime<Utc>) -> Cell<'static> {
    match col {
        Col::Name => Cell::from(format::truncate(&r.name, name_w as usize).into_owned()),
        Col::Engine => Cell::from(format::truncate(&r.engine_label, 8).into_owned())
            .style(Style::default().fg(theme::MUTED)),
        Col::Id => Cell::from(
            r.digest
                .as_deref()
                .map(|d| format::digest_short(d).to_string())
                .unwrap_or_else(|| format::NA.to_string()),
        )
        .style(Style::default().fg(theme::MUTED)),
        Col::Size => right(
            r.size_total
                .map(format::bytes_si)
                .unwrap_or_else(|| format::NA.to_string()),
        ),
        Col::Processor => {
            if !r.is_resident() {
                // Nothing is resident, so there is no split to report. Say
                // which kind of not-resident it is instead of implying one.
                return Cell::from(r.status.label()).style(Style::default().fg(theme::MUTED));
            }
            let text = r
                .processor
                .map(|p| p.label())
                .unwrap_or_else(|| format::NA.to_string());
            Cell::from(text).style(theme::processor_style(r.gpu_fraction()))
        }
        Col::Context => right(format::context_opt(r.context_size))
            .style(Style::default().fg(theme::ACCENT)),
        Col::Until => {
            let text = format::until_compact(&r.expiry, now);
            let style = match r.expiry.deadline() {
                // About to be evicted — worth noticing before it happens.
                Some(t) if (t - now) <= TimeDelta::seconds(60) => {
                    Style::default().fg(theme::WARN)
                }
                Some(_) => Style::default(),
                None => Style::default().fg(theme::MUTED),
            };
            right(text).style(style)
        }
        Col::Status => Cell::from(r.status.label()).style(if r.stale {
            Style::default().fg(theme::WARN)
        } else {
            Style::default().fg(theme::MUTED)
        }),
    }
}

pub fn title(rows: &[ModelRow]) -> String {
    let loaded = rows.iter().filter(|r| r.is_resident()).count();
    let served = rows
        .iter()
        .filter(|r| r.status == ModelStatus::Served)
        .count();
    let installed = rows
        .iter()
        .filter(|r| r.status == ModelStatus::Installed)
        .count();

    // Counting router routes as "loaded" would inflate the headline number
    // from 1 to 30 on the target host.
    let mut parts = vec![format!("{loaded} loaded")];
    if served > 0 {
        parts.push(format!("{served} served"));
    }
    if installed > 0 {
        parts.push(format!("{installed} installed"));
    }
    format!(" MODELS ({}) ", parts.join(" · "))
}

pub fn render(
    f: &mut Frame,
    area: Rect,
    rows: &[ModelRow],
    ui: &mut ModelsUiState,
    focused: bool,
    now: DateTime<Utc>,
) {
    let block = theme::panel_block(title(rows), focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if rows.is_empty() {
        render_empty_state(f, inner, ui);
        return;
    }

    // minus the header row
    ui.visible_rows = inner.height.saturating_sub(1) as usize;

    let cols = pick_columns(inner.width, ui.multi_engine);
    let name_w = name_width(inner.width, &cols);

    let header = Row::new(
        cols.iter()
            .map(|c| Cell::from(header_label(*c)))
            .collect::<Vec<_>>(),
    )
    .style(
        Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD),
    );

    let body: Vec<Row> = rows
        .iter()
        .map(|r| {
            let mut base = Style::default().fg(theme::TEXT);
            if r.stale || !r.is_resident() {
                base = base.add_modifier(Modifier::DIM);
            }
            Row::new(
                cols.iter()
                    .map(|c| cell(*c, r, name_w, now))
                    .collect::<Vec<_>>(),
            )
            .style(base)
        })
        .collect();

    let widths: Vec<Constraint> = cols
        .iter()
        .map(|c| Constraint::Length(fixed_width(*c).unwrap_or(name_w)))
        .collect();

    let table = Table::new(body, widths)
        .header(header)
        .column_spacing(COL_SPACING)
        .highlight_symbol("▸ ")
        .highlight_spacing(HighlightSpacing::Always)
        .row_highlight_style(
            Style::default()
                .bg(theme::SEL_BG)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(table, inner, &mut ui.table);

    if rows.len() > ui.visible_rows && area.height > 2 {
        let mut sb = ScrollbarState::new(rows.len()).position(ui.table.offset());
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area.inner(Margin { horizontal: 0, vertical: 1 }),
            &mut sb,
        );
    }
}

/// Distinguishes the two causes so the user knows whether to start an engine
/// or load a model. The old view drew an empty box either way.
fn render_empty_state(f: &mut Frame, inner: Rect, ui: &ModelsUiState) {
    let lines = if ui.any_engine_connected {
        vec![
            Line::from("No models loaded").centered(),
            Line::from(Span::styled(
                "run  ollama run <model>  to load one",
                Style::default().fg(theme::MUTED),
            ))
            .centered(),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                "No inference engine reachable",
                Style::default().fg(theme::CRIT),
            ))
            .centered(),
            Line::from(Span::styled(
                "see the ENGINES row above for the endpoints checked",
                Style::default().fg(theme::MUTED),
            ))
            .centered(),
        ]
    };

    let top = inner.height.saturating_sub(lines.len() as u16) / 2;
    let area = Rect {
        x: inner.x,
        y: inner.y + top,
        width: inner.width,
        height: inner.height.saturating_sub(top),
    };
    if area.height > 0 {
        f.render_widget(Paragraph::new(lines), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_sets_are_monotonic_in_width() {
        let mut prev: Vec<Col> = Vec::new();
        for w in 0..200u16 {
            let cols = pick_columns(w, true);
            for c in &prev {
                assert!(
                    cols.contains(c),
                    "column {c:?} disappeared going from {} to {w}",
                    w.saturating_sub(1)
                );
            }
            prev = cols;
        }
    }

    #[test]
    fn narrow_terminals_keep_name_size_and_processor() {
        let cols = pick_columns(50, true);
        assert_eq!(cols, vec![Col::Name, Col::Size, Col::Processor]);

        let very_narrow = pick_columns(30, true);
        assert_eq!(very_narrow, vec![Col::Name, Col::Size]);
    }

    #[test]
    fn wide_terminals_show_every_ollama_ps_column() {
        let cols = pick_columns(130, true);
        for c in [
            Col::Name,
            Col::Engine,
            Col::Id,
            Col::Size,
            Col::Processor,
            Col::Context,
            Col::Until,
            Col::Status,
        ] {
            assert!(cols.contains(&c), "missing {c:?}");
        }
    }

    #[test]
    fn engine_column_only_appears_with_more_than_one_engine() {
        assert!(!pick_columns(200, false).contains(&Col::Engine));
        assert!(pick_columns(200, true).contains(&Col::Engine));
    }

    /// The layout must always fit: nothing may be pushed off the right edge.
    #[test]
    fn columns_always_fit_the_available_width() {
        for w in 40..250u16 {
            for multi in [false, true] {
                let cols = pick_columns(w, multi);
                let name = name_width(w, &cols);
                let total: u16 = HIGHLIGHT_W
                    + cols
                        .iter()
                        .map(|c| fixed_width(*c).unwrap_or(name))
                        .sum::<u16>()
                    + COL_SPACING * cols.len().saturating_sub(1) as u16;
                assert!(total <= w, "w={w} multi={multi} total={total} cols={cols:?}");
            }
        }
    }

    #[test]
    fn name_never_shrinks_below_the_minimum() {
        for w in 0..250u16 {
            let cols = pick_columns(w, true);
            assert!(name_width(w, &cols) >= MIN_NAME_W);
        }
    }

    // -----------------------------------------------------------------
    // rendering, via TestBackend — no real terminal involved
    // -----------------------------------------------------------------

    use crate::models::inference::{Expiry, ProcessorSplit};
    use crate::ui::state::ModelRow;
    use chrono::TimeZone;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    fn empty_row(name: &str) -> ModelRow {
        ModelRow {
            key: format!("ollama/{name}"),
            name: name.into(),
            engine_id: "ollama".into(),
            engine_label: "Ollama".into(),
            engine_url: "http://localhost:11434".into(),
            digest: None,
            family: None,
            families: None,
            parameter_size: None,
            quantization: None,
            format: None,
            parent_model: None,
            capabilities: Vec::new(),
            size_total: None,
            size_vram: None,
            size_cpu: None,
            processor: None,
            context_size: None,
            context_max: None,
            expiry: Expiry::Unknown,
            status: ModelStatus::Loaded,
            detail: None,
            supports_detail: true,
            stale: false,
        }
    }

    /// The row measured on the PGX.
    fn examplemoe() -> ModelRow {
        ModelRow {
            digest: Some("aaaa1111bbbb2222cccc3333dddd4444eeee5555ff".into()),
            size_total: Some(96_261_027_921),
            size_vram: Some(96_261_027_921),
            size_cpu: Some(0),
            processor: Some(ProcessorSplit::AllGpu),
            context_size: Some(262_144),
            expiry: Expiry::At(now() + chrono::TimeDelta::minutes(4)),
            ..empty_row("example-moe:q8_0")
        }
    }

    fn offloaded() -> ModelRow {
        ModelRow {
            size_total: Some(1000),
            size_vram: Some(630),
            size_cpu: Some(370),
            processor: Some(ProcessorSplit::Split { cpu_pct: 37, gpu_pct: 63 }),
            context_size: Some(131_072),
            expiry: Expiry::At(now() + chrono::TimeDelta::minutes(30)),
            ..empty_row("glm-5:q4_K_M")
        }
    }

    fn buffer_text(buf: &Buffer) -> String {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn draw(rows: &[ModelRow], ui: &mut ModelsUiState, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| render(f, f.area(), rows, ui, true, now())).unwrap();
        buffer_text(t.backend().buffer())
    }

    #[test]
    fn a_wide_terminal_shows_every_ollama_ps_field() {
        let rows = vec![examplemoe(), offloaded()];
        let mut ui = ModelsUiState { multi_engine: true, ..Default::default() };
        ui.reconcile(&rows);

        let s = draw(&rows, &mut ui, 130, 10);
        for header in ["NAME", "ENGINE", "ID", "SIZE", "PROCESSOR", "CTX", "UNTIL"] {
            assert!(s.contains(header), "missing header {header}\n{s}");
        }
        assert!(s.contains("example-moe:q8_0"), "{s}");
        assert!(s.contains("aaaa1111bbbb"), "{s}");
        assert!(s.contains("96 GB"), "{s}");
        assert!(s.contains("100% GPU"), "{s}");
        assert!(s.contains("37%/63% CPU/GPU"), "{s}");
        assert!(s.contains("256K"), "{s}");
        assert!(s.contains("4m"), "{s}");
        assert!(s.contains("▸ example-moe"), "selection marker missing\n{s}");
        assert!(s.contains("MODELS (2 loaded)"), "{s}");
    }

    #[test]
    fn a_narrow_terminal_ellipsises_and_drops_columns() {
        let rows = vec![ModelRow {
            name: "acme/example-27b:q8_0".into(),
            ..examplemoe()
        }];
        let mut ui = ModelsUiState::default();
        ui.reconcile(&rows);

        let s = draw(&rows, &mut ui, 50, 6);
        assert!(s.contains('…'), "long name must be ellipsised\n{s}");
        assert!(!s.contains("UNTIL"), "{s}");
        assert!(!s.contains("CTX"), "{s}");
        assert!(s.contains("SIZE"), "{s}");
        assert!(s.contains("100% GPU"), "placement must survive\n{s}");
        // Nothing may spill past the right edge.
        for line in s.lines() {
            assert!(line.chars().count() <= 50, "line too wide: {line:?}");
        }
    }

    /// The project spec forbids inventing values; a row with nothing known
    /// must show dashes, not zeros.
    #[test]
    fn missing_data_renders_placeholders_never_zeros() {
        let rows = vec![empty_row("mystery-model")];
        let mut ui = ModelsUiState::default();
        ui.reconcile(&rows);

        let s = draw(&rows, &mut ui, 130, 6);
        assert!(s.contains(crate::format::NA), "{s}");
        assert!(!s.contains("0 B"), "{s}");
        assert!(!s.contains("0.0"), "{s}");
        assert!(!s.contains("100% GPU"), "{s}");
        assert!(!s.contains("100% CPU"), "{s}");
    }

    #[test]
    fn installed_only_rows_say_installed_rather_than_implying_a_split() {
        let mut r = empty_row("qwen3-coder:30b");
        r.status = ModelStatus::Installed;
        r.size_total = Some(18_000_000_000);
        let rows = vec![r];
        let mut ui = ModelsUiState::default();
        ui.reconcile(&rows);

        let s = draw(&rows, &mut ui, 130, 6);
        assert!(s.contains("installed"), "{s}");
        assert!(s.contains("18 GB"), "{s}");
        assert!(!s.contains("100% GPU"), "{s}");
        assert!(s.contains("1 loaded") || s.contains("0 loaded"), "{s}");
    }

    /// On the target host `:8888` is a vllm-semantic-router: its 29
    /// `/v1/models` entries are virtual routes, and counting them as loaded
    /// turned "1 loaded" into "30 loaded".
    #[test]
    fn router_routes_are_counted_and_labelled_as_served() {
        let mut served = empty_row("vllm-sr/auto");
        served.status = ModelStatus::Served;
        served.engine_label = "vLLM (local)".into();
        let mut installed = empty_row("gemma3:27b");
        installed.status = ModelStatus::Installed;
        installed.size_total = Some(17_000_000_000);

        let rows = vec![examplemoe(), served, installed];
        let mut ui = ModelsUiState { multi_engine: true, ..Default::default() };
        ui.reconcile(&rows);

        let s = draw(&rows, &mut ui, 130, 8);
        assert!(
            s.contains("MODELS (1 loaded · 1 served · 1 installed)"),
            "headline count must not fold routes into \"loaded\"\n{s}"
        );
        assert!(s.contains("served"), "{s}");
        assert!(s.contains("installed"), "{s}");
        // A route has nothing resident, so its row must not claim a placement.
        let route_line = s
            .lines()
            .find(|l| l.contains("vllm-sr/auto"))
            .expect("route row");
        assert!(!route_line.contains("GPU"), "{route_line}");
        assert!(!route_line.contains("CPU"), "{route_line}");
    }

    #[test]
    fn the_empty_state_distinguishes_its_two_causes() {
        for (connected, needle) in [
            (true, "No models loaded"),
            (false, "No inference engine reachable"),
        ] {
            let mut ui = ModelsUiState {
                any_engine_connected: connected,
                ..Default::default()
            };
            let s = draw(&[], &mut ui, 80, 8);
            assert!(s.contains(needle), "expected {needle:?} in\n{s}");
        }
    }

    /// Regression guard for the whole `size.height - 3` underflow class.
    #[test]
    fn never_panics_on_tiny_or_degenerate_areas() {
        let rows = vec![examplemoe(), offloaded()];
        for w in 1..=60u16 {
            for h in 1..=8u16 {
                let mut ui = ModelsUiState { multi_engine: true, ..Default::default() };
                ui.reconcile(&rows);
                let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
                t.draw(|f| render(f, f.area(), &rows, &mut ui, true, now()))
                    .unwrap();
            }
        }
    }

    #[test]
    fn visible_rows_is_reported_so_paging_works() {
        let rows: Vec<ModelRow> = (0..30).map(|i| empty_row(&format!("m{i}"))).collect();
        let mut ui = ModelsUiState::default();
        ui.reconcile(&rows);
        draw(&rows, &mut ui, 100, 12);
        // 12 rows - 2 borders - 1 header
        assert_eq!(ui.visible_rows, 9);
        assert_eq!(ui.page(), 9);
    }
}
