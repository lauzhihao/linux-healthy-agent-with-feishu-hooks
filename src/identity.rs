use serde::Serialize;
use std::env;
use std::fs;
use std::process::Command;

#[derive(Clone, Debug, Serialize)]
pub struct MachineIdentity {
    pub host_id: String,
    pub display_name: String,
    pub hostname: String,
    pub kernel: String,
    pub machine_id_short: String,
}

fn read_hostname() -> String {
    env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fs::read_to_string("/etc/hostname").ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_kernel() -> String {
    let output = Command::new("uname").args(["-srmo"]).output();
    let Ok(output) = output else {
        return "unknown".to_string();
    };
    if !output.status.success() {
        return "unknown".to_string();
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn read_machine_id_short() -> String {
    let machine_id = fs::read_to_string("/etc/machine-id")
        .or_else(|_| fs::read_to_string("/var/lib/dbus/machine-id"))
        .unwrap_or_default();
    let cleaned: String = machine_id
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(12)
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

pub fn collect_machine_identity(
    instance_name: Option<&str>,
    host_id: Option<&str>,
) -> MachineIdentity {
    let hostname = read_hostname();
    let machine_id_short = read_machine_id_short();
    let env_name = env::var("LINUX_HEALTHY_AGENT_INSTANCE_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let display_name = instance_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or(env_name)
        .unwrap_or_else(|| hostname.clone());
    let env_host_id = env::var("LINUX_HEALTHY_AGENT_HOST_ID")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let host_id = host_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or(env_host_id)
        .unwrap_or_else(|| {
            if machine_id_short == "unknown" {
                hostname.clone()
            } else {
                machine_id_short.clone()
            }
        });
    MachineIdentity {
        host_id,
        display_name,
        hostname,
        kernel: read_kernel(),
        machine_id_short,
    }
}
