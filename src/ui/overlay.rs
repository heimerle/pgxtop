//! Modal overlays: `?` help and the `Enter` model detail.

use chrono::{DateTime, Utc};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::View;
use crate::models::GpuProcess;
use crate::ui::panels::model_detail;
use crate::ui::state::ModelRow;
use crate::ui::{keys, theme};

pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [a] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [a] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(a);
    a
}

fn modal_block<'a>(title: &'a str, hint: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(Line::from(hint).right_aligned())
        .border_style(Style::default().fg(theme::ACCENT))
        .style(Style::default().bg(theme::OVERLAY_BG))
}

pub fn render_help(f: &mut Frame, area: Rect, view: View) {
    let rows = keys::help(view);
    let rect = centered(
        area,
        52.min(area.width),
        (rows.len() as u16 + 2).min(area.height),
    );
    if rect.width < 8 || rect.height < 3 {
        return;
    }
    // Without Clear the modal would be drawn over the live view underneath.
    f.render_widget(Clear, rect);
    let block = modal_block(" HELP ", " Esc close ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, d)| {
            if k.is_empty() {
                Line::raw("")
            } else {
                Line::from(vec![
                    Span::styled(
                        format!(" {k:<11}"),
                        Style::default()
                            .fg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(*d, Style::default().fg(theme::TEXT)),
                ])
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

pub fn render_model_detail(
    f: &mut Frame,
    area: Rect,
    row: &ModelRow,
    procs: &[GpuProcess],
    now: DateTime<Utc>,
    loading: bool,
) {
    let rect = centered(area, 74.min(area.width), 28.min(area.height));
    if rect.width < 20 || rect.height < 5 {
        return;
    }
    f.render_widget(Clear, rect);
    let block = modal_block(" MODEL DETAIL ", " Esc close ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let body = model_detail::lines(row, procs, inner.width, now, loading);
    f.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_never_exceeds_the_area() {
        for w in 1..40u16 {
            for h in 1..20u16 {
                let area = Rect::new(0, 0, w, h);
                let r = centered(area, 74, 28);
                assert!(r.width <= w && r.height <= h);
                assert!(r.x + r.width <= w && r.y + r.height <= h);
            }
        }
    }

    #[test]
    fn centered_is_actually_centered() {
        let r = centered(Rect::new(0, 0, 100, 40), 50, 20);
        assert_eq!(r.x, 25);
        assert_eq!(r.y, 10);
    }
}
