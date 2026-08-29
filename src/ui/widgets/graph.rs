//! Block-column history graph.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

pub struct Graph<'a> {
    data: &'a [f32],
    max_value: f32,
    color: Color,
}

impl<'a> Graph<'a> {
    pub fn new(data: &'a [f32], max_value: f32, color: Color) -> Self {
        Self { data, max_value, color }
    }

    /// Scales to the largest finite sample, with a floor so a flat-zero series
    /// does not blow the scale up.
    pub fn autoscaled(data: &'a [f32], floor: f32, color: Color) -> Self {
        let max = data
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(floor, f32::max);
        Self { data, max_value: max.max(floor).max(f32::EPSILON), color }
    }
}

impl Widget for Graph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.data.is_empty() {
            return;
        }
        if !self.max_value.is_finite() || self.max_value <= 0.0 {
            return;
        }

        let width = area.width as usize;
        let height = area.height as usize;
        let len = self.data.len();

        for x in 0..width {
            // Map the column onto the series directly. The previous code used
            // an integer `step`, which was 0 whenever `len <= width` — so every
            // column read `data[0]` and the graph was a flat line.
            let idx = if width == 1 || len == 1 {
                len - 1
            } else {
                x * (len - 1) / (width - 1)
            };
            let value = self.data[idx];

            // NaN marks a sample the source never reported: draw a gap.
            let bar_height = if value.is_finite() {
                let frac = (value / self.max_value).clamp(0.0, 1.0);
                // Unclamped, this underflowed `height - bar_height` for any
                // value above max_value and aborted the process.
                ((frac * height as f32).round() as usize).min(height)
            } else {
                0
            };

            for y in 0..height {
                let symbol = if y < bar_height { "█" } else { " " };
                let px = area.left() + x as u16;
                let py = area.top() + (height - 1 - y) as u16;
                if px < area.right() && py < area.bottom() {
                    buf.set_string(px, py, symbol, Style::default().fg(self.color));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(data: &[f32], max: f32, w: u16, h: u16) -> String {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        Graph::new(data, max, Color::Green).render(area, &mut buf);
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Regression: with fewer samples than columns the old `step` was 0, so
    /// every column read `data[0]` and the graph was flat.
    #[test]
    fn fewer_samples_than_columns_still_spans_the_series() {
        let data = [0.0, 100.0];
        let s = render_to_string(&data, 100.0, 8, 1);
        assert!(s.starts_with(' '), "left edge should be the low sample: {s:?}");
        assert!(s.ends_with('█'), "right edge should be the high sample: {s:?}");
    }

    /// Regression: `height - bar_height` underflowed and, with
    /// `panic = "abort"`, killed the process without restoring the terminal.
    #[test]
    fn values_above_the_maximum_do_not_panic() {
        let data = [5000.0, 12000.0];
        let s = render_to_string(&data, 1000.0, 10, 3);
        assert!(s.contains('█'));
    }

    #[test]
    fn nan_samples_render_as_gaps() {
        let data = [100.0, f32::NAN, 100.0];
        let s = render_to_string(&data, 100.0, 3, 1);
        assert_eq!(s, "█ █");
    }

    #[test]
    fn degenerate_areas_and_inputs_are_no_ops() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        Graph::new(&[1.0], 1.0, Color::Green).render(Rect::new(0, 0, 0, 0), &mut buf);
        Graph::new(&[], 1.0, Color::Green).render(Rect::new(0, 0, 4, 2), &mut buf);
        Graph::new(&[1.0], 0.0, Color::Green).render(Rect::new(0, 0, 4, 2), &mut buf);
        Graph::new(&[1.0], f32::NAN, Color::Green).render(Rect::new(0, 0, 4, 2), &mut buf);
    }

    #[test]
    fn never_panics_across_a_range_of_sizes() {
        let data: Vec<f32> = (0..300).map(|i| i as f32).collect();
        for w in 1..40u16 {
            for h in 1..12u16 {
                let area = Rect::new(0, 0, w, h);
                let mut buf = Buffer::empty(area);
                Graph::new(&data, 150.0, Color::Green).render(area, &mut buf);
            }
        }
    }

    #[test]
    fn autoscale_uses_the_largest_finite_sample() {
        let data = [0.0, f32::NAN, 50.0];
        let s = {
            let area = Rect::new(0, 0, 3, 2);
            let mut buf = Buffer::empty(area);
            Graph::autoscaled(&data, 1.0, Color::Green).render(area, &mut buf);
            (0..2)
                .map(|y| (0..3).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        };
        // The 50.0 sample is the maximum, so its column is full height.
        assert!(s.lines().all(|l| l.ends_with('█')), "{s:?}");
    }
}
