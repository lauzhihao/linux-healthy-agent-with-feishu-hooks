use crate::model::{
    CpuTimes, DiskStat, NetworkStat, BYTES_PER_GIB, BYTES_PER_KIB, BYTES_PER_MIB, DISK_SECTOR_BYTES,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub type DeltaInputs = (
    CpuTimes,
    BTreeMap<String, DiskStat>,
    BTreeMap<String, NetworkStat>,
);

pub fn read_proc_text(proc_root: &Path, relative: &str) -> io::Result<String> {
    fs::read_to_string(proc_root.join(relative))
}

pub fn parse_cpu_times(text: &str) -> io::Result<CpuTimes> {
    for line in text.lines() {
        if !line.starts_with("cpu ") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cpu stat line has too few fields",
            ));
        }
        return Ok(CpuTimes {
            user: parts[1].parse().unwrap_or(0),
            nice: parts[2].parse().unwrap_or(0),
            system: parts[3].parse().unwrap_or(0),
            idle: parts[4].parse().unwrap_or(0),
            iowait: parts[5].parse().unwrap_or(0),
            irq: parts[6].parse().unwrap_or(0),
            softirq: parts[7].parse().unwrap_or(0),
            steal: parts[8].parse().unwrap_or(0),
        });
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "cpu stat line not found",
    ))
}

pub fn calculate_cpu_busy_percent(start: CpuTimes, end: CpuTimes) -> f64 {
    let total_delta = end.total().saturating_sub(start.total());
    let idle_delta = end.idle_all().saturating_sub(start.idle_all());
    if total_delta == 0 {
        return 0.0;
    }
    let busy = 1.0 - idle_delta as f64 / total_delta as f64;
    (busy * 100.0).clamp(0.0, 100.0)
}

pub fn parse_cpu_count(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let bytes = line.as_bytes();
            bytes.len() > 3
                && bytes[0] == b'c'
                && bytes[1] == b'p'
                && bytes[2] == b'u'
                && bytes[3].is_ascii_digit()
        })
        .count()
        .max(1)
}

pub fn parse_loadavg(text: &str) -> io::Result<BTreeMap<String, f64>> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "loadavg has too few fields",
        ));
    }
    let mut loadavg = BTreeMap::new();
    loadavg.insert("load1".to_string(), parts[0].parse().unwrap_or(0.0));
    loadavg.insert("load5".to_string(), parts[1].parse().unwrap_or(0.0));
    loadavg.insert("load15".to_string(), parts[2].parse().unwrap_or(0.0));
    Ok(loadavg)
}

pub fn parse_pressure(text: &str) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(kind) = parts.next() else {
            continue;
        };
        let mut values = BTreeMap::new();
        for token in parts {
            let Some((key, value)) = token.split_once('=') else {
                continue;
            };
            values.insert(key.to_string(), value.parse().unwrap_or(0.0));
        }
        result.insert(kind.to_string(), values);
    }
    result
}

pub fn read_pressure(
    proc_root: &Path,
) -> BTreeMap<String, BTreeMap<String, BTreeMap<String, f64>>> {
    let mut pressure = BTreeMap::new();
    for name in ["cpu", "memory", "io"] {
        let path = proc_root.join("pressure").join(name);
        let value = fs::read_to_string(path)
            .map(|text| parse_pressure(&text))
            .unwrap_or_default();
        pressure.insert(name.to_string(), value);
    }
    pressure
}

pub fn parse_meminfo(text: &str) -> BTreeMap<String, u64> {
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let mut parsed = parts[0].parse::<u64>().unwrap_or(0);
        if parts.get(1).copied() == Some("kB") {
            parsed *= BYTES_PER_KIB as u64;
        }
        result.insert(key.to_string(), parsed);
    }
    result
}

pub fn parse_diskstats(text: &str) -> BTreeMap<String, DiskStat> {
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 14 {
            continue;
        }
        let stat = DiskStat {
            name: parts[2].to_string(),
            read_ios: parts[3].parse().unwrap_or(0),
            read_sectors: parts[5].parse().unwrap_or(0),
            write_ios: parts[7].parse().unwrap_or(0),
            write_sectors: parts[9].parse().unwrap_or(0),
            io_ms: parts[12].parse().unwrap_or(0),
        };
        result.insert(stat.name.clone(), stat);
    }
    result
}

pub fn calculate_disk_rates(start: &DiskStat, end: &DiskStat, elapsed: f64) -> Value {
    let elapsed = elapsed.max(0.001);
    let read_ios = end.read_ios.saturating_sub(start.read_ios);
    let write_ios = end.write_ios.saturating_sub(start.write_ios);
    let read_sectors = end.read_sectors.saturating_sub(start.read_sectors);
    let write_sectors = end.write_sectors.saturating_sub(start.write_sectors);
    let io_ms = end.io_ms.saturating_sub(start.io_ms);
    let read_mib = read_sectors as f64 * DISK_SECTOR_BYTES / BYTES_PER_MIB;
    let write_mib = write_sectors as f64 * DISK_SECTOR_BYTES / BYTES_PER_MIB;
    json!({
        "read_iops": read_ios as f64 / elapsed,
        "write_iops": write_ios as f64 / elapsed,
        "total_iops": (read_ios + write_ios) as f64 / elapsed,
        "read_mib_per_second": read_mib / elapsed,
        "write_mib_per_second": write_mib / elapsed,
        "total_mib_per_second": (read_mib + write_mib) / elapsed,
        "busy_percent": (io_ms as f64 / (elapsed * 1000.0) * 100.0).min(100.0),
    })
}

pub fn parse_net_dev(text: &str) -> BTreeMap<String, NetworkStat> {
    let mut result = BTreeMap::new();
    for line in text.lines().skip(2) {
        let Some((name, values)) = line.split_once(':') else {
            continue;
        };
        let parts: Vec<&str> = values.split_whitespace().collect();
        if parts.len() < 16 {
            continue;
        }
        let stat = NetworkStat {
            name: name.trim().to_string(),
            rx_bytes: parts[0].parse().unwrap_or(0),
            rx_packets: parts[1].parse().unwrap_or(0),
            rx_errors: parts[2].parse().unwrap_or(0),
            rx_drops: parts[3].parse().unwrap_or(0),
            tx_bytes: parts[8].parse().unwrap_or(0),
            tx_packets: parts[9].parse().unwrap_or(0),
            tx_errors: parts[10].parse().unwrap_or(0),
            tx_drops: parts[11].parse().unwrap_or(0),
        };
        result.insert(stat.name.clone(), stat);
    }
    result
}

pub fn calculate_network_rates(start: &NetworkStat, end: &NetworkStat, elapsed: f64) -> Value {
    let elapsed = elapsed.max(0.001);
    json!({
        "rx_mib_per_second": end.rx_bytes.saturating_sub(start.rx_bytes) as f64
            / BYTES_PER_MIB / elapsed,
        "tx_mib_per_second": end.tx_bytes.saturating_sub(start.tx_bytes) as f64
            / BYTES_PER_MIB / elapsed,
        "rx_packets_per_second": end.rx_packets.saturating_sub(start.rx_packets)
            as f64 / elapsed,
        "tx_packets_per_second": end.tx_packets.saturating_sub(start.tx_packets)
            as f64 / elapsed,
        "rx_errors_delta": end.rx_errors.saturating_sub(start.rx_errors),
        "tx_errors_delta": end.tx_errors.saturating_sub(start.tx_errors),
        "rx_drops_delta": end.rx_drops.saturating_sub(start.rx_drops),
        "tx_drops_delta": end.tx_drops.saturating_sub(start.tx_drops),
    })
}

pub fn disk_usage(path: &Path) -> io::Result<Value> {
    let c_path = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    let total = stat.f_frsize as u128 * stat.f_blocks as u128;
    let available = stat.f_frsize as u128 * stat.f_bavail as u128;
    let used = total.saturating_sub(available);
    let used_percent = if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    };
    Ok(json!({
        "mount": path.to_string_lossy(),
        "total_bytes": total,
        "available_bytes": available,
        "used_bytes": used,
        "used_percent": used_percent,
        "available_gib": available as f64 / BYTES_PER_GIB,
        "total_gib": total as f64 / BYTES_PER_GIB,
    }))
}

pub fn process_identity(proc_root: &Path, pid: u32) -> (String, String) {
    let process_root: PathBuf = proc_root.join(pid.to_string());
    let user = fs::read_to_string(process_root.join("status"))
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                if !line.starts_with("Uid:") {
                    return None;
                }
                line.split_whitespace().nth(1).map(str::to_string)
            })
        })
        .unwrap_or_else(|| "unknown".to_string());
    let command = fs::read(process_root.join("cmdline"))
        .ok()
        .map(|bytes| {
            bytes
                .split(|byte| *byte == 0)
                .filter(|part| !part.is_empty())
                .map(|part| String::from_utf8_lossy(part).to_string())
                .collect::<Vec<String>>()
                .join(" ")
        })
        .unwrap_or_default();
    (user, command)
}

pub fn read_delta_inputs(proc_root: &Path) -> io::Result<DeltaInputs> {
    let cpu = parse_cpu_times(&read_proc_text(proc_root, "stat")?)?;
    let disk = parse_diskstats(&read_proc_text(proc_root, "diskstats")?);
    let net = parse_net_dev(&read_proc_text(proc_root, "net/dev")?);
    Ok((cpu, disk, net))
}
