use std::time::Instant;

use crate::models::{GpuInfo, GpuMemory, GpuMetrics, GpuProcess, MappingConfidence};

/// Raw NVML output for one device, before unified-memory resolution.
pub struct DeviceSample {
    pub info: GpuInfo,
    pub metrics: GpuMetrics,
    pub processes: Vec<GpuProcess>,
}

pub struct NvmlCollector {
    nvml: Option<nvml_wrapper::Nvml>,
    /// Why NVML is unavailable, so the UI can say more than "no GPU".
    init_error: Option<String>,
}

impl NvmlCollector {
    pub fn new() -> Self {
        match nvml_wrapper::Nvml::init() {
            Ok(nvml) => Self { nvml: Some(nvml), init_error: None },
            Err(e) => {
                tracing::info!(target: "pgxtop::nvml", "NVML unavailable: {e}");
                Self { nvml: None, init_error: Some(e.to_string()) }
            }
        }
    }

    pub fn init_error(&self) -> Option<&str> {
        self.init_error.as_deref()
    }

    pub fn collect(&self) -> Vec<DeviceSample> {
        let mut results = Vec::new();

        let Some(nvml) = &self.nvml else {
            return results;
        };
        let Ok(device_count) = nvml.device_count() else {
            return results;
        };

        for i in 0..device_count {
            if let Ok(device) = nvml.device_by_index(i) {
                let processes = self.get_gpu_processes(&device, i);
                let info = self.get_gpu_info(&device, i);
                let metrics = self.get_gpu_metrics(&device);
                results.push(DeviceSample { info, metrics, processes });
            }
        }

        results
    }

    fn get_gpu_info(&self, device: &nvml_wrapper::Device, index: u32) -> GpuInfo {
        // Previously `name()`/`uuid()` used `?`, which dropped the entire
        // device — metrics and processes included — when either failed.
        GpuInfo {
            index,
            name: device.name().unwrap_or_else(|_| format!("GPU {index}")),
            uuid: device.uuid().unwrap_or_default(),
            memory_total: device.memory_info().ok().map(|m| m.total),
            power_limit_watts: device.enforced_power_limit().ok().map(mw_to_w),
            total_energy: device.total_energy_consumption().ok().map(|e| e as f64),
        }
    }

    fn get_gpu_metrics(&self, device: &nvml_wrapper::Device) -> GpuMetrics {
        let utilization = device.utilization_rates().ok();

        // On Grace-Blackwell (GB10) this query is unsupported: nvidia-smi
        // reports N/A for every FB memory field because the GPU shares the
        // host's unified memory. The old code turned that into a hard 0 via
        // `unwrap_or(0)`, so pgxtop displayed "0/0 GB" on the target hardware.
        // `Unavailable` here is upgraded to `GpuMemory::Unified` by
        // `resolve_memory` once host memory figures are available.
        let memory = match device.memory_info() {
            Ok(m) => GpuMemory::Dedicated { used: m.used, total: m.total },
            Err(_) => GpuMemory::Unavailable,
        };

        GpuMetrics {
            timestamp: Instant::now(),
            utilization_gpu: utilization.as_ref().map(|u| u.gpu as f32),
            utilization_memory: utilization.as_ref().map(|u| u.memory as f32),
            memory,
            temperature: device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .ok()
                .map(|t| t as f32),
            // NVML reports milliwatts. Printing these raw as watts is why a
            // 38 W draw rendered as "38310 W".
            power_watts: device.power_usage().ok().map(mw_to_w),
            power_limit_watts: device.enforced_power_limit().ok().map(mw_to_w),
            sm_clock: device
                .clock_info(nvml_wrapper::enum_wrappers::device::Clock::SM)
                .ok(),
            mem_clock: device
                .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Memory)
                .ok(),
            fan_speed: device.fan_speed(0).ok(),
            pcie_tx_kbs: device
                .pcie_throughput(nvml_wrapper::enum_wrappers::device::PcieUtilCounter::Send)
                .ok(),
            pcie_rx_kbs: device
                .pcie_throughput(nvml_wrapper::enum_wrappers::device::PcieUtilCounter::Receive)
                .ok(),
        }
    }

    fn get_gpu_processes(&self, device: &nvml_wrapper::Device, gpu_index: u32) -> Vec<GpuProcess> {
        let mut processes = Vec::new();

        let add = |procs: Vec<nvml_wrapper::struct_wrappers::device::ProcessInfo>,
                       graphics: bool,
                       out: &mut Vec<GpuProcess>| {
            for proc in procs {
                let pid = proc.pid;
                // `Unavailable` is an unknown, not a zero — the old code
                // coerced it to 0, making an unattributable process look empty.
                let used_memory = match proc.used_gpu_memory {
                    nvml_wrapper::enums::device::UsedGpuMemory::Used(bytes) => Some(bytes),
                    nvml_wrapper::enums::device::UsedGpuMemory::Unavailable => None,
                };

                out.push(GpuProcess {
                    pid,
                    name: read_comm(pid).unwrap_or_else(|| format!("PID {pid}")),
                    cmdline: read_cmdline(pid),
                    gpu_index,
                    used_memory,
                    graphics,
                    engine: None,
                    model: None,
                    confidence: MappingConfidence::Unknown,
                });
            }
        };

        if let Ok(procs) = device.running_compute_processes() {
            add(procs, false, &mut processes);
        }
        // Without this, VRAM held by a graphics context is invisible, which is
        // a common reason the per-process sum does not add up to `used`.
        if let Ok(procs) = device.running_graphics_processes() {
            let known: Vec<u32> = processes.iter().map(|p| p.pid).collect();
            let fresh: Vec<_> = procs.into_iter().filter(|p| !known.contains(&p.pid)).collect();
            add(fresh, true, &mut processes);
        }

        processes
    }
}

fn mw_to_w(mw: u32) -> f32 {
    mw as f32 / 1000.0
}

/// `/proc/<pid>/comm` — short name, truncated to 15 characters by the kernel.
fn read_comm(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `/proc/<pid>/cmdline` — NUL-separated argv.
///
/// This, not `/proc/<pid>/exe`, is what identifies the engine: the runner
/// typically belongs to another user (`ollama`), and reading the `exe`
/// symlink of another user's process requires ptrace rights we do not have,
/// while `cmdline` is world-readable.
fn read_cmdline(pid: u32) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if raw.is_empty() {
        return None;
    }
    let s: String = String::from_utf8_lossy(&raw)
        .split('\0')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Upgrades `GpuMemory::Unavailable` to `Unified` on hardware where NVML has
/// no frame buffer to report but the GPU shares host memory.
///
/// `gpu_resident` is the sum of what NVML *does* still report per process —
/// on a GB10 that works even though the device-level query does not.
pub fn resolve_memory(
    metrics: &mut GpuMetrics,
    processes: &[GpuProcess],
    host_used: u64,
    host_total: u64,
) {
    if !matches!(metrics.memory, GpuMemory::Unavailable) {
        return;
    }
    if host_total == 0 {
        return;
    }
    let gpu_resident = processes
        .iter()
        .filter_map(|p| p.used_memory)
        .fold(0u64, |a, b| a.saturating_add(b));

    metrics.memory = GpuMemory::Unified { host_used, host_total, gpu_resident };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, mem: Option<u64>) -> GpuProcess {
        GpuProcess {
            pid,
            name: "llama-server".into(),
            cmdline: None,
            gpu_index: 0,
            used_memory: mem,
            graphics: false,
            engine: None,
            model: None,
            confidence: MappingConfidence::Unknown,
        }
    }

    fn metrics(memory: GpuMemory) -> GpuMetrics {
        GpuMetrics {
            timestamp: Instant::now(),
            utilization_gpu: Some(95.0),
            utilization_memory: Some(0.0),
            memory,
            temperature: Some(67.0),
            power_watts: Some(38.31),
            power_limit_watts: None,
            sm_clock: Some(2502),
            mem_clock: None,
            fan_speed: None,
            pcie_tx_kbs: None,
            pcie_rx_kbs: None,
        }
    }

    #[test]
    fn milliwatts_become_watts() {
        // The value measured on the PGX: 38310 mW.
        assert!((mw_to_w(38_310) - 38.31).abs() < 0.001);
    }

    /// The GB10 case: NVML answers nothing about memory, so we fall back to
    /// host memory and the per-process figures that *do* work.
    #[test]
    fn unavailable_memory_upgrades_to_unified() {
        let mut m = metrics(GpuMemory::Unavailable);
        let procs = vec![
            proc(245034, Some(104_367 * 1024 * 1024)),
            proc(3449, Some(170 * 1024 * 1024)),
        ];
        let host_total = 119 * 1024 * 1024 * 1024;
        let host_used = 112 * 1024 * 1024 * 1024;
        resolve_memory(&mut m, &procs, host_used, host_total);

        match m.memory {
            GpuMemory::Unified { host_used: u, host_total: t, gpu_resident } => {
                assert_eq!(u, host_used);
                assert_eq!(t, host_total);
                assert_eq!(gpu_resident, (104_367 + 170) * 1024 * 1024);
            }
            other => panic!("expected Unified, got {other:?}"),
        }
    }

    #[test]
    fn dedicated_memory_is_left_alone() {
        let mut m = metrics(GpuMemory::Dedicated { used: 10, total: 100 });
        resolve_memory(&mut m, &[], 1, 2);
        assert_eq!(m.memory, GpuMemory::Dedicated { used: 10, total: 100 });
    }

    /// Without host figures there is nothing honest to show, so it stays
    /// Unavailable rather than becoming a fabricated 0.
    #[test]
    fn no_host_memory_leaves_it_unavailable() {
        let mut m = metrics(GpuMemory::Unavailable);
        resolve_memory(&mut m, &[proc(1, Some(100))], 0, 0);
        assert_eq!(m.memory, GpuMemory::Unavailable);
    }

    #[test]
    fn processes_with_unknown_memory_do_not_count_as_zero() {
        let mut m = metrics(GpuMemory::Unavailable);
        let procs = vec![proc(1, None), proc(2, Some(1024))];
        resolve_memory(&mut m, &procs, 10, 100);
        assert_eq!(m.memory.gpu_resident(), Some(1024));
    }
}
