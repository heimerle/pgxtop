use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::Frame;

use crate::app::App;
use crate::format;
use crate::ui::theme;
use crate::ui::widgets::graph::Graph;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let [cpu, memory, history] = Layout::vertical([
        Constraint::Min(6),
        Constraint::Min(5),
        Constraint::Min(5),
    ])
    .areas(area);

    render_cpu(f, cpu, app);
    render_memory(f, memory, app);
    render_history(f, history, app);
}

fn render_cpu(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" CPU ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let Some(m) = &app.system_metrics else {
        return;
    };
    let bar_w = (inner.width as usize).saturating_sub(20).clamp(6, 30);

    let mut lines = vec![Line::from(vec![
        Span::styled(" ALL  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            format::bar(m.cpu_usage, bar_w),
            Style::default().fg(theme::util_color(m.cpu_usage)),
        ),
        Span::raw(format!(" {:>5.1}%", m.cpu_usage)),
    ])];

    for (i, usage) in m
        .per_core_usage
        .iter()
        .enumerate()
        .take(inner.height.saturating_sub(1) as usize)
    {
        lines.push(Line::from(vec![
            Span::styled(format!(" {i:<4} "), Style::default().fg(theme::MUTED)),
            Span::styled(
                format::bar(*usage, bar_w),
                Style::default().fg(theme::util_color(*usage)),
            ),
            Span::raw(format!(" {usage:>5.1}%")),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_memory(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" MEMORY ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let (Some(info), Some(m)) = (&app.system_info, &app.system_metrics) else {
        return;
    };
    let bar_w = (inner.width as usize).saturating_sub(34).clamp(6, 30);

    let row = |label: &str, used: u64, total: u64| -> Line<'static> {
        let pct = format::pct(used, total);
        Line::from(vec![
            Span::styled(format!(" {label:<5}"), Style::default().fg(theme::MUTED)),
            Span::styled(
                format::bar(pct.unwrap_or(0.0), bar_w),
                Style::default().fg(theme::mem_color(pct.unwrap_or(0.0))),
            ),
            Span::raw(format!(
                " {}/{} {} ({})",
                format::bytes_iec_value(used, total),
                format::bytes_iec_value(total, total),
                format::bytes_iec_unit(total),
                format::pct_str(used, total)
            )),
        ])
    };

    let mut lines = vec![row("RAM", m.used_memory, info.total_memory)];
    if info.total_swap > 0 {
        lines.push(row("SWAP", m.used_swap, info.total_swap));
    } else {
        lines.push(Line::from(vec![
            Span::styled(" SWAP ", Style::default().fg(theme::MUTED)),
            Span::styled("not configured", Style::default().fg(theme::MUTED)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_history(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" CPU HISTORY ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    Graph::new(app.system_history.cpu.as_slice(), 100.0, theme::ACCENT)
        .render(inner, f.buffer_mut());
}

#[allow(dead_code)]
fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}
