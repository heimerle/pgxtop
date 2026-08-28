use std::time::Instant;

use crate::models::{SystemInfo, SystemMetrics, ProcessInfo};

pub struct SystemCollector {
    sys: sysinfo::System,
    prev_net_io: Option<(u64, u64)>,
}

impl SystemCollector {
    pub fn new() -> Self {
        Self {
            sys: sysinfo::System::new_all(),
            prev_net_io: None,
        }
    }

    pub fn collect(&mut self) -> (SystemInfo, SystemMetrics, Vec<ProcessInfo>) {
        self.sys.refresh_all();

        let info = self.get_system_info();
        let metrics = self.get_system_metrics();
        let processes = self.get_processes();

        (info, metrics, processes)
    }

    fn get_system_info(&self) -> SystemInfo {
        SystemInfo {
            name: sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string()),
            uptime: sysinfo::System::uptime(),
            cpu_count: self.sys.cpus().len(),
            total_memory: self.sys.total_memory(),
            total_swap: self.sys.total_swap(),
        }
    }

    fn get_system_metrics(&mut self) -> SystemMetrics {
        let cpu_usage = self.sys.global_cpu_usage();
        let per_core_usage: Vec<f32> = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();

        let load_avg = sysinfo::System::load_average();

        SystemMetrics {
            timestamp: Instant::now(),
            cpu_usage,
            per_core_usage,
            used_memory: self.sys.used_memory(),
            used_swap: self.sys.used_swap(),
            load_avg: [load_avg.one as f32, load_avg.five as f32, load_avg.fifteen as f32],
            disk_io: Default::default(),
            network_io: Default::default(),
        }
    }

    fn get_processes(&self) -> Vec<ProcessInfo> {
        self.sys.processes().iter().map(|(pid, proc)| {
            ProcessInfo {
                pid: pid.as_u32(),
                name: proc.name().to_string_lossy().to_string(),
                cpu_usage: proc.cpu_usage(),
                memory_usage: proc.memory(),
                gpu_memory: None,
                gpu_index: None,
            }
        }).collect()
    }
}