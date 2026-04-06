use std::pin::Pin;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use sysinfo::System;
use tokio::time::timeout;
use process_wrap::tokio::*;

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
    pub memory_used_mb: Option<u64>,
    pub driver_version: Option<String>,
    pub utilization_percent: Option<u32>,
    pub temperature_celsius: Option<u32>,
    pub power_usage_watts: Option<f64>,
}

#[derive(Debug)]
pub struct HardwareCache {
    ttl: Duration,
    last_fetched: Option<Instant>,
    last_value: Option<HardwareInfo>,
    gpu_timeout: Duration,
    skip_gpu: bool,
}

impl HardwareCache {
    pub fn new(ttl: Duration, gpu_timeout: Duration, skip_gpu: bool) -> Self {
        Self {
            ttl,
            last_fetched: None,
            last_value: None,
            gpu_timeout,
            skip_gpu,
        }
    }

    pub fn from_env() -> Self {
        let ttl_secs = std::env::var("AIMAN_HARDWARE_TTL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10);
        let gpu_timeout_secs = std::env::var("AIMAN_HARDWARE_GPU_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(2);
        let skip_gpu = std::env::var("AIMAN_HARDWARE_SKIP_GPU")
            .ok()
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        Self::new(
            Duration::from_secs(ttl_secs),
            Duration::from_secs(gpu_timeout_secs),
            skip_gpu,
        )
    }

    pub async fn get(&mut self) -> HardwareInfo {
        if self.ttl.as_secs() > 0 {
            if let (Some(last), Some(value)) = (self.last_fetched, self.last_value.clone()) {
                if last.elapsed() < self.ttl {
                    return value;
                }
            }
        }

        let value = collect_hardware_info(self.gpu_timeout, self.skip_gpu).await;
        self.last_fetched = Some(Instant::now());
        self.last_value = Some(value.clone());
        value
    }
}

pub async fn collect_hardware_info(
    gpu_timeout: Duration,
    skip_gpu: bool,
) -> HardwareInfo {
    let mut system = System::new();
    // Only refresh what we read to keep collection cheap on repeated polling.
    system.refresh_cpu();
    system.refresh_memory();
    let gpus = collect_gpus(gpu_timeout, skip_gpu).await;

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

async fn collect_gpus(gpu_timeout: Duration, skip_gpu: bool) -> Vec<GpuInfo> {
    if skip_gpu {
        return Vec::new();
    }

    // 1. NVML (preferred — richer runtime metrics, no subprocess overhead)
    let nvml_result = tokio::task::spawn_blocking(collect_nvidia_gpus_nvml).await.ok().flatten();
    if let Some(gpus) = nvml_result {
        return gpus;
    }

    // 2. nvidia-smi subprocess fallback
    if let Some(gpus) = collect_nvidia_gpus(gpu_timeout).await {
        return gpus;
    }

    // 3. lspci fallback for non-NVML environments
    if let Some(gpus) = collect_lspci_gpus(gpu_timeout).await {
        return gpus;
    }

    Vec::new()
}

fn collect_nvidia_gpus_nvml() -> Option<Vec<GpuInfo>> {
    use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
    use nvml_wrapper::Nvml;

    let nvml = Nvml::init().ok()?;
    let driver_version = nvml.sys_driver_version().ok();
    let device_count = nvml.device_count().ok()?;

    let mut gpus = Vec::new();
    for i in 0..device_count {
        let device = match nvml.device_by_index(i) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let name = device.name().ok();
        let memory_info = device.memory_info().ok();
        let memory_total_mb = memory_info.as_ref().map(|m| m.total / (1024 * 1024));
        let memory_used_mb = memory_info.as_ref().map(|m| m.used / (1024 * 1024));
        let utilization_percent = device.utilization_rates().ok().map(|u| u.gpu);
        let temperature_celsius = device.temperature(TemperatureSensor::Gpu).ok();
        let power_usage_watts = device.power_usage().ok().map(|mw| mw as f64 / 1000.0);

        gpus.push(GpuInfo {
            name,
            vendor: Some("NVIDIA".to_string()),
            memory_total_mb,
            memory_used_mb,
            driver_version: driver_version.clone(),
            utilization_percent,
            temperature_celsius,
            power_usage_watts,
        });
    }

    Some(gpus)
}

async fn collect_nvidia_gpus(gpu_timeout: Duration) -> Option<Vec<GpuInfo>> {
    let output = run_command_with_timeout(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ],
        gpu_timeout,
    )
    .await?;

    let text = String::from_utf8_lossy(&output);
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
            memory_used_mb: None,
            driver_version,
            utilization_percent: None,
            temperature_celsius: None,
            power_usage_watts: None,
        });
    }

    Some(gpus)
}

async fn collect_lspci_gpus(gpu_timeout: Duration) -> Option<Vec<GpuInfo>> {
    let output = run_command_with_timeout("lspci", &[], gpu_timeout).await?;
    let text = String::from_utf8_lossy(&output);
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
            memory_used_mb: None,
            driver_version: None,
            utilization_percent: None,
            temperature_celsius: None,
            power_usage_watts: None,
        });
    }

    Some(gpus)
}

async fn run_command_with_timeout(
    command: &str,
    args: &[&str],
    timeout_duration: Duration,
) -> Option<Vec<u8>> {
    let mut wrap = CommandWrap::with_new(command, |cmd| {
        cmd.args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
    });
    wrap.wrap(ProcessGroup::leader());
    // wait_with_output consumes the child, so a single timeout covers both
    // stdout collection and process exit. On timeout the future is dropped
    // which triggers process-group cleanup via the ProcessGroup wrapper.
    let result = timeout(timeout_duration, Pin::from(wrap.spawn().ok()?.wait_with_output())).await;

    let output = match result {
        Ok(Ok(output)) => output,
        _ => return None,
    };

    if !output.status.success() {
        return None;
    }

    Some(output.stdout)
}
