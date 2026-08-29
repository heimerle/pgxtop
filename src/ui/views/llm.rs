//! View `[3]` — the models screen.
//!
//! Composed so it strictly dominates
//! `watch -n 1 'ollama ps; nvidia-smi --query-gpu=...'`: the GPU strip on top,
//! engine status on one line, then the full models table. `Enter` opens the
//! detail overlay.

use chrono::Utc;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::widgets::Widget;
use ratatui::Frame;

use crate::app::App;
use crate::ui::panels::{engines_line, gpu_strip, models_table};
use crate::ui::state::Overlay;
use crate::ui::theme;
use crate::ui::widgets::graph::Graph;

/// Rows the collapsible inference-history panel takes when shown.
const HISTORY_H: u16 = 7;

pub fn render(f: &mut Frame, area: Rect, app: &mut App) {
    let summaries = app.gpu_summaries();
    let bordered_gpu = area.height >= 24;
    let gpu_h = gpu_strip::height(&summaries, area.height);

    // The history panel is permanently empty unless an engine actually reports
    // throughput, so it only takes space once there is something to draw.
    let history_h = if app.models_ui.show_history && app.inference_history.has_data() {
        HISTORY_H.min(area.height / 3)
    } else {
        0
    };

    let [gpu_area, engines_area, table_area, history_area] = Layout::vertical([
        Constraint::Length(gpu_h.min(area.height)),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(history_h),
    ])
    .areas(area);

    gpu_strip::render(
        f,
        gpu_area,
        &app.gpu_info,
        &summaries,
        app.collectors.nvml.init_error(),
        bordered_gpu,
    );

    engines_line::render(f, engines_area, &app.inference_snapshot, &app.models_ui);

    let focused = app.overlay == Overlay::None;
    models_table::render(
        f,
        table_area,
        &app.model_rows,
        &mut app.models_ui,
        focused,
        Utc::now(),
    );

    if history_h > 0 {
        render_history(f, history_area, app);
    }
}

fn render_history(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" INFERENCE (tok/s) ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 || inner.width == 0 {
        return;
    }

    let half = inner.height / 2;
    let [prompt_area, gen_area] = Layout::vertical([
        Constraint::Length(half),
        Constraint::Length(inner.height - half),
    ])
    .areas(inner);

    // Autoscaled: a fixed max_value is what made the old graphs either flat or
    // out of range.
    let mut buf_area = prompt_area;
    if buf_area.height > 0 {
        Graph::autoscaled(app.inference_history.prompt_tok_s.as_slice(), 1.0, Color::Yellow)
            .render(buf_area, f.buffer_mut());
    }
    buf_area = gen_area;
    if buf_area.height > 0 {
        Graph::autoscaled(app.inference_history.gen_tok_s.as_slice(), 1.0, theme::OK)
            .render(buf_area, f.buffer_mut());
    }
}
