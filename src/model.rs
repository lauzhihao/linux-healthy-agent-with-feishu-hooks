use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::identity::MachineIdentity;

pub const BYTES_PER_KIB: f64 = 1024.0;
pub const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
pub const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;
pub const DISK_SECTOR_BYTES: f64 = 512.0;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warning,
    Critical,
}

impl Status {
    pub fn exit_code(self) -> i32 {
        match self {
            Status::Ok => 0,
            Status::Warning => 1,
            Status::Critical => 2,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub status: Status,
    pub message: String,
    pub values: serde_json::Value,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTimes {
    pub fn total(self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    pub fn idle_all(self) -> u64 {
        self.idle + self.iowait
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiskStat {
    pub name: String,
    pub read_ios: u64,
    pub read_sectors: u64,
    pub write_ios: u64,
    pub write_sectors: u64,
    pub io_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetworkStat {
    pub name: String,
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub rx_errors: u64,
    pub rx_drops: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
    pub tx_errors: u64,
    pub tx_drops: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GpuMetric {
    pub index: u32,
    pub name: String,
    pub uuid: String,
    pub memory_total_mib: f64,
    pub memory_used_mib: f64,
    pub utilization_gpu_percent: f64,
    pub utilization_memory_percent: f64,
    pub temperature_c: f64,
    pub power_draw_w: Option<f64>,
    pub power_limit_w: Option<f64>,
}

impl GpuMetric {
    pub fn memory_used_percent(&self) -> f64 {
        if self.memory_total_mib <= 0.0 {
            return 0.0;
        }
        self.memory_used_mib / self.memory_total_mib * 100.0
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct GpuProcess {
    pub gpu_uuid: String,
    pub pid: u32,
    pub process_name: String,
    pub used_memory_mib: f64,
    pub user: String,
    pub command: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeploymentMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

impl DeploymentMetadata {
    pub fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.cloud_region.is_none()
            && self.zone.is_none()
            && self.fleet_region.is_none()
            && self.role.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub cpu_busy_warning: f64,
    pub cpu_busy_critical: f64,
    pub load_per_cpu_warning: f64,
    pub load_per_cpu_critical: f64,
    pub psi_avg60_warning: f64,
    pub psi_avg60_critical: f64,
    pub mem_available_warning_percent: f64,
    pub mem_available_critical_percent: f64,
    pub mem_available_warning_gib: f64,
    pub mem_available_critical_gib: f64,
    pub mem_absolute_floor_min_total_gib: f64,
    pub disk_usage_warning: f64,
    pub disk_usage_critical: f64,
    pub io_limit_warning_ratio: f64,
    pub io_limit_critical_ratio: f64,
    pub gpu_memory_warning: f64,
    pub gpu_memory_critical: f64,
    pub gpu_util_warning: f64,
    pub gpu_util_critical: f64,
    pub gpu_temp_warning: f64,
    pub gpu_temp_critical: f64,
    pub gpu_idle_memory_percent: f64,
    pub gpu_idle_util_percent: f64,
    pub network_errors_warning_delta: u64,
    pub network_errors_critical_delta: u64,
    pub docker_unhealthy_warning_count: u64,
    pub docker_unhealthy_critical_count: u64,
    pub docker_restarting_warning_count: u64,
    pub docker_restarting_critical_count: u64,
    pub docker_exited_warning_count: u64,
    pub docker_exited_critical_count: u64,
    pub docker_other_abnormal_warning_count: u64,
    pub docker_other_abnormal_critical_count: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_busy_warning: 85.0,
            cpu_busy_critical: 95.0,
            load_per_cpu_warning: 1.0,
            load_per_cpu_critical: 1.5,
            psi_avg60_warning: 10.0,
            psi_avg60_critical: 20.0,
            mem_available_warning_percent: 10.0,
            mem_available_critical_percent: 5.0,
            mem_available_warning_gib: 64.0,
            mem_available_critical_gib: 32.0,
            mem_absolute_floor_min_total_gib: 256.0,
            disk_usage_warning: 80.0,
            disk_usage_critical: 90.0,
            io_limit_warning_ratio: 0.8,
            io_limit_critical_ratio: 0.9,
            gpu_memory_warning: 85.0,
            gpu_memory_critical: 95.0,
            gpu_util_warning: 90.0,
            gpu_util_critical: 98.0,
            gpu_temp_warning: 80.0,
            gpu_temp_critical: 90.0,
            gpu_idle_memory_percent: 80.0,
            gpu_idle_util_percent: 10.0,
            network_errors_warning_delta: 1,
            network_errors_critical_delta: 10,
            docker_unhealthy_warning_count: 1,
            docker_unhealthy_critical_count: 1,
            docker_restarting_warning_count: 1,
            docker_restarting_critical_count: 1,
            docker_exited_warning_count: 1,
            docker_exited_critical_count: 10,
            docker_other_abnormal_warning_count: 1,
            docker_other_abnormal_critical_count: 10,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    pub schema_version: u32,
    pub timestamp_unix: u64,
    pub hostname: String,
    pub identity: MachineIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<DeploymentMetadata>,
    pub elapsed_seconds: f64,
    pub metrics: serde_json::Value,
    pub errors: BTreeMap<String, String>,
}
