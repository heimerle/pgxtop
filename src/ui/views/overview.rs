use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::Frame;

use crate::app::App;
use crate::format;
use crate::ui::panels::{gpu_strip, models_table};
use crate::ui::theme;
use crate::ui::widgets::graph::Graph;

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let summaries = app.gpu_summaries();
    let gpu_h = gpu_strip::height(&summaries, area.height);

    let [gpu_area, rest] = Layout::vertical([
        Constraint::Length(gpu_h.min(area.height)),
        Constraint::Min(3),
    ])
    .areas(area);

    gpu_strip::render(
        f,
        gpu_area,
        &app.gpu_info,
        &summaries,
        app.collectors.nvml.init_error(),
        area.height >= 24,
    );

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(rest);

    // The same table as view [3], read-only: no selection, no scrollbar.
    render_models_summary(f, left, app);
    render_system(f, right, app);
}

fn render_models_summary(f: &mut Frame, area: Rect, app: &mut App) {
    let rows: Vec<_> = app
        .model_rows
        .iter()
        .filter(|r| r.is_resident())
        .cloned()
        .collect();
    let mut ui = crate::ui::state::ModelsUiState {
        multi_engine: app.models_ui.multi_engine,
        any_engine_connected: app.models_ui.any_engine_connected,
        ..Default::default()
    };
    models_table::render(f, area, &rows, &mut ui, false, chrono::Utc::now());
}

fn render_system(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" SYSTEM ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    if let (Some(info), Some(m)) = (&app.system_info, &app.system_metrics) {
        let bar_w = (inner.width as usize).saturating_sub(28).clamp(6, 20);

        lines.push(Line::from(vec![
            Span::styled(" CPU  ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format::bar(m.cpu_usage, bar_w),
                Style::default().fg(theme::util_color(m.cpu_usage)),
            ),
            Span::raw(format!(" {:>5.1}%", m.cpu_usage)),
        ]));

        let mem_pct = format::pct(m.used_memory, info.total_memory);
        lines.push(Line::from(vec![
            Span::styled(" RAM  ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format::bar(mem_pct.unwrap_or(0.0), bar_w),
                Style::default().fg(theme::mem_color(mem_pct.unwrap_or(0.0))),
            ),
            Span::raw(format!(
                " {}/{} {}",
                format::bytes_iec_value(m.used_memory, info.total_memory),
                format::bytes_iec_value(info.total_memory, info.total_memory),
                format::bytes_iec_unit(info.total_memory),
            )),
        ]));

        if info.total_swap > 0 {
            let swap_pct = format::pct(m.used_swap, info.total_swap).unwrap_or(0.0);
            lines.push(Line::from(vec![
                Span::styled(" SWAP ", Style::default().fg(theme::MUTED)),
                Span::styled(
                    format::bar(swap_pct, bar_w),
                    Style::default().fg(theme::mem_color(swap_pct)),
                ),
                Span::raw(format!(" {}", format::bytes_iec(m.used_swap))),
            ]));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(" LOAD ", Style::default().fg(theme::MUTED)),
            Span::raw(format!(
                "{:.2} / {:.2} / {:.2}   {} cores",
                m.load_avg[0], m.load_avg[1], m.load_avg[2], info.cpu_count
            )),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            " collecting…",
            Style::default().fg(theme::MUTED),
        )));
    }

    let text_h = lines.len() as u16;
    f.render_widget(Paragraph::new(lines), inner);

    // The graph is drawn *below* the text, not underneath it — previously the
    // Paragraph was rendered after the Graph into the same rect and erased it.
    if inner.height > text_h + 1 {
        let graph_area = Rect {
            x: inner.x,
            y: inner.y + text_h + 1,
            width: inner.width,
            height: inner.height - text_h - 1,
        };
        Graph::new(app.system_history.cpu.as_slice(), 100.0, theme::ACCENT)
            .render(graph_area, f.buffer_mut());
    }
}
