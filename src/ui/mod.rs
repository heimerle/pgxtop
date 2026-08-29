pub mod keys;
pub mod overlay;
pub mod panels;
pub mod state;
pub mod theme;
pub mod views;
pub mod widgets;

use chrono::Utc;
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};

use crate::app::{App, View};
use crate::format;
use crate::ui::state::Overlay;

/// Below this the layout has nothing meaningful to say.
pub const MIN_WIDTH: u16 = 40;
pub const MIN_HEIGHT: u16 = 8;

pub fn draw<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> anyhow::Result<()> {
    terminal.draw(|f| render_frame(f, app))?;
    Ok(())
}

fn render_frame(f: &mut Frame, app: &mut App) {
    let area = f.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        f.render_widget(
            Paragraph::new(format!(
                "terminal too small\nneed {MIN_WIDTH}x{MIN_HEIGHT}"
            ))
            .centered(),
            area,
        );
        return;
    }

    // A Layout rather than hand-computed Rects: it saturates, so the two
    // `size.height - n` underflow panics are gone, and the main area no longer
    // runs underneath the footer.
    let [header, main, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(f, header, app);

    match app.current_view {
        View::Overview => views::overview::render(f, main, app),
        View::Gpu => views::gpu::render(f, main, app),
        View::Llm => views::llm::render(f, main, app),
        View::Processes => views::processes::render(f, main, app),
        View::System => views::system::render(f, main, app),
        View::Network => views::network::render(f, main, app),
    }

    render_footer(f, footer, app);

    match app.overlay {
        Overlay::Help => overlay::render_help(f, area, app.current_view),
        Overlay::Detail => {
            if let Some(row) = app.models_ui.selected(&app.model_rows) {
                overlay::render_model_detail(
                    f,
                    area,
                    row,
                    &app.gpu_processes,
                    Utc::now(),
                    row.detail.is_none(),
                );
            }
        }
        Overlay::None => {}
    }
}

fn view_title(view: View) -> &'static str {
    match view {
        View::Overview => "OVERVIEW",
        View::Gpu => "GPU",
        View::Llm => "MODELS",
        View::Processes => "PROCESSES",
        View::System => "SYSTEM",
        View::Network => "NETWORK",
    }
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let host = app
        .system_info
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "pgxtop".to_string());
    let uptime = app
        .system_info
        .as_ref()
        .map(|s| format::uptime(s.uptime))
        .unwrap_or_default();

    let left = Line::from(vec![
        Span::styled(
            " pgxtop ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("[{}] ", view_title(app.current_view))),
        Span::raw(host),
        Span::raw(if uptime.is_empty() {
            String::new()
        } else {
            format!("  uptime {uptime}")
        }),
    ]);

    let right = Line::from(if app.paused {
        Span::raw("⏸ PAUSED ")
    } else {
        Span::raw("● LIVE ")
    })
    .right_aligned();

    let style = Style::default().bg(theme::HEADER_BG).fg(theme::HEADER_FG);
    // Two Rects, not two Paragraphs into one: a Paragraph repaints its whole
    // area, so the right-aligned one would erase the left-hand text.
    let status_w = 10.min(area.width);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(status_w)]).areas(area);
    f.render_widget(Paragraph::new(left).style(style), left_area);
    f.render_widget(Paragraph::new(right).style(style), right_area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let bindings = keys::footer(app.current_view, app.overlay, app.models_ui.multi_engine);
    let mut spans = Vec::new();
    for (key, label) in bindings {
        spans.push(Span::styled(
            format!(" {key}"),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {label} ")));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::FOOTER_BG)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn buffer_text(buf: &Buffer) -> String {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn header_line(width: u16, paused: bool) -> String {
        let mut t = Terminal::new(TestBackend::new(width, 1)).unwrap();
        t.draw(|f| {
            let area = f.area();
            let style = Style::default();
            let left = Line::from(" pgxtop [MODELS] workstation");
            let right = Line::from(if paused { "⏸ PAUSED " } else { "● LIVE " }).right_aligned();
            let status_w = 10.min(area.width);
            let [l, r] =
                Layout::horizontal([Constraint::Min(0), Constraint::Length(status_w)]).areas(area);
            f.render_widget(Paragraph::new(left).style(style), l);
            f.render_widget(Paragraph::new(right).style(style), r);
        })
        .unwrap();
        buffer_text(t.backend().buffer())
    }

    /// Regression: rendering both halves into the same Rect made the
    /// right-aligned status repaint the row and erase the title.
    #[test]
    fn the_header_shows_both_halves() {
        let s = header_line(60, false);
        assert!(s.contains("pgxtop"), "{s}");
        assert!(s.contains("[MODELS]"), "{s}");
        assert!(s.contains("LIVE"), "{s}");

        let paused = header_line(60, true);
        assert!(paused.contains("pgxtop"), "{paused}");
        assert!(paused.contains("PAUSED"), "{paused}");
    }
}
