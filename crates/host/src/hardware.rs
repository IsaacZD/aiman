use serde::{Deserialize, Serialize};
use std::process::Command;
use sysinfo::System;
use tokio::task;

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuInfo {
    pub name: Option<String>,
    pub vendor: Option<String>,
    pub memory_total_mb: Option<u64>,
    pub driver_version: Option<String>,
}

pub async fn collect_hardware_info() -> HardwareInfo {
    let mut system = System::new_all();
    system.refresh_all();
    let gpus = task::spawn_blocking(collect_gpus).await.unwrap_or_default();

    let cpu = system.cpus().first();
    let cpu_brand = cpu
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|brand| !brand.is_empty());
    let cpu_frequency_mhz = cpu
        .map(|cpu| cpu.frequency())
        .filter(|freq| *freq > 0)
        .map(u64::from);

    HardwareInfo {
        hostname: System::host_name(),
        os_name: System::name(),
        os_version: System::os_version(),
        kernel_version: System::kernel_version(),
        cpu_brand,
        cpu_cores_logical: system.cpus().len(),
        cpu_cores_physical: system.physical_core_count(),
        cpu_frequency_mhz,
        memory_total_kb: system.total_memory(),
        memory_available_kb: system.available_memory(),
        swap_total_kb: system.total_swap(),
        swap_free_kb: system.free_swap(),
        uptime_seconds: System::uptime(),
        gpus,
    }
}

fn collect_gpus() -> Vec<GpuInfo> {
    if let Some(gpus) = collect_nvidia_gpus() {
        return gpus;
    }

    if let Some(gpus) = collect_lspci_gpus() {
        return gpus;
    }

    Vec::new()
}

fn collect_nvidia_gpus() -> Option<Vec<GpuInfo>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();
    for line in text.lines().map(|line| line.trim()).filter(|line| !line.is_empty()) {
        let mut parts = line.split(',').map(|part| part.trim());
        let name = parts.next().map(|value| value.to_string()).filter(|v| !v.is_empty());
        let memory_total_mb = parts.next().and_then(|value| value.parse::<u64>().ok());
        let driver_version =
            parts.next().map(|value| value.to_string()).filter(|v| !v.is_empty());

        gpus.push(GpuInfo {
            name,
            vendor: Some("NVIDIA".to_string()),
            memory_total_mb,
            driver_version,
        });
    }

    Some(gpus)
}

fn collect_lspci_gpus() -> Option<Vec<GpuInfo>> {
    let output = Command::new("lspci").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();
    for line in text.lines() {
        if !(line.contains("VGA compatible controller")
            || line.contains("3D controller")
            || line.contains("Display controller"))
        {
            continue;
        }

        let name = line
            .splitn(2, ':')
            .nth(1)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if name.is_none() {
            continue;
        }

        gpus.push(GpuInfo {
            name,
            vendor: None,
            memory_total_mb: None,
            driver_version: None,
        });
    }

    Some(gpus)
}
