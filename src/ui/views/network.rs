use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::format;
use crate::ui::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" NETWORK ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    match &app.system_metrics {
        Some(m) if m.network_io.rx_bytes > 0 || m.network_io.tx_bytes > 0 => {
            lines.push(Line::from(vec![
                Span::styled(" RX  ", Style::default().fg(theme::MUTED)),
                Span::raw(format::bytes_iec(m.network_io.rx_bytes)),
                Span::styled("   packets ", Style::default().fg(theme::MUTED)),
                Span::raw(m.network_io.rx_packets.to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled(" TX  ", Style::default().fg(theme::MUTED)),
                Span::raw(format::bytes_iec(m.network_io.tx_bytes)),
                Span::styled("   packets ", Style::default().fg(theme::MUTED)),
                Span::raw(m.network_io.tx_packets.to_string()),
            ]));
        }
        _ => {
            // Honest about the gap rather than showing four permanent zeros:
            // the system collector never populates network counters.
            lines.push(Line::from(Span::styled(
                " network counters are not collected yet",
                Style::default().fg(theme::MUTED),
            )));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}
