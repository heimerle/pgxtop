use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::widgets::graph::Graph;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Min(10),
            Constraint::Min(8),
        ])
        .split(area);

    render_cpu(f, chunks[0], app);
    render_memory(f, chunks[1], app);
    render_cpu_history(f, chunks[2], app);
}

fn render_cpu(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("CPU")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    if let Some(metrics) = &app.system_metrics {
        lines.push(Line::raw(format!(" Overall: {:.1}%", metrics.cpu_usage)));

        for (i, usage) in metrics.per_core_usage.iter().enumerate() {
            lines.push(Line::raw(format!(" Core {:>2}: {:.1}% ", i, usage)));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_memory(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("MEMORY")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    if let Some(metrics) = &app.system_metrics {
        let mem_total = app.system_info.as_ref().map(|s| s.total_memory).unwrap_or(1);
        let swap_total = app.system_info.as_ref().map(|s| s.total_swap).unwrap_or(1);

        lines.push(Line::raw(format!(" RAM:   {}/{} GB ({:.1}%)",
            metrics.used_memory / 1024 / 1024,
            mem_total / 1024 / 1024,
            (metrics.used_memory as f32 / mem_total as f32) * 100.0
        )));

        lines.push(Line::raw(format!(" Swap:  {}/{} GB ({:.1}%)",
            metrics.used_swap / 1024 / 1024,
            swap_total / 1024 / 1024,
            (metrics.used_swap as f32 / swap_total as f32) * 100.0
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_cpu_history(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("CPU HISTORY")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    f.render_widget(
        Graph::new(app.system_history.cpu.clone(), 100.0, Color::Cyan),
        inner,
    );
}