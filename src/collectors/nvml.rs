use std::time::Instant;

use crate::models::{GpuInfo, GpuMetrics, GpuProcess};

pub struct NvmlCollector {
    nvml: Option<nvml_wrapper::Nvml>,
}

impl NvmlCollector {
    pub fn new() -> Self {
        let nvml = nvml_wrapper::Nvml::init().ok();
        Self { nvml }
    }

    pub fn collect(&self) -> Vec<(GpuInfo, GpuMetrics, Vec<GpuProcess>)> {
        let mut results = Vec::new();

        let nvml = match &self.nvml {
            Some(n) => n,
            None => return results,
        };

        let device_count = match nvml.device_count() {
            Ok(count) => count,
            Err(_) => return results,
        };

        for i in 0..device_count {
            if let Ok(device) = nvml.device_by_index(i) {
                if let Some(info) = self.get_gpu_info(&device, i) {
                    let metrics = self.get_gpu_metrics(&device);
                    let processes = self.get_gpu_processes(&device);
                    results.push((info, metrics, processes));
                }
            }
        }

        results
    }

    fn get_gpu_info(&self, device: &nvml_wrapper::Device, index: u32) -> Option<GpuInfo> {
        let name = device.name().ok()?;
        let uuid = device.uuid().ok()?;
        let total_memory = device.memory_info().ok().map(|m| m.total).unwrap_or(0);
        let total_power = device.enforced_power_limit().ok().map(|p| p as f32);
        let total_energy = device.total_energy_consumption().ok().map(|e| e as f64);

        Some(GpuInfo {
            index,
            name,
            uuid,
            total_memory,
            total_power,
            total_energy,
        })
    }

    fn get_gpu_metrics(&self, device: &nvml_wrapper::Device) -> GpuMetrics {
        let utilization = device.utilization_rates().ok();
        let utilization_gpu = utilization.as_ref().map(|u| u.gpu as f32);
        let utilization_memory = utilization.as_ref().map(|u| u.memory as f32);
        let memory_info = device.memory_info().ok();
        let used_memory = memory_info.as_ref().map(|m| m.used).unwrap_or(0);
        let free_memory = memory_info.as_ref().map(|m| m.free).unwrap_or(0);
        let temperature = device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .ok()
            .map(|t| t as f32);
        let power = device.power_usage().ok().map(|p| p as f32);
        let power_limit = device.enforced_power_limit().ok().map(|p| p as f32);
        let sm_clock = device
            .clock_info(nvml_wrapper::enum_wrappers::device::Clock::SM)
            .ok();
        let mem_clock = device
            .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Memory)
            .ok();
        let fan_speed = device.fan_speed(0).ok();
        let pcie_tx = device
            .pcie_throughput(nvml_wrapper::enum_wrappers::device::PcieUtilCounter::Send)
            .ok();
        let pcie_rx = device
            .pcie_throughput(nvml_wrapper::enum_wrappers::device::PcieUtilCounter::Receive)
            .ok();

        GpuMetrics {
            timestamp: Instant::now(),
            utilization_gpu,
            utilization_memory,
            used_memory,
            free_memory,
            temperature,
            power,
            power_limit,
            sm_clock,
            mem_clock,
            fan_speed,
            pcie_tx,
            pcie_rx,
        }
    }

    fn get_gpu_processes(&self, device: &nvml_wrapper::Device) -> Vec<GpuProcess> {
        let mut processes = Vec::new();

        if let Ok(procs) = device.running_compute_processes() {
            for proc in procs {
                let pid = proc.pid;
                let used_memory = match proc.used_gpu_memory {
                    nvml_wrapper::enums::device::UsedGpuMemory::Used(bytes) => bytes,
                    nvml_wrapper::enums::device::UsedGpuMemory::Unavailable => 0,
                };

                // Try to get process name
                let name = std::fs::read_to_string(format!("/proc/{}/comm", pid))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| format!("PID {}", pid));

                processes.push(GpuProcess {
                    pid,
                    name,
                    used_memory,
                    engine: None,
                    model: None,
                });
            }
        }

        processes
    }
}