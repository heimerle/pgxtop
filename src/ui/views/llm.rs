use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Min(10),
        ])
        .split(area);

    render_engines(f, chunks[0], app);
    render_models(f, chunks[1], app);
}

fn render_engines(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("ENGINES")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    for engine in &app.inference_engines {
        let status_icon = match engine.status {
            crate::engines::EngineStatus::Connected => "●",
            crate::engines::EngineStatus::Unavailable => "○",
            crate::engines::EngineStatus::Connecting => "◐",
        };

        lines.push(Line::from(vec![
            Span::raw(format!(" {} {} ", status_icon, engine.name)),
            Span::raw(format!("{:?}", engine.engine_type)),
            Span::raw(format!(" {}", engine.url)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_models(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title("MODELS")
        .borders(Borders::all());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::raw(format!(" {:<30} {:<10} {:<10} {:<10} ", "MODEL", "ENGINE", "VRAM", "STATUS")),
    ]));

    for model in &app.model_instances {
        let vram = model.vram_usage
            .map(|v| format!("{:.1} GB", v as f32 / 1024.0 / 1024.0))
            .unwrap_or_else(|| "N/A".to_string());

        let status = match model.status {
            crate::engines::ModelStatus::Loaded => "loaded",
            crate::engines::ModelStatus::Active => "active",
            crate::engines::ModelStatus::Idle => "idle",
            crate::engines::ModelStatus::Unloading => "unloading",
        };

        lines.push(Line::from(vec![
            Span::raw(format!(" {:<30} {:<10} {:<10} {:<10} ",
                model.name, model.engine_id, vram, status)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}