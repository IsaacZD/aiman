//! Hardware information data types.
//!
//! These types represent hardware state and are shared between
//! the agent (which collects the data) and the dashboard (which displays it).

use serde::{Deserialize, Serialize};

/// Snapshot of host hardware state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub hostname: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub cpu_brand: Option<String>,
    pub cpu_cores_logical: usize,
    pub cpu_cores_physical: Option<usize>,
    pub cpu_frequency_mhz: Option<u64>,
    pub memory_total_kb: u64,
    pub memory_available_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
    pub uptime_seconds: u64,
    pub gpus: Vec<GpuInfo>,
}

/// GPU device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: Option<String>,
    pub vendor: Option<String>,
    pub memory_total_mb: Option<u64>,
    pub memory_used_mb: Option<u64>,
    pub driver_version: Option<String>,
    pub utilization_percent: Option<u32>,
    pub temperature_celsius: Option<u32>,
    pub power_usage_watts: Option<f64>,
}
