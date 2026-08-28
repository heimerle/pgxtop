use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_gpu_summary(f, chunks[0], app);
    render_system_summary(f, chunks[1], app);
}

fn render_gpu_summary(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("GPU")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    for (i, info) in app.gpu_info.iter().enumerate() {
        let metrics = app.gpu_metrics.get(i);

        lines.push(Line::from(vec![
            Span::raw(format!(" GPU {} ", i)),
            Span::raw(info.name.clone()),
        ]));

        if let Some(m) = metrics {
            let gpu_util = m.utilization_gpu.unwrap_or(0.0);
            let vram_used = m.used_memory;
            let vram_total = info.total_memory;
            let temp = m.temperature.unwrap_or(0.0);
            let power = m.power.unwrap_or(0.0);

            lines.push(Line::from(vec![
                Span::raw(format!(" GPU   {}   ", format_bar(gpu_util, 20))),
                Span::raw(format!("{:.0}%", gpu_util)),
            ]));

            lines.push(Line::from(vec![
                Span::raw(format!(" VRAM  {}   ", format_bar(
                    (vram_used as f32 / vram_total as f32) * 100.0, 20
                ))),
                Span::raw(format!("{}/{} GB", vram_used / 1024 / 1024, vram_total / 1024 / 1024)),
            ]));

            lines.push(Line::from(vec![
                Span::raw(format!(" TEMP  {:.0}C   ", temp)),
                Span::raw(format!("POWER {:.0}W", power)),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_system_summary(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("SYSTEM")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    if let Some(metrics) = &app.system_metrics {
        let cpu = metrics.cpu_usage;
        let mem_used = metrics.used_memory;
        let mem_total = app.system_info.as_ref().map(|s| s.total_memory).unwrap_or(1);

        lines.push(Line::from(vec![
            Span::raw(format!(" CPU   {}   ", format_bar(cpu, 20))),
            Span::raw(format!("{:.0}%", cpu)),
        ]));

        lines.push(Line::from(vec![
            Span::raw(format!(" RAM   {}   ", format_bar(
                (mem_used as f32 / mem_total as f32) * 100.0, 20
            ))),
            Span::raw(format!("{}/{} GB", mem_used / 1024 / 1024, mem_total / 1024 / 1024)),
        ]));

        lines.push(Line::from(vec![
            Span::raw(format!(" Load  {:.2} / {:.2} / {:.2}",
                metrics.load_avg[0], metrics.load_avg[1], metrics.load_avg[2])),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn format_bar(percent: f32, width: usize) -> String {
    let filled = (percent / 100.0 * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}