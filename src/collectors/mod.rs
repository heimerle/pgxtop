pub mod inference;
pub mod nvml;
pub mod system;

use crate::models::{GpuInfo, GpuMetrics, GpuProcess, SystemInfo, SystemMetrics, ProcessInfo};
use crate::engines::{InferenceEngine, ModelInstance, InferenceMetrics};

pub struct Collectors {
    pub nvml: nvml::NvmlCollector,
    pub system: system::SystemCollector,
    pub inference: inference::InferenceCollector,
}

impl Collectors {
    pub fn new() -> Self {
        Self {
            nvml: nvml::NvmlCollector::new(),
            system: system::SystemCollector::new(),
            inference: inference::InferenceCollector::new(),
        }
    }
}