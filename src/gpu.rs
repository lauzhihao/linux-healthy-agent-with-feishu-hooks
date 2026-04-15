use crate::model::{GpuMetric, GpuProcess};
use crate::procfs::process_identity;
use serde::Serialize;
use std::fmt;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuQueryErrorKind {
    CommandMissing,
    NoDevices,
    CommandTimeout,
    CommandFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GpuQueryError {
    pub kind: GpuQueryErrorKind,
    pub message: String,
}

impl fmt::Display for GpuQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for GpuQueryError {}

fn run_command(args: &[&str], timeout: Duration) -> io::Result<String> {
    let mut child = Command::new(args[0])
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    match child.wait_timeout(timeout)? {
        Some(status) => {
            let mut stdout = String::new();
            if let Some(mut pipe) = child.stdout.take() {
                pipe.read_to_string(&mut stdout)?;
            }
            if status.success() {
                return Ok(stdout);
            }
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr)?;
            }
            Err(io::Error::other(stderr.trim().to_string()))
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            Err(io::Error::new(io::ErrorKind::TimedOut, "command timeout"))
        }
    }
}

fn classify_gpu_error(error: io::Error) -> GpuQueryError {
    if error.kind() == io::ErrorKind::NotFound {
        return GpuQueryError {
            kind: GpuQueryErrorKind::CommandMissing,
            message: error.to_string(),
        };
    }
    if error.kind() == io::ErrorKind::TimedOut {
        return GpuQueryError {
            kind: GpuQueryErrorKind::CommandTimeout,
            message: error.to_string(),
        };
    }
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("no devices were found")
        || lower.contains("no devices found")
        || lower.contains("no nvidia")
    {
        return GpuQueryError {
            kind: GpuQueryErrorKind::NoDevices,
            message,
        };
    }
    GpuQueryError {
        kind: GpuQueryErrorKind::CommandFailed,
        message,
    }
}

fn parse_optional_float(raw: &str) -> Option<f64> {
    let value = raw.trim();
    if value.is_empty() || value == "[N/A]" || value == "N/A" {
        return None;
    }
    value.parse().ok()
}

pub fn parse_gpu_metrics(text: &str) -> Vec<GpuMetric> {
    let mut metrics = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() < 11 {
            continue;
        }
        metrics.push(GpuMetric {
            index: parts[0].parse().unwrap_or(0),
            name: parts[1].to_string(),
            uuid: parts[2].to_string(),
            memory_total_mib: parts[3].parse().unwrap_or(0.0),
            memory_used_mib: parts[4].parse().unwrap_or(0.0),
            utilization_gpu_percent: parts[6].parse().unwrap_or(0.0),
            utilization_memory_percent: parts[7].parse().unwrap_or(0.0),
            temperature_c: parts[8].parse().unwrap_or(0.0),
            power_draw_w: parse_optional_float(parts[9]),
            power_limit_w: parse_optional_float(parts[10]),
        });
    }
    metrics
}

pub fn collect_gpu_metrics(timeout: Duration) -> Result<Vec<GpuMetric>, GpuQueryError> {
    let output = run_command(
        &[
            "nvidia-smi",
            "--query-gpu=index,name,uuid,memory.total,memory.used,memory.free,\
             utilization.gpu,utilization.memory,temperature.gpu,power.draw,\
             power.limit",
            "--format=csv,noheader,nounits",
        ],
        timeout,
    )
    .map_err(classify_gpu_error)?;
    let metrics = parse_gpu_metrics(&output);
    if metrics.is_empty() {
        return Err(GpuQueryError {
            kind: GpuQueryErrorKind::NoDevices,
            message: "nvidia-smi returned no gpu rows".to_string(),
        });
    }
    Ok(metrics)
}

pub fn parse_gpu_processes(text: &str, proc_root: &Path) -> Vec<GpuProcess> {
    let mut processes = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() < 4 {
            continue;
        }
        let Ok(pid) = parts[1].parse::<u32>() else {
            continue;
        };
        let used_memory_mib = parts[3].parse().unwrap_or(0.0);
        let (user, command) = process_identity(proc_root, pid);
        processes.push(GpuProcess {
            gpu_uuid: parts[0].to_string(),
            pid,
            process_name: parts[2].to_string(),
            used_memory_mib,
            user,
            command,
        });
    }
    processes
}

pub fn collect_gpu_processes(proc_root: &Path, timeout: Duration) -> io::Result<Vec<GpuProcess>> {
    let output = run_command(
        &[
            "nvidia-smi",
            "--query-compute-apps=gpu_uuid,pid,process_name,used_memory",
            "--format=csv,noheader,nounits",
        ],
        timeout,
    )?;
    Ok(parse_gpu_processes(&output, proc_root))
}
