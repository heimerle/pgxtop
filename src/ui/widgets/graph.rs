use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

pub struct Graph {
    data: Vec<f32>,
    max_value: f32,
    color: Color,
}

impl Graph {
    pub fn new(data: Vec<f32>, max_value: f32, color: Color) -> Self {
        Self { data, max_value, color }
    }
}

impl Widget for Graph {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.data.is_empty() {
            return;
        }

        let width = area.width as usize;
        let height = area.height as usize;

        let step = self.data.len().saturating_sub(1).max(1) / width.max(1);
        let mut idx = 0;

        for x in 0..width {
            let value = self.data.get(idx * step).copied().unwrap_or(0.0);
            let bar_height = (value / self.max_value * height as f32).round() as usize;

            for y in 0..height {
                let symbol = if y >= height - bar_height {
                    "█"
                } else {
                    " "
                };

                buf.set_string(
                    area.left() + x as u16,
                    area.top() + (height - 1 - y) as u16,
                    symbol,
                    Style::default().fg(self.color),
                );
            }

            idx = idx.saturating_add(1);
        }
    }
}