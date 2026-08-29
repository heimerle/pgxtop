use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::models::series::Series;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub uuid: String,
    /// `None` when NVML does not support a memory query on this device.
    /// Grace-Blackwell (GB10) reports `N/A` for all FB memory fields because
    /// the GPU shares the host's unified LPDDR5X — see [`GpuMemory::Unified`].
    pub memory_total: Option<u64>,
    /// Enforced power limit, WATTS. NVML reports milliwatts; the collector
    /// converts. `None` on devices that do not expose a limit (GB10).
    pub power_limit_watts: Option<f32>,
    pub total_energy: Option<f64>,
}

/// How a device's memory can be accounted for.
///
/// Keeping this an enum rather than a `(used, total)` pair is what stops the
/// UI from rendering a fabricated `0/0 GB` on hardware where NVML simply does
/// not answer the question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuMemory {
    /// Classic discrete VRAM: NVML `memory_info()` succeeded.
    Dedicated { used: u64, total: u64 },
    /// Unified memory (NVIDIA GB10 / Grace-Blackwell): NVML reports no frame
    /// buffer at all, so the meaningful figures are the *host* memory the GPU
    /// shares, plus how much of it currently sits in GPU-resident allocations.
    Unified {
        host_used: u64,
        host_total: u64,
        gpu_resident: u64,
    },
    /// NVML unavailable, or the query failed for an unknown reason.
    Unavailable,
}

impl GpuMemory {
    /// Short label for the memory row: never says "VRAM" about unified memory.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Dedicated { .. } => "VRAM",
            Self::Unified { .. } => "UNIFIED",
            Self::Unavailable => "MEM",
        }
    }

    pub fn used(&self) -> Option<u64> {
        match self {
            Self::Dedicated { used, .. } => Some(*used),
            Self::Unified { host_used, .. } => Some(*host_used),
            Self::Unavailable => None,
        }
    }

    pub fn total(&self) -> Option<u64> {
        match self {
            Self::Dedicated { total, .. } => Some(*total),
            Self::Unified { host_total, .. } => Some(*host_total),
            Self::Unavailable => None,
        }
    }

    /// Bytes held by GPU compute/graphics contexts. On a dedicated card this
    /// is the same as `used`; on a unified system it is the only figure that
    /// is actually GPU-specific.
    pub fn gpu_resident(&self) -> Option<u64> {
        match self {
            Self::Dedicated { used, .. } => Some(*used),
            Self::Unified { gpu_resident, .. } => Some(*gpu_resident),
            Self::Unavailable => None,
        }
    }

    pub fn is_unified(&self) -> bool {
        matches!(self, Self::Unified { .. })
    }

    pub fn percent(&self) -> Option<f32> {
        match (self.used(), self.total()) {
            (Some(u), Some(t)) => crate::format::pct(u, t),
            _ => None,
        }
    }
}

/// `timestamp` and the PCIe counters are collected but not rendered yet.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GpuMetrics {
    pub timestamp: Instant,
    pub utilization_gpu: Option<f32>,
    pub utilization_memory: Option<f32>,
    pub memory: GpuMemory,
    pub temperature: Option<f32>,
    /// WATTS (NVML reports milliwatts; the collector converts).
    pub power_watts: Option<f32>,
    /// WATTS. `None` when unsupported — do not render this as 0.
    pub power_limit_watts: Option<f32>,
    pub sm_clock: Option<u32>,
    pub mem_clock: Option<u32>,
    pub fan_speed: Option<u32>,
    /// KB/s, as NVML reports it.
    pub pcie_tx_kbs: Option<u32>,
    pub pcie_rx_kbs: Option<u32>,
}

/// How confidently a GPU process was attributed to an engine and a model.
///
/// `impl-plan.md` §6: "Never present guesses as confirmed facts."
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MappingConfidence {
    /// Derived from an unambiguous identifier (the model blob referenced in
    /// the runner's cmdline, or a one-process/one-model situation).
    Confirmed,
    /// Derived from a heuristic (context length or memory footprint).
    Inferred,
    /// Engine may be known, model is not.
    Unknown,
}

impl MappingConfidence {
    pub fn marker(self) -> &'static str {
        match self {
            Self::Confirmed => "✓",
            Self::Inferred => "~",
            Self::Unknown => "?",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProcess {
    pub pid: u32,
    /// Short process name (`/proc/<pid>/comm`), e.g. `llama-server`.
    pub name: String,
    /// Full command line, when readable. This — not `comm` and not
    /// `/proc/<pid>/exe`, which needs ptrace rights we do not have when the
    /// runner belongs to another user — is what identifies the engine.
    pub cmdline: Option<String>,
    /// Which device this process is resident on.
    pub gpu_index: u32,
    /// `None` when NVML reports `UsedGpuMemory::Unavailable`; that is an
    /// unknown, not a zero.
    pub used_memory: Option<u64>,
    /// True for a graphics context rather than a compute context.
    pub graphics: bool,
    pub engine: Option<String>,
    pub model: Option<String>,
    pub confidence: MappingConfidence,
}

/// Exactly the four fields of
/// `nvidia-smi --query-gpu=memory.used,memory.total,utilization.gpu,temperature.gpu`,
/// plus power draw, which is the other thing that actually works on a GB10.
#[derive(Debug, Clone, Copy)]
pub struct GpuSummary {
    pub index: u32,
    pub memory: GpuMemory,
    pub util_gpu_pct: Option<f32>,
    pub temp_c: Option<f32>,
    pub power_watts: Option<f32>,
    pub power_limit_watts: Option<f32>,
}

impl GpuSummary {
    pub fn from_parts(info: &GpuInfo, m: &GpuMetrics) -> Self {
        Self {
            index: info.index,
            memory: m.memory,
            util_gpu_pct: m.utilization_gpu,
            temp_c: m.temperature,
            power_watts: m.power_watts,
            power_limit_watts: m.power_limit_watts.or(info.power_limit_watts),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GpuHistory {
    pub utilization: Series,
    pub memory: Series,
    pub temperature: Series,
    pub power: Series,
    pub max_points: usize,
}

impl GpuHistory {
    pub fn new(max_points: usize) -> Self {
        Self {
            utilization: Series::with_capacity(max_points),
            memory: Series::with_capacity(max_points),
            temperature: Series::with_capacity(max_points),
            power: Series::with_capacity(max_points),
            max_points,
        }
    }

    pub fn push(&mut self, metrics: &GpuMetrics) {
        self.utilization.push(metrics.utilization_gpu, self.max_points);
        // Memory as a percentage of the device's own total, so the series is
        // comparable across dedicated and unified devices.
        self.memory.push(metrics.memory.percent(), self.max_points);
        self.temperature.push(metrics.temperature, self.max_points);
        self.power.push(metrics.power_watts, self.max_points);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_memory_reports_host_totals_and_gpu_residency() {
        let m = GpuMemory::Unified {
            host_used: 112 * 1024 * 1024 * 1024,
            host_total: 119 * 1024 * 1024 * 1024,
            gpu_resident: 104_367 * 1024 * 1024,
        };
        assert_eq!(m.label(), "UNIFIED");
        assert!(m.is_unified());
        assert_eq!(m.total(), Some(119 * 1024 * 1024 * 1024));
        assert_eq!(m.gpu_resident(), Some(104_367 * 1024 * 1024));
        let pct = m.percent().unwrap();
        assert!((pct - 94.1).abs() < 0.2, "pct was {pct}");
    }

    #[test]
    fn unavailable_memory_never_pretends_to_be_zero() {
        let m = GpuMemory::Unavailable;
        assert_eq!(m.used(), None);
        assert_eq!(m.total(), None);
        assert_eq!(m.percent(), None);
        assert_eq!(m.label(), "MEM");
    }

    /// Regression: with an always-`None` metric the old push/trim scheme
    /// desynchronised the series and panicked.
    #[test]
    fn history_push_with_unsupported_metrics_stays_aligned() {
        let mut h = GpuHistory::new(4);
        for _ in 0..10 {
            h.push(&GpuMetrics {
                timestamp: Instant::now(),
                utilization_gpu: Some(95.0),
                utilization_memory: None,
                memory: GpuMemory::Unavailable,
                temperature: Some(67.0),
                power_watts: Some(38.31),
                power_limit_watts: None, // GB10: unsupported
                sm_clock: Some(2502),
                mem_clock: None, // GB10: unsupported
                fan_speed: None, // GB10: unsupported
                pcie_tx_kbs: None,
                pcie_rx_kbs: None,
            });
        }
        assert_eq!(h.utilization.len(), 4);
        assert_eq!(h.memory.len(), 4);
        assert_eq!(h.temperature.len(), 4);
        assert_eq!(h.power.len(), 4);
        assert!(h.memory.is_all_missing());
        assert_eq!(h.utilization.last(), Some(95.0));
    }
}
