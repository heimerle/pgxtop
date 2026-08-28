use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

const BRAILLE_CHARS: [&str; 8] = [" ", "⢀", "⢠", "⢰", "⢸", "⢼", "⣠", "⣾"];

pub struct Sparkline {
    data: Vec<f32>,
    max_value: f32,
    color: Color,
}

impl Sparkline {
    pub fn new(data: Vec<f32>, max_value: f32, color: Color) -> Self {
        Self { data, max_value, color }
    }
}

impl Widget for Sparkline {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.data.is_empty() {
            return;
        }

        let width = area.width as usize;
        let height = area.height as usize;

        let step = self.data.len().saturating_sub(1).max(1) / width.max(1);

        for x in 0..width {
            let value = self.data.get(x * step).copied().unwrap_or(0.0);
            let normalized = (value / self.max_value).clamp(0.0, 1.0);
            let braille_idx = (normalized * 7.0).round() as usize;
            let char = BRAILLE_CHARS[braille_idx.min(7)];

            buf.set_string(
                area.left() + x as u16,
                area.top(),
                char,
                Style::default().fg(self.color),
            );
        }
    }
}