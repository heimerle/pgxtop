use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    widths: Vec<u16>,
}

impl Table {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>, widths: Vec<u16>) -> Self {
        Self { headers, rows, widths }
    }
}

impl Widget for Table {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut x = area.left();
        let mut y = area.top();

        // Render headers
        for (i, header) in self.headers.iter().enumerate() {
            let width = self.widths.get(i).copied().unwrap_or(10).min(area.width);
            buf.set_stringn(x, y, header, width as usize, Style::default().add_modifier(ratatui::style::Modifier::BOLD));
            x += width;
        }

        y += 1;

        // Render rows
        for row in &self.rows {
            let mut x = area.left();
            for (i, cell) in row.iter().enumerate() {
                let width = self.widths.get(i).copied().unwrap_or(10).min(area.width);
                buf.set_stringn(x, y, cell, width as usize, Style::default());
                x += width;
            }
            y += 1;
        }
    }
}