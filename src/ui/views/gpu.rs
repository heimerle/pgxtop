use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::Frame;

use crate::app::App;
use crate::format;
use crate::models::{GpuInfo, GpuMetrics};
use crate::ui::theme;
use crate::ui::widgets::graph::Graph;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let [details, procs, history] = Layout::vertical([
        Constraint::Min(8),
        Constraint::Min(6),
        Constraint::Min(6),
    ])
    .areas(area);

    render_details(f, details, app);
    render_processes(f, procs, app);
    render_history(f, history, app);
}

/// Renders a metric, or `N/A` when NVML does not support it on this device.
/// Never a fabricated 0 — on a GB10 that would be most of this panel.
fn metric(label: &str, value: Option<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("   {label:<11}"),
            Style::default().fg(theme::MUTED),
        ),
        match value {
            Some(v) => Span::raw(v),
            None => Span::styled("N/A", Style::default().fg(theme::MUTED)),
        },
    ])
}

fn detail_lines(info: &GpuInfo, m: &GpuMetrics) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        format!(" GPU{} {}", info.index, info.name),
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))];

    lines.push(metric(
        "Utilization",
        m.utilization_gpu.map(|v| format!("{v:.0}%")),
    ));
    lines.push(metric(
        m.memory.label(),
        match (m.memory.used(), m.memory.total()) {
            (Some(u), Some(t)) => Some(format!(
                "{}/{} {} ({})",
                format::bytes_iec_value(u, t),
                format::bytes_iec_value(t, t),
                format::bytes_iec_unit(t),
                format::pct_str(u, t)
            )),
            _ => None,
        },
    ));
    if m.memory.is_unified() {
        lines.push(metric("GPU-resident", m.memory.gpu_resident().map(format::bytes_iec)));
    }
    lines.push(metric("Temperature", m.temperature.map(format::celsius)));
    lines.push(metric(
        "Power",
        m.power_watts.map(|p| match m.power_limit_watts {
            Some(l) => format!("{} / {}", format::watts(p), format::watts(l)),
            None => format::watts(p),
        }),
    ));
    lines.push(metric("SM clock", m.sm_clock.map(|c| format!("{c} MHz"))));
    lines.push(metric("MEM clock", m.mem_clock.map(|c| format!("{c} MHz"))));
    lines.push(metric("Fan", m.fan_speed.map(|s| format!("{s}%"))));
    lines
}

fn render_details(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" GPU DETAILS ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (info, m) in app.gpu_info.iter().zip(app.gpu_metrics.iter()) {
        lines.extend(detail_lines(info, m));
        lines.push(Line::raw(""));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                " NVML unavailable{}",
                app.collectors
                    .nvml
                    .init_error()
                    .map(|e| format!(" — {e}"))
                    .unwrap_or_default()
            ),
            Style::default().fg(theme::MUTED),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_processes(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" GPU PROCESSES ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        format!(
            " {:>7} {:<18} {:<9} {:<22} {}",
            "PID", "PROCESS", "ENGINE", "MODEL", "MEMORY"
        ),
        Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD),
    ))];

    for p in app.gpu_processes.iter().take(inner.height as usize - 1) {
        let confidence_colour = match p.confidence {
            crate::models::MappingConfidence::Confirmed => theme::OK,
            crate::models::MappingConfidence::Inferred => theme::WARN,
            crate::models::MappingConfidence::Unknown => theme::MUTED,
        };
        lines.push(Line::from(vec![
            Span::raw(format!(
                " {:>7} {:<18} {:<9} ",
                p.pid,
                format::truncate(&p.name, 18),
                p.engine.as_deref().unwrap_or(format::NA)
            )),
            Span::styled(
                format!("{} ", p.confidence.marker()),
                Style::default().fg(confidence_colour),
            ),
            Span::raw(format!(
                "{:<20} {}",
                format::truncate(p.model.as_deref().unwrap_or(format::NA), 20),
                p.used_memory
                    .map(format::bytes_iec)
                    .unwrap_or_else(|| format::NA.to_string())
            )),
        ]));
    }

    if app.gpu_processes.is_empty() {
        lines.push(Line::from(Span::styled(
            " no GPU processes",
            Style::default().fg(theme::MUTED),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_history(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" GPU HISTORY  util / mem / temp ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 || inner.width == 0 {
        return;
    }
    let Some(h) = app.gpu_history.first() else {
        return;
    };

    let third = inner.height / 3;
    let [util, mem, temp] = Layout::vertical([
        Constraint::Length(third),
        Constraint::Length(third),
        Constraint::Length(inner.height - 2 * third),
    ])
    .areas(inner);

    Graph::new(h.utilization.as_slice(), 100.0, theme::OK).render(util, f.buffer_mut());
    Graph::new(h.memory.as_slice(), 100.0, theme::WARN).render(mem, f.buffer_mut());
    // Degrees, not percent — the old code scaled temperature against 100.0
    // as if it were a percentage.
    Graph::new(h.temperature.as_slice(), 105.0, theme::CRIT).render(temp, f.buffer_mut());
}
