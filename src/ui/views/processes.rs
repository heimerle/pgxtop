use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("PROCESSES")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::raw(format!(" {:>6} {:<20} {:>8} {:>10} ", "PID", "NAME", "CPU%", "MEM(MB)")),
    ]));

    // Sort by CPU usage
    let mut procs = app.system_processes.clone();
    procs.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal));

    for proc in procs.iter().take(50) {
        lines.push(Line::from(vec![
            Span::raw(format!(" {:>6} {:<20} {:>8.1} {:>10} ",
                proc.pid,
                proc.name,
                proc.cpu_usage,
                proc.memory_usage / 1024
            )),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}