use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub uuid: String,
    pub total_memory: u64,
    pub total_power: Option<f32>,
    pub total_energy: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GpuMetrics {
    pub timestamp: Instant,
    pub utilization_gpu: Option<f32>,
    pub utilization_memory: Option<f32>,
    pub used_memory: u64,
    pub free_memory: u64,
    pub temperature: Option<f32>,
    pub power: Option<f32>,
    pub power_limit: Option<f32>,
    pub sm_clock: Option<u32>,
    pub mem_clock: Option<u32>,
    pub fan_speed: Option<u32>,
    pub pcie_tx: Option<u64>,
    pub pcie_rx: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuProcess {
    pub pid: u32,
    pub name: String,
    pub used_memory: u64,
    pub engine: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GpuHistory {
    pub utilization: Vec<f32>,
    pub memory: Vec<f32>,
    pub temperature: Vec<f32>,
    pub power: Vec<f32>,
    pub max_points: usize,
}

impl GpuHistory {
    pub fn new(max_points: usize) -> Self {
        Self {
            utilization: Vec::with_capacity(max_points),
            memory: Vec::with_capacity(max_points),
            temperature: Vec::with_capacity(max_points),
            power: Vec::with_capacity(max_points),
            max_points,
        }
    }

    pub fn push(&mut self, metrics: &GpuMetrics) {
        if let Some(v) = metrics.utilization_gpu {
            self.utilization.push(v);
        }
        if let Some(v) = metrics.utilization_memory {
            self.memory.push(v);
        }
        if let Some(v) = metrics.temperature {
            self.temperature.push(v);
        }
        if let Some(v) = metrics.power {
            self.power.push(v);
        }

        // Trim to max_points
        if self.utilization.len() > self.max_points {
            let excess = self.utilization.len() - self.max_points;
            self.utilization.drain(0..excess);
            self.memory.drain(0..excess);
            self.temperature.drain(0..excess);
            self.power.drain(0..excess);
        }
    }
}