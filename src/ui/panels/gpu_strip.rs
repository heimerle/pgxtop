//! The compact GPU strip: exactly the fields of
//! `nvidia-smi --query-gpu=memory.used,memory.total,utilization.gpu,temperature.gpu`,
//! plus power draw.
//!
//! On unified-memory hardware (GB10) NVML answers nothing about the frame
//! buffer, so the row reports host memory labelled `UNIFIED` plus the
//! GPU-resident total — never a fabricated `0/0 GB`.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::format;
use crate::models::{GpuInfo, GpuMemory, GpuSummary};
use crate::ui::theme;

/// Rows needed for `n` GPUs, borders included.
pub fn height(summaries: &[GpuSummary], area_h: u16) -> u16 {
    if summaries.is_empty() {
        return if area_h >= 24 { 3 } else { 1 };
    }
    let per_gpu: u16 = summaries
        .iter()
        .map(|s| if s.memory.is_unified() { 2 } else { 1 })
        .sum();
    let capped = per_gpu.min(8);
    if area_h >= 24 {
        capped + 2
    } else {
        capped
    }
}

fn short_name(name: &str) -> String {
    name.trim_start_matches("NVIDIA ").trim().to_string()
}

fn na() -> Span<'static> {
    Span::styled(format::NA, Style::default().fg(theme::MUTED))
}

fn mem_spans(mem: &GpuMemory, width: u16) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!(" {:<7} ", mem.label()),
        Style::default().fg(theme::MUTED),
    )];

    let (Some(used), Some(total)) = (mem.used(), mem.total()) else {
        spans.push(Span::styled(
            "not reported by NVML",
            Style::default().fg(theme::MUTED),
        ));
        return spans;
    };

    let pct = format::pct(used, total).unwrap_or(0.0);
    let color = theme::mem_color(pct);
    if width >= 78 {
        spans.push(Span::styled(format::bar(pct, 16), Style::default().fg(color)));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!(
            "{}/{} {} ({:>3.0}%)",
            format::bytes_iec_value(used, total),
            format::bytes_iec_value(total, total),
            format::bytes_iec_unit(total),
            pct
        ),
        Style::default().fg(color),
    ));
    spans
}

fn stats_spans(s: &GpuSummary, width: u16) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    spans.push(Span::styled("  UTIL ", Style::default().fg(theme::MUTED)));
    spans.push(match s.util_gpu_pct {
        Some(u) => Span::styled(format!("{u:>3.0}%"), Style::default().fg(theme::util_color(u))),
        None => na(),
    });

    spans.push(Span::styled("  TEMP ", Style::default().fg(theme::MUTED)));
    spans.push(match s.temp_c {
        Some(t) => Span::styled(
            format::celsius(t),
            Style::default().fg(theme::temp_color(t)),
        ),
        None => na(),
    });

    if width >= 92 {
        spans.push(Span::styled("  PWR ", Style::default().fg(theme::MUTED)));
        spans.push(match (s.power_watts, s.power_limit_watts) {
            (Some(p), Some(limit)) => Span::raw(format!("{}/{}", format::watts(p), format::watts(limit))),
            // GB10 reports draw but no limit; showing "0 W" as a limit would
            // be an invention.
            (Some(p), None) => Span::raw(format::watts(p)),
            (None, _) => na(),
        });
    }

    spans
}

/// Pure — the rendering tests assert on these lines, no backend needed.
pub fn lines(
    infos: &[GpuInfo],
    summaries: &[GpuSummary],
    width: u16,
    nvml_error: Option<&str>,
) -> Vec<Line<'static>> {
    if summaries.is_empty() {
        let msg = match nvml_error {
            Some(e) => format!(" NVML unavailable — {e}"),
            None => " NVML unavailable — no NVIDIA GPU detected".to_string(),
        };
        return vec![Line::from(Span::styled(
            msg,
            Style::default().fg(theme::MUTED),
        ))];
    }

    let mut out = Vec::new();
    for s in summaries {
        let name = infos
            .iter()
            .find(|i| i.index == s.index)
            .map(|i| short_name(&i.name))
            .unwrap_or_default();

        let mut head = vec![Span::styled(
            format!(" GPU{} ", s.index),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )];
        if width >= 100 && !name.is_empty() {
            head.push(Span::styled(
                format!("{:<14}", format::truncate(&name, 14)),
                Style::default().fg(theme::TEXT),
            ));
        }

        if s.memory.is_unified() {
            // Two rows: shared host memory, then what is actually GPU-resident.
            let mut first = head.clone();
            first.extend(mem_spans(&s.memory, width));
            out.push(Line::from(first));

            let indent = if width >= 100 && !name.is_empty() { 20 } else { 6 };
            let mut second = vec![Span::raw(" ".repeat(indent))];
            second.push(Span::styled(
                format!("{:<7} ", "GPU-RES"),
                Style::default().fg(theme::MUTED),
            ));
            second.push(match s.memory.gpu_resident() {
                Some(b) => Span::styled(format::bytes_iec(b), Style::default().fg(theme::ACCENT)),
                None => na(),
            });
            second.extend(stats_spans(s, width));
            out.push(Line::from(second));
        } else {
            let mut line = head;
            line.extend(mem_spans(&s.memory, width));
            line.extend(stats_spans(s, width));
            out.push(Line::from(line));
        }
    }
    out
}

pub fn render(
    f: &mut Frame,
    area: Rect,
    infos: &[GpuInfo],
    summaries: &[GpuSummary],
    nvml_error: Option<&str>,
    bordered: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let inner = if bordered {
        let block = theme::panel_block(" GPU ", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        inner
    } else {
        area
    };
    if inner.height == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(lines(infos, summaries, inner.width, nvml_error)),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn info(index: u32, name: &str) -> GpuInfo {
        GpuInfo {
            index,
            name: name.into(),
            uuid: "GPU-x".into(),
            memory_total: None,
            power_limit_watts: None,
            total_energy: None,
        }
    }

    /// The measured GB10 state: no frame buffer, but util, temp, power draw
    /// and per-process residency all work.
    fn gb10() -> GpuSummary {
        GpuSummary {
            index: 0,
            memory: GpuMemory::Unified {
                host_used: 112 * 1024 * 1024 * 1024,
                host_total: 119 * 1024 * 1024 * 1024,
                gpu_resident: 104_367 * 1024 * 1024,
            },
            util_gpu_pct: Some(95.0),
            temp_c: Some(67.0),
            power_watts: Some(38.31),
            power_limit_watts: None,
        }
    }

    #[test]
    fn unified_memory_is_labelled_and_never_shows_zero() {
        let s = text(&lines(&[info(0, "NVIDIA GB10")], &[gb10()], 120, None));
        assert!(s.contains("UNIFIED"), "{s}");
        assert!(!s.contains("VRAM"), "must not call unified memory VRAM: {s}");
        assert!(s.contains("112.0/119.0 GiB"), "{s}");
        assert!(s.contains("94%"), "{s}");
        assert!(s.contains("GPU-RES"), "{s}");
        assert!(s.contains("101.9 GiB"), "{s}");
        assert!(s.contains("UTIL  95%"), "{s}");
        assert!(s.contains("67°C"), "{s}");
        // Power draw is reported; the limit is not, and must not be invented.
        assert!(s.contains("38.3 W"), "{s}");
        assert!(!s.contains("0.0 W"), "{s}");
        assert!(!s.contains("0/0"), "{s}");
    }

    #[test]
    fn dedicated_vram_uses_the_vram_label_and_one_row() {
        let s = GpuSummary {
            index: 0,
            memory: GpuMemory::Dedicated {
                used: 40 * 1024 * 1024 * 1024,
                total: 48 * 1024 * 1024 * 1024,
            },
            util_gpu_pct: Some(87.0),
            temp_c: Some(61.0),
            power_watts: Some(210.0),
            power_limit_watts: Some(300.0),
        };
        let l = lines(&[info(0, "NVIDIA RTX 6000 Ada")], &[s], 120, None);
        assert_eq!(l.len(), 1);
        let t = text(&l);
        assert!(t.contains("VRAM"), "{t}");
        assert!(t.contains("40.0/48.0 GiB"), "{t}");
        assert!(t.contains("210 W/300 W"), "{t}");
        // The "NVIDIA " prefix is noise when every device has it.
        assert!(t.contains("RTX 6000 Ada"), "{t}");
    }

    #[test]
    fn unsupported_memory_says_so_instead_of_showing_zero() {
        let s = GpuSummary {
            index: 0,
            memory: GpuMemory::Unavailable,
            util_gpu_pct: Some(95.0),
            temp_c: None,
            power_watts: None,
            power_limit_watts: None,
        };
        let t = text(&lines(&[info(0, "NVIDIA GB10")], &[s], 120, None));
        assert!(t.contains("not reported by NVML"), "{t}");
        assert!(t.contains(format::NA), "temp must render as a dash: {t}");
        assert!(!t.contains("0%"), "{t}");
    }

    #[test]
    fn no_gpu_reports_the_nvml_error() {
        let t = text(&lines(&[], &[], 120, Some("libnvidia-ml.so not found")));
        assert!(t.contains("NVML unavailable"), "{t}");
        assert!(t.contains("libnvidia-ml.so not found"), "{t}");
    }

    #[test]
    fn narrow_widths_drop_decoration_but_never_the_numbers() {
        for w in [60u16, 78, 92, 100, 140] {
            let t = text(&lines(&[info(0, "NVIDIA GB10")], &[gb10()], w, None));
            assert!(t.contains("UTIL"), "w={w}: {t}");
            assert!(t.contains("TEMP"), "w={w}: {t}");
            assert!(t.contains("112.0/119.0 GiB"), "w={w}: {t}");
        }
        // The bar and the device name are the first things to go.
        let narrow = text(&lines(&[info(0, "NVIDIA GB10")], &[gb10()], 60, None));
        assert!(!narrow.contains('█'), "{narrow}");
        assert!(!narrow.contains("GB10"), "{narrow}");
    }

    #[test]
    fn height_accounts_for_the_second_unified_row() {
        assert_eq!(height(&[gb10()], 40), 4); // 2 rows + borders
        assert_eq!(height(&[gb10()], 20), 2); // borderless when short
        assert_eq!(height(&[], 40), 3);
        assert_eq!(height(&[], 20), 1);
    }
}
