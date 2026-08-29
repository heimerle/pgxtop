use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::format;
use crate::models::ProcessInfo;
use crate::ui::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block(" PROCESSES ", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines = vec![Line::from(Span::styled(
        format!(
            " {:>7} {:<22} {:>7} {:>11} {:>11} {}",
            "PID", "PROCESS", "CPU%", "MEM", "GPU MEM", "MODEL"
        ),
        Style::default()
            .fg(theme::MUTED)
            .add_modifier(Modifier::BOLD),
    ))];

    // Sorted once per refresh in `App`, not per frame.
    for p in app
        .system_processes
        .iter()
        .take(inner.height.saturating_sub(1) as usize)
    {
        lines.push(process_line(p, app));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn process_line(p: &ProcessInfo, app: &App) -> Line<'static> {
    let gpu = app.gpu_processes.iter().find(|g| g.pid == p.pid);
    let gpu_mem = gpu
        .and_then(|g| g.used_memory)
        .map(format::bytes_iec)
        .unwrap_or_else(|| format::NA.to_string());
    let model = gpu
        .and_then(|g| g.model.clone())
        .unwrap_or_else(|| format::NA.to_string());

    Line::from(vec![
        Span::raw(format!(
            " {:>7} {:<22} {:>6.1}% {:>11} ",
            p.pid,
            format::truncate(&p.name, 22),
            p.cpu_usage,
            // sysinfo reports bytes; the old code divided by 1024 and labelled
            // the result MB, which was off by a factor of 1024.
            format::bytes_iec(p.memory_usage)
        )),
        Span::styled(
            format!("{gpu_mem:>11} "),
            Style::default().fg(if gpu.is_some() {
                theme::ACCENT
            } else {
                theme::MUTED
            }),
        ),
        Span::raw(format::truncate(&model, 28).into_owned()),
    ])
}
