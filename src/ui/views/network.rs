use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("NETWORK")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    if let Some(metrics) = &app.system_metrics {
        lines.push(Line::raw(format!(" RX: {} bytes", metrics.network_io.rx_bytes)));
        lines.push(Line::raw(format!(" TX: {} bytes", metrics.network_io.tx_bytes)));
        lines.push(Line::raw(format!(" RX Packets: {}", metrics.network_io.rx_packets)));
        lines.push(Line::raw(format!(" TX Packets: {}", metrics.network_io.tx_packets)));
    }

    f.render_widget(Paragraph::new(lines), inner);
}