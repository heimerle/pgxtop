use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub name: String,
    pub uptime: u64,
    pub cpu_count: usize,
    pub total_memory: u64,
    pub total_swap: u64,
}

#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub timestamp: Instant,
    pub cpu_usage: f32,
    pub per_core_usage: Vec<f32>,
    pub used_memory: u64,
    pub used_swap: u64,
    pub load_avg: [f32; 3],
    pub disk_io: DiskIo,
    pub network_io: NetworkIo,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiskIo {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkIo {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

#[derive(Debug, Clone)]
pub struct SystemHistory {
    pub cpu: Vec<f32>,
    pub memory: Vec<f32>,
    pub max_points: usize,
}

impl SystemHistory {
    pub fn new(max_points: usize) -> Self {
        Self {
            cpu: Vec::with_capacity(max_points),
            memory: Vec::with_capacity(max_points),
            max_points,
        }
    }

    pub fn push(&mut self, metrics: &SystemMetrics) {
        self.cpu.push(metrics.cpu_usage);
        self.memory.push(metrics.used_memory as f32 / 1024.0 / 1024.0 / 1024.0);

        if self.cpu.len() > self.max_points {
            let excess = self.cpu.len() - self.max_points;
            self.cpu.drain(0..excess);
            self.memory.drain(0..excess);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub gpu_memory: Option<u64>,
    pub gpu_index: Option<u32>,
}