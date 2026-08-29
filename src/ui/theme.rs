//! The single place colours are decided.
//!
//! Previously there were ten inline `Color::` literals across the views and no
//! threshold logic at all, so nothing ever turned red.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};

pub const TEXT: Color = Color::Gray;
pub const MUTED: Color = Color::DarkGray;
pub const ACCENT: Color = Color::Cyan;
pub const OK: Color = Color::Green;
pub const WARN: Color = Color::Yellow;
pub const CRIT: Color = Color::Red;

pub const HEADER_BG: Color = Color::Blue;
pub const HEADER_FG: Color = Color::White;
pub const FOOTER_BG: Color = Color::DarkGray;
pub const SEL_BG: Color = Color::Rgb(38, 54, 70);
pub const OVERLAY_BG: Color = Color::Black;

pub fn util_color(p: f32) -> Color {
    if p >= 95.0 {
        CRIT
    } else if p >= 80.0 {
        WARN
    } else {
        OK
    }
}

pub fn mem_color(p: f32) -> Color {
    if p >= 95.0 {
        CRIT
    } else if p >= 85.0 {
        WARN
    } else {
        OK
    }
}

pub fn temp_color(c: f32) -> Color {
    if c >= 85.0 {
        CRIT
    } else if c >= 75.0 {
        WARN
    } else {
        OK
    }
}

/// CPU offload is the single most actionable signal on an AI workstation: a
/// partially offloaded model runs an order of magnitude slower, so anything
/// short of fully resident is loud.
pub fn processor_style(gpu_fraction: Option<f32>) -> Style {
    match gpu_fraction {
        // Unknown must never look like a value.
        None => Style::default().fg(MUTED),
        Some(f) if f >= 0.999 => Style::default().fg(OK),
        Some(f) if f >= 0.5 => Style::default().fg(WARN),
        Some(_) => Style::default().fg(CRIT).add_modifier(Modifier::BOLD),
    }
}

pub fn engine_status_color(status: crate::engines::EngineStatus) -> Color {
    match status {
        crate::engines::EngineStatus::Connected => OK,
        crate::engines::EngineStatus::Unavailable => CRIT,
        crate::engines::EngineStatus::Connecting => WARN,
    }
}

pub fn panel_block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if focused { ACCENT } else { MUTED }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_are_exact_at_the_boundaries() {
        assert_eq!(util_color(79.9), OK);
        assert_eq!(util_color(80.0), WARN);
        assert_eq!(util_color(94.9), WARN);
        assert_eq!(util_color(95.0), CRIT);

        assert_eq!(mem_color(84.9), OK);
        assert_eq!(mem_color(85.0), WARN);
        assert_eq!(mem_color(95.0), CRIT);

        assert_eq!(temp_color(74.9), OK);
        assert_eq!(temp_color(75.0), WARN);
        assert_eq!(temp_color(85.0), CRIT);
    }

    #[test]
    fn unknown_placement_is_muted_not_green() {
        assert_eq!(processor_style(None).fg, Some(MUTED));
        assert_eq!(processor_style(Some(1.0)).fg, Some(OK));
        assert_eq!(processor_style(Some(0.63)).fg, Some(WARN));
        assert_eq!(processor_style(Some(0.2)).fg, Some(CRIT));
    }
}
