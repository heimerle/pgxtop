pub mod correlate;
pub mod inference;
pub mod nvml;
pub mod system;

pub struct Collectors {
    pub nvml: nvml::NvmlCollector,
    pub system: system::SystemCollector,
}

impl Collectors {
    pub fn new() -> Self {
        Self {
            nvml: nvml::NvmlCollector::new(),
            system: system::SystemCollector::new(),
        }
    }
}
