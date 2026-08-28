use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Min(10),
        ])
        .split(area);

    render_gpu_details(f, chunks[0], app);
    render_gpu_processes(f, chunks[1], app);
}

fn render_gpu_details(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("GPU DETAILS")
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
            lines.push(Line::raw(format!("  Utilization:  {:.0}%", m.utilization_gpu.unwrap_or(0.0))));
            lines.push(Line::raw(format!("  VRAM:         {}/{} GB ({:.0}%)",
                m.used_memory / 1024 / 1024,
                info.total_memory / 1024 / 1024,
                (m.used_memory as f32 / info.total_memory as f32) * 100.0
            )));
            lines.push(Line::raw(format!("  Temperature:  {:.0}C", m.temperature.unwrap_or(0.0))));
            lines.push(Line::raw(format!("  Power:        {:.0}W / {:.0}W",
                m.power.unwrap_or(0.0),
                m.power_limit.unwrap_or(0.0)
            )));
            lines.push(Line::raw(format!("  SM Clock:     {} MHz", m.sm_clock.unwrap_or(0))));
            lines.push(Line::raw(format!("  MEM Clock:    {} MHz", m.mem_clock.unwrap_or(0))));
            lines.push(Line::raw(format!("  Fan Speed:    {}%", m.fan_speed.unwrap_or(0))));
        }

        lines.push(Line::raw(""));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_gpu_processes(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("GPU PROCESSES")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    for proc in &app.gpu_processes {
        lines.push(Line::from(vec![
            Span::raw(format!(" {:>6} ", proc.pid)),
            Span::raw(format!("{:<20} ", proc.name)),
            Span::raw(format!("{} GB", proc.used_memory / 1024 / 1024)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}