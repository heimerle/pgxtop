//! Shared formatting helpers.
//!
//! Deliberately free of any `ratatui` dependency so the data layer can use it
//! too, and so every function here is unit-testable without a terminal.
//!
//! Two byte formatters exist on purpose:
//!   * [`bytes_si`]  — decimal (1000-based), an exact port of ollama's
//!     `format.HumanBytes`. Use it for Ollama model sizes so the models table
//!     is diffable against `ollama ps` character for character.
//!   * [`bytes_iec`] — binary (1024-based), what nvidia-smi and the kernel
//!     report. Use it for VRAM, system RAM and process memory.

use chrono::{DateTime, TimeDelta, Utc};

use crate::models::inference::Expiry;

/// Placeholder for "we do not have this value". Never render a fabricated 0.
pub const NA: &str = "—";

// ---------------------------------------------------------------------------
// bytes
// ---------------------------------------------------------------------------

/// Decimal (SI, 1000-based) byte formatting — exact port of ollama's
/// `format.HumanBytes`, i.e. the SIZE column of `ollama ps`.
///
/// Note the inherited quirk: values >= 10 lose their decimal, so
/// `96_261_027_921` renders as `"96 GB"`, not `"96.3 GB"`. That is what
/// `ollama ps` prints, and matching it is the point.
pub fn bytes_si(b: u64) -> String {
    const KB: f64 = 1e3;
    const MB: f64 = 1e6;
    const GB: f64 = 1e9;
    const TB: f64 = 1e12;

    let bf = b as f64;
    let (value, unit) = if bf >= TB {
        (bf / TB, "TB")
    } else if bf >= GB {
        (bf / GB, "GB")
    } else if bf >= MB {
        (bf / MB, "MB")
    } else if bf >= KB {
        (bf / KB, "KB")
    } else {
        return format!("{b} B");
    };

    if value >= 10.0 || value == value.trunc() {
        format!("{} {}", value as u64, unit)
    } else {
        format!("{value:.1} {unit}")
    }
}

/// Binary (IEC, 1024-based) byte formatting — matches nvidia-smi and the kernel.
pub fn bytes_iec(b: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const TIB: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

    let bf = b as f64;
    let (value, unit) = if bf >= TIB {
        (bf / TIB, "TiB")
    } else if bf >= GIB {
        (bf / GIB, "GiB")
    } else if bf >= MIB {
        (bf / MIB, "MiB")
    } else if bf >= KIB {
        (bf / KIB, "KiB")
    } else {
        return format!("{b} B");
    };

    // Always one decimal: 101.9 GiB reads better than 102 GiB when the
    // number is being compared against a model's reported VRAM.
    format!("{value:.1} {unit}")
}

/// [`bytes_iec`] without the unit — for `used/total` pairs where the unit is
/// printed once at the end.
pub fn bytes_iec_value(b: u64, unit_of: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const TIB: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

    // Scale by the *larger* value so both halves of "112/119 GiB" share a unit.
    let scale = unit_of as f64;
    let div = if scale >= TIB {
        TIB
    } else if scale >= GIB {
        GIB
    } else if scale >= MIB {
        MIB
    } else if scale >= KIB {
        KIB
    } else {
        return format!("{b}");
    };

    format!("{:.1}", b as f64 / div)
}

/// Just the IEC unit that [`bytes_iec_value`] would have used.
pub fn bytes_iec_unit(b: u64) -> &'static str {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * 1024 * 1024;
    const TIB: u64 = 1024 * 1024 * 1024 * 1024;

    if b >= TIB {
        "TiB"
    } else if b >= GIB {
        "GiB"
    } else if b >= MIB {
        "MiB"
    } else if b >= KIB {
        "KiB"
    } else {
        "B"
    }
}

// ---------------------------------------------------------------------------
// percentages, power, bars
// ---------------------------------------------------------------------------

/// `None` when `total == 0`. Guards against the `NaN`/`inf` that a bare
/// `used as f32 / total as f32` produces when a memory query is unsupported.
pub fn pct(used: u64, total: u64) -> Option<f32> {
    if total == 0 {
        None
    } else {
        Some(used as f32 / total as f32 * 100.0)
    }
}

pub fn pct_str(used: u64, total: u64) -> String {
    match pct(used, total) {
        Some(p) => format!("{p:.0}%"),
        None => NA.to_string(),
    }
}

pub fn watts(w: f32) -> String {
    if w >= 100.0 {
        format!("{w:.0} W")
    } else {
        format!("{w:.1} W")
    }
}

pub fn celsius(c: f32) -> String {
    format!("{c:.0}°C")
}

/// Unicode block bar. Clamps, so a >100% or NaN value cannot blow up the layout.
pub fn bar(percent: f32, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let p = if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    let filled = ((p / 100.0) * width as f32).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

// ---------------------------------------------------------------------------
// context length, digests, uptime
// ---------------------------------------------------------------------------

/// `65536 -> "64K"`, `262144 -> "256K"`, `1048576 -> "1M"`.
///
/// Only abbreviates exact power-of-two multiples so the display never rounds
/// a context length into a lie. `100000` stays `"100000"`.
pub fn context(n: u32) -> String {
    if n == 0 {
        return NA.to_string();
    }
    const K: u32 = 1 << 10;
    const M: u32 = 1 << 20;
    if n >= M && n.is_multiple_of(M) {
        return format!("{}M", n / M);
    }
    if n >= K && n.is_multiple_of(K) {
        return format!("{}K", n / K);
    }
    n.to_string()
}

pub fn context_opt(n: Option<u32>) -> String {
    n.map(context).unwrap_or_else(|| NA.to_string())
}

/// First 12 characters of a digest, respecting char boundaries.
pub fn digest_short(d: &str) -> &str {
    match d.char_indices().nth(12) {
        Some((idx, _)) => &d[..idx],
        None => d,
    }
}

pub fn uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

/// Truncate to `max` characters, appending an ellipsis when it does not fit.
pub fn truncate(s: &str, max: usize) -> std::borrow::Cow<'_, str> {
    if s.chars().count() <= max {
        return s.into();
    }
    if max == 0 {
        return "".into();
    }
    if max == 1 {
        return "…".into();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out.into()
}

// ---------------------------------------------------------------------------
// durations / expiry
// ---------------------------------------------------------------------------

/// Port of ollama's `format.HumanDuration`.
///
/// The `hours` branches use a *rounded* hour count while the final `years`
/// branch uses a *truncated* one — that asymmetry is in the Go original and is
/// reproduced deliberately.
pub fn human_duration(d: TimeDelta) -> String {
    let d = if d < TimeDelta::zero() { TimeDelta::zero() } else { d };

    let seconds = d.num_seconds();
    if seconds < 1 {
        return "Less than a second".to_string();
    }
    if seconds == 1 {
        return "1 second".to_string();
    }
    if seconds < 60 {
        return format!("{seconds} seconds");
    }

    let minutes = d.num_minutes();
    if minutes == 1 {
        return "About a minute".to_string();
    }
    if minutes < 60 {
        return format!("{minutes} minutes");
    }

    let hours = (d.num_milliseconds() as f64 / 3_600_000.0).round() as i64;
    if hours == 1 {
        return "About an hour".to_string();
    }
    if hours < 48 {
        return format!("{hours} hours");
    }
    if hours < 24 * 7 * 2 {
        return format!("{} days", hours / 24);
    }
    if hours < 24 * 30 * 2 {
        return format!("{} weeks", hours / 24 / 7);
    }
    if hours < 24 * 365 * 2 {
        return format!("{} months", hours / 24 / 30);
    }
    format!("{} years", d.num_hours() / 24 / 365)
}

/// The `ollama ps` UNTIL column. `now` is injected so this is deterministic.
pub fn until(expiry: &Expiry, now: DateTime<Utc>) -> String {
    match expiry {
        Expiry::Unknown => NA.to_string(),
        Expiry::Never => "Never".to_string(),
        Expiry::Forever => "Forever".to_string(),
        Expiry::At(t) => {
            let delta = now.signed_duration_since(*t);
            if delta < TimeDelta::zero() {
                format!("{} from now", human_duration(-delta))
            } else {
                // What `ollama ps` actually prints once the deadline passes,
                // verified against Ollama 0.32.14 on the target host.
                "Stopping...".to_string()
            }
        }
    }
}

/// Compact UNTIL, for the models table.
///
/// [`until`] renders ollama's verbose phrasing ("4 minutes from now"), which is
/// right for the detail overlay but 18 characters wide — far more than a table
/// column can give it, and clipping it produced the nonsense "s from now".
pub fn until_compact(expiry: &Expiry, now: DateTime<Utc>) -> String {
    match expiry {
        Expiry::Unknown => NA.to_string(),
        Expiry::Never => "Never".to_string(),
        Expiry::Forever => "Forever".to_string(),
        Expiry::At(t) => {
            let d = t.signed_duration_since(now);
            if d <= TimeDelta::zero() {
                return "stopping".to_string();
            }
            let secs = d.num_seconds();
            if secs < 60 {
                format!("{secs}s")
            } else if secs < 3600 {
                format!("{}m", secs / 60)
            } else if secs < 86_400 {
                format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
            } else {
                format!("{}d", secs / 86_400)
            }
        }
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    #[test]
    fn bytes_si_matches_ollama_human_bytes() {
        assert_eq!(bytes_si(0), "0 B");
        assert_eq!(bytes_si(999), "999 B");
        assert_eq!(bytes_si(1_000), "1 KB");
        assert_eq!(bytes_si(1_500), "1.5 KB");
        assert_eq!(bytes_si(274_000_000), "274 MB");
        assert_eq!(bytes_si(5_137_025_024), "5.1 GB");
        assert_eq!(bytes_si(9_900_000_000), "9.9 GB");
        // >= 10 loses the decimal — ollama's own quirk.
        assert_eq!(bytes_si(10_000_000_000), "10 GB");
        // The real value measured on the PGX: `ollama ps` prints "96 GB".
        assert_eq!(bytes_si(96_261_027_921), "96 GB");
        assert_eq!(bytes_si(2_000_000_000_000), "2 TB");
    }

    #[test]
    fn bytes_iec_is_binary() {
        assert_eq!(bytes_iec(0), "0 B");
        assert_eq!(bytes_iec(1023), "1023 B");
        assert_eq!(bytes_iec(1024), "1.0 KiB");
        assert_eq!(bytes_iec(1024 * 1024), "1.0 MiB");
        // 104367 MiB, the llama-server figure measured on the PGX.
        assert_eq!(bytes_iec(104_367 * 1024 * 1024), "101.9 GiB");
    }

    #[test]
    fn bytes_iec_pair_shares_a_unit() {
        let total = 119 * 1024 * 1024 * 1024u64;
        let used = 112 * 1024 * 1024 * 1024u64;
        assert_eq!(bytes_iec_value(used, total), "112.0");
        assert_eq!(bytes_iec_value(total, total), "119.0");
        assert_eq!(bytes_iec_unit(total), "GiB");
    }

    #[test]
    fn pct_guards_zero_total() {
        assert_eq!(pct(0, 0), None);
        assert_eq!(pct_str(0, 0), NA);
        assert_eq!(pct(50, 100), Some(50.0));
        assert_eq!(pct_str(94, 100), "94%");
    }

    #[test]
    fn bar_clamps_out_of_range_and_nan() {
        assert_eq!(bar(0.0, 4), "░░░░");
        assert_eq!(bar(100.0, 4), "████");
        assert_eq!(bar(150.0, 4), "████");
        assert_eq!(bar(-20.0, 4), "░░░░");
        assert_eq!(bar(f32::NAN, 4), "░░░░");
        assert_eq!(bar(50.0, 0), "");
        assert_eq!(bar(50.0, 4).chars().count(), 4);
    }

    #[test]
    fn context_only_abbreviates_exact_multiples() {
        assert_eq!(context(0), NA);
        assert_eq!(context(4096), "4K");
        assert_eq!(context(8192), "8K");
        assert_eq!(context(65536), "64K");
        assert_eq!(context(262144), "256K");
        assert_eq!(context(1 << 20), "1M");
        assert_eq!(context(100_000), "100000");
        assert_eq!(context_opt(None), NA);
    }

    #[test]
    fn digest_short_is_twelve_chars() {
        assert_eq!(
            digest_short("aaaa1111bbbb2222cccc3333dddd4444eeee5555ffff6666aaaa7777bbbb8888"),
            "aaaa1111bbbb"
        );
        assert_eq!(digest_short("abc"), "abc");
        assert_eq!(digest_short(""), "");
    }

    #[test]
    fn truncate_never_exceeds_max() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("example-moe:q8_0", 10), "example-m…");
        assert_eq!(truncate("abc", 1), "…");
        assert_eq!(truncate("abc", 0), "");
        for max in 0..8 {
            assert!(truncate("a-very-long-model-name", max).chars().count() <= max);
        }
    }

    #[test]
    fn human_duration_matches_ollama() {
        assert_eq!(human_duration(TimeDelta::milliseconds(500)), "Less than a second");
        assert_eq!(human_duration(TimeDelta::seconds(1)), "1 second");
        assert_eq!(human_duration(TimeDelta::seconds(45)), "45 seconds");
        assert_eq!(human_duration(TimeDelta::seconds(61)), "About a minute");
        assert_eq!(human_duration(TimeDelta::minutes(4)), "4 minutes");
        // round(1.5h) == 2h — pins Go-rounding fidelity.
        assert_eq!(human_duration(TimeDelta::minutes(90)), "2 hours");
        assert_eq!(human_duration(TimeDelta::hours(50)), "2 days");
        assert_eq!(human_duration(TimeDelta::hours(24 * 20)), "2 weeks");
        assert_eq!(human_duration(TimeDelta::hours(24 * 100)), "3 months");
    }

    #[test]
    fn until_renders_every_expiry_variant() {
        let now = t0();
        assert_eq!(until(&Expiry::Unknown, now), NA);
        assert_eq!(until(&Expiry::Never, now), "Never");
        assert_eq!(until(&Expiry::Forever, now), "Forever");
        assert_eq!(
            until(&Expiry::At(now + TimeDelta::minutes(4)), now),
            "4 minutes from now"
        );
        assert_eq!(
            until(&Expiry::At(now - TimeDelta::seconds(30)), now),
            "Stopping..."
        );
    }

    /// The table column is 8 wide; every rendering must fit it.
    #[test]
    fn until_compact_fits_a_narrow_column() {
        let now = t0();
        let cases = [
            (Expiry::Unknown, NA),
            (Expiry::Never, "Never"),
            (Expiry::Forever, "Forever"),
        ];
        for (e, expected) in cases {
            assert_eq!(until_compact(&e, now), expected);
        }
        assert_eq!(until_compact(&Expiry::At(now + TimeDelta::seconds(27)), now), "27s");
        assert_eq!(until_compact(&Expiry::At(now + TimeDelta::minutes(4)), now), "4m");
        assert_eq!(
            until_compact(&Expiry::At(now + TimeDelta::minutes(64)), now),
            "1h04m"
        );
        assert_eq!(until_compact(&Expiry::At(now + TimeDelta::days(3)), now), "3d");
        assert_eq!(until_compact(&Expiry::At(now - TimeDelta::seconds(1)), now), "stopping");

        for offset in [-5i64, 0, 27, 240, 3840, 90_000, 999_999] {
            let s = until_compact(&Expiry::At(now + TimeDelta::seconds(offset)), now);
            assert!(s.chars().count() <= 8, "{s:?} is too wide");
        }
    }

    #[test]
    fn uptime_formats() {
        assert_eq!(uptime(0), "0m");
        assert_eq!(uptime(90), "1m");
        assert_eq!(uptime(3661), "1h 1m");
        assert_eq!(uptime(12 * 86400 + 4 * 3600), "12d 4h");
    }
}
