use crate::model::{CheckResult, GpuMetric, Status, Thresholds, BYTES_PER_GIB};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn check(name: &str, status: Status, message: &str, values: Value) -> CheckResult {
    CheckResult {
        name: name.to_string(),
        status,
        message: message.to_string(),
        values,
    }
}

fn threshold_status(value: f64, warning: f64, critical: f64) -> Status {
    if value >= critical {
        Status::Critical
    } else if value >= warning {
        Status::Warning
    } else {
        Status::Ok
    }
}

pub fn evaluate_cpu(
    busy_percent: f64,
    loadavg: &BTreeMap<String, f64>,
    cpu_count: usize,
    pressure: &BTreeMap<String, BTreeMap<String, BTreeMap<String, f64>>>,
    thresholds: &Thresholds,
) -> Vec<CheckResult> {
    let load1 = *loadavg.get("load1").unwrap_or(&0.0);
    let load5 = *loadavg.get("load5").unwrap_or(&0.0);
    let load15 = *loadavg.get("load15").unwrap_or(&0.0);
    let load_per_cpu = load1 / cpu_count.max(1) as f64;
    let cpu_pressure = pressure
        .get("cpu")
        .and_then(|value| value.get("some"))
        .and_then(|value| value.get("avg60"))
        .copied()
        .unwrap_or(0.0);

    vec![
        check(
            "cpu_busy",
            threshold_status(
                busy_percent,
                thresholds.cpu_busy_warning,
                thresholds.cpu_busy_critical,
            ),
            "cpu busy percent",
            json!({"busy_percent": busy_percent}),
        ),
        check(
            "cpu_load",
            threshold_status(
                load_per_cpu,
                thresholds.load_per_cpu_warning,
                thresholds.load_per_cpu_critical,
            ),
            "load average normalized by cpu count",
            json!({
                "load1": load1,
                "load5": load5,
                "load15": load15,
                "cpu_count": cpu_count,
                "load1_per_cpu": load_per_cpu,
            }),
        ),
        check(
            "cpu_psi",
            threshold_status(
                cpu_pressure,
                thresholds.psi_avg60_warning,
                thresholds.psi_avg60_critical,
            ),
            "cpu pressure stall information avg60",
            json!({"cpu_some_avg60": cpu_pressure}),
        ),
    ]
}

pub fn evaluate_memory(
    meminfo: &BTreeMap<String, u64>,
    thresholds: &Thresholds,
) -> Vec<CheckResult> {
    let total = *meminfo.get("MemTotal").unwrap_or(&0);
    let available = *meminfo
        .get("MemAvailable")
        .or_else(|| meminfo.get("MemFree"))
        .unwrap_or(&0);
    let total_gib = total as f64 / BYTES_PER_GIB;
    let available_gib = available as f64 / BYTES_PER_GIB;
    let available_percent = if total == 0 {
        0.0
    } else {
        available as f64 / total as f64 * 100.0
    };
    let use_absolute_floor = total_gib >= thresholds.mem_absolute_floor_min_total_gib;
    let status = if available_percent <= thresholds.mem_available_critical_percent
        || (use_absolute_floor && available_gib <= thresholds.mem_available_critical_gib)
    {
        Status::Critical
    } else if available_percent <= thresholds.mem_available_warning_percent
        || (use_absolute_floor && available_gib <= thresholds.mem_available_warning_gib)
    {
        Status::Warning
    } else {
        Status::Ok
    };
    vec![check(
        "memory_available",
        status,
        "memory available capacity",
        json!({
            "total_gib": total_gib,
            "available_gib": available_gib,
            "available_percent": available_percent,
        }),
    )]
}

pub fn evaluate_disk_usage(usages: &[Value], thresholds: &Thresholds) -> Vec<CheckResult> {
    usages
        .iter()
        .map(|usage| {
            let used_percent = usage["used_percent"].as_f64().unwrap_or(0.0);
            check(
                "disk_usage",
                threshold_status(
                    used_percent,
                    thresholds.disk_usage_warning,
                    thresholds.disk_usage_critical,
                ),
                "filesystem usage percent",
                usage.clone(),
            )
        })
        .collect()
}

pub fn evaluate_disk_io(
    rates: &BTreeMap<String, Value>,
    thresholds: &Thresholds,
    ebs_iops_limit: f64,
    ebs_throughput_mib_limit: f64,
) -> Vec<CheckResult> {
    rates
        .iter()
        .map(|(device, value)| {
            let iops = value["total_iops"].as_f64().unwrap_or(0.0);
            let throughput = value["total_mib_per_second"].as_f64().unwrap_or(0.0);
            let iops_ratio = if ebs_iops_limit > 0.0 {
                iops / ebs_iops_limit
            } else {
                0.0
            };
            let throughput_ratio = if ebs_throughput_mib_limit > 0.0 {
                throughput / ebs_throughput_mib_limit
            } else {
                0.0
            };
            let ratio = iops_ratio.max(throughput_ratio);
            let status = threshold_status(
                ratio,
                thresholds.io_limit_warning_ratio,
                thresholds.io_limit_critical_ratio,
            );
            check(
                "disk_io",
                status,
                "block device io rate",
                json!({
                    "device": device,
                    "total_iops": iops,
                    "total_mib_per_second": throughput,
                    "busy_percent": value["busy_percent"].as_f64().unwrap_or(0.0),
                    "iops_limit": ebs_iops_limit,
                    "throughput_mib_limit": ebs_throughput_mib_limit,
                }),
            )
        })
        .collect()
}

pub fn evaluate_network(rates: &BTreeMap<String, Value>) -> Vec<CheckResult> {
    rates
        .iter()
        .map(|(interface, value)| {
            let error_delta = value["rx_errors_delta"].as_u64().unwrap_or(0)
                + value["tx_errors_delta"].as_u64().unwrap_or(0)
                + value["rx_drops_delta"].as_u64().unwrap_or(0)
                + value["tx_drops_delta"].as_u64().unwrap_or(0);
            let status = if error_delta > 0 {
                Status::Warning
            } else {
                Status::Ok
            };
            let mut values = value.clone();
            values["interface"] = json!(interface);
            check(
                "network_errors",
                status,
                "network errors and drops delta",
                values,
            )
        })
        .collect()
}

pub fn evaluate_gpu(samples: &[Vec<GpuMetric>], thresholds: &Thresholds) -> Vec<CheckResult> {
    let Some(latest) = samples.last() else {
        return Vec::new();
    };
    let mut checks = Vec::new();
    for metric in latest {
        let memory_used_percent = metric.memory_used_percent();
        checks.push(check(
            "gpu_memory",
            threshold_status(
                memory_used_percent,
                thresholds.gpu_memory_warning,
                thresholds.gpu_memory_critical,
            ),
            "gpu memory usage percent",
            json!({
                "gpu_index": metric.index,
                "gpu_name": metric.name,
                "memory_total_mib": metric.memory_total_mib,
                "memory_used_mib": metric.memory_used_mib,
                "memory_used_percent": memory_used_percent,
            }),
        ));
        checks.push(check(
            "gpu_utilization",
            threshold_status(
                metric.utilization_gpu_percent,
                thresholds.gpu_util_warning,
                thresholds.gpu_util_critical,
            ),
            "gpu utilization percent",
            json!({
                "gpu_index": metric.index,
                "utilization_gpu_percent": metric.utilization_gpu_percent,
                "utilization_memory_percent": metric.utilization_memory_percent,
            }),
        ));
        checks.push(check(
            "gpu_temperature",
            threshold_status(
                metric.temperature_c,
                thresholds.gpu_temp_warning,
                thresholds.gpu_temp_critical,
            ),
            "gpu temperature celsius",
            json!({
                "gpu_index": metric.index,
                "temperature_c": metric.temperature_c,
            }),
        ));

        let idle = samples.iter().all(|sample| {
            sample.iter().any(|item| {
                item.index == metric.index
                    && item.memory_used_percent() >= thresholds.gpu_idle_memory_percent
                    && item.utilization_gpu_percent < thresholds.gpu_idle_util_percent
            })
        });
        checks.push(check(
            "gpu_memory_idle",
            if idle { Status::Warning } else { Status::Ok },
            "gpu memory is high while gpu utilization is low",
            json!({
                "gpu_index": metric.index,
                "samples": samples.len(),
                "memory_used_percent": memory_used_percent,
                "utilization_gpu_percent": metric.utilization_gpu_percent,
            }),
        ));
    }
    checks
}

pub fn worst_status(checks: &[CheckResult]) -> Status {
    checks
        .iter()
        .map(|check| check.status)
        .max()
        .unwrap_or(Status::Ok)
}
