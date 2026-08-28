pub mod views;
pub mod widgets;

use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, View};
use crate::config::Config;

pub struct Ui {
    config: Config,
}

impl Ui {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn render<B: Backend>(&self, terminal: &mut ratatui::Terminal<B>, app: &App) -> anyhow::Result<()> {
        terminal.draw(|f| self.render_frame(f, app))?;
        Ok(())
    }

    fn render_frame(&self, f: &mut Frame, app: &App) {
        let size = f.area();

        // Header
        let header = self.render_header(app);
        f.render_widget(header, Rect::new(0, 0, size.width, 3));

        // Main content based on view
        let main_area = Rect::new(0, 3, size.width, size.height - 3);
        match app.current_view {
            View::Overview => views::overview::render(f, main_area, app),
            View::Gpu => views::gpu::render(f, main_area, app),
            View::Llm => views::llm::render(f, main_area, app),
            View::Processes => views::processes::render(f, main_area, app),
            View::System => views::system::render(f, main_area, app),
            View::Network => views::network::render(f, main_area, app),
        }

        // Footer
        let footer = self.render_footer(app);
        f.render_widget(footer, Rect::new(0, size.height - 1, size.width, 1));
    }

    fn render_header(&self, app: &App) -> Paragraph {
        let title = match app.current_view {
            View::Overview => "OVERVIEW",
            View::Gpu => "GPU",
            View::Llm => "LLM ENGINES",
            View::Processes => "PROCESSES",
            View::System => "SYSTEM",
            View::Network => "NETWORK",
        };

        let status = if app.paused { "⏸ PAUSED" } else { "● LIVE" };
        let uptime = app.system_info.as_ref().map(|s| format_uptime(s.uptime)).unwrap_or_default();

        Paragraph::new(Line::from(vec![
            Span::raw(format!(" pgxtop ")),
            Span::raw(format!("[{}] ", title)),
            Span::raw(format!("uptime {} ", uptime)),
            Span::raw(status),
        ]))
        .style(Style::default().bg(Color::Blue).fg(Color::White))
    }

    fn render_footer(&self, app: &App) -> Paragraph {
        let keys = "1-6:Views  Tab:Panel  ↑↓jk:Select  ←→hl:Change  Enter:Details  s:Sort  r:Refresh  p:Pause  ?:Help  q:Quit";
        Paragraph::new(Line::raw(keys))
            .style(Style::default().bg(Color::DarkGray).fg(Color::White))
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}