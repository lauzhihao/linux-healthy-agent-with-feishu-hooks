use clap::{Parser, ValueEnum};
use linux_healthy_agent::checks::{
    evaluate_cpu, evaluate_disk_io, evaluate_disk_usage, evaluate_gpu, evaluate_memory,
    evaluate_network, worst_status,
};
use linux_healthy_agent::docker::{collect_docker_health, DockerHealthSummary};
use linux_healthy_agent::gpu::{
    collect_gpu_metrics, collect_gpu_processes, GpuQueryError, GpuQueryErrorKind,
};
use linux_healthy_agent::identity::collect_machine_identity;
use linux_healthy_agent::model::{
    CheckResult, DeploymentMetadata, ProbeReport, Status, Thresholds,
};
use linux_healthy_agent::procfs::{
    calculate_cpu_busy_percent, calculate_disk_rates, calculate_network_rates, count_processes,
    disk_usage, parse_cpu_count, parse_loadavg, parse_meminfo, parse_uptime_seconds,
    read_delta_inputs, read_pressure, read_proc_text,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[command(version, about = "Read-only low-overhead EC2 resource probe.")]
struct Args {
    #[arg(long, default_value = "/proc")]
    proc_root: PathBuf,

    #[arg(long = "mount")]
    mounts: Vec<PathBuf>,

    #[arg(long = "disk-device")]
    disk_devices: Vec<String>,

    #[arg(long, default_value_t = 5.0)]
    interval: f64,

    #[arg(long, default_value_t = 2)]
    samples: usize,

    #[arg(long, default_value_t = 3.0)]
    command_timeout: f64,

    #[arg(long, default_value_t = 20000.0)]
    ebs_iops_limit: f64,

    #[arg(long, default_value_t = 2000.0)]
    ebs_throughput_mib_limit: f64,

    #[arg(long, default_value_t = 8)]
    top_processes: usize,

    #[arg(long)]
    skip_gpu: bool,

    #[arg(long, value_enum, default_value_t = GpuMode::Auto)]
    gpu_mode: GpuMode,

    #[arg(long)]
    gpu_processes: bool,

    #[arg(long)]
    skip_docker: bool,

    #[arg(long, default_value_t = 1.0)]
    docker_timeout: f64,

    #[arg(long)]
    pretty: bool,

    #[arg(long)]
    enable_alerts: bool,

    #[arg(long)]
    webhook_url: Option<String>,

    #[arg(long)]
    instance_name: Option<String>,

    #[arg(long)]
    host_id: Option<String>,

    #[arg(long)]
    provider: Option<String>,

    #[arg(long)]
    cloud_region: Option<String>,

    #[arg(long)]
    zone: Option<String>,

    #[arg(long)]
    fleet_region: Option<String>,

    #[arg(long)]
    role: Option<String>,

    #[arg(long)]
    output_file: Option<PathBuf>,

    #[arg(long)]
    alert_state_file: Option<PathBuf>,

    #[arg(long)]
    alert_thresholds_file: Option<PathBuf>,

    #[arg(long, default_value_t = 3600)]
    warning_alert_interval_seconds: u64,

    #[arg(long, default_value_t = 3.0)]
    alert_timeout: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum GpuMode {
    Auto,
    Required,
    Disabled,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct AlertState {
    last_warning_sent_unix: u64,
}

struct AlertConfig {
    webhook_url: String,
    thresholds: Thresholds,
    state_file: Option<PathBuf>,
    warning_alert_interval_seconds: u64,
    timeout: Duration,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn hostname() -> String {
    env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fs::read_to_string("/etc/hostname").ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn option_or_env(option: &Option<String>, env_key: &str) -> Option<String> {
    option
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            env::var(env_key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

fn path_option_or_env(option: &Option<PathBuf>, env_key: &str) -> Option<PathBuf> {
    option.clone().or_else(|| {
        env::var(env_key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    })
}

fn env_flag(env_key: &str) -> bool {
    env::var(env_key)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

fn collect_deployment_metadata(args: &Args) -> Option<DeploymentMetadata> {
    let metadata = DeploymentMetadata {
        provider: option_or_env(&args.provider, "LINUX_HEALTHY_AGENT_PROVIDER"),
        cloud_region: option_or_env(&args.cloud_region, "LINUX_HEALTHY_AGENT_CLOUD_REGION"),
        zone: option_or_env(&args.zone, "LINUX_HEALTHY_AGENT_ZONE"),
        fleet_region: option_or_env(&args.fleet_region, "LINUX_HEALTHY_AGENT_FLEET_REGION"),
        role: option_or_env(&args.role, "LINUX_HEALTHY_AGENT_ROLE"),
    };
    if metadata.is_empty() {
        None
    } else {
        Some(metadata)
    }
}

fn primary_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

fn check_result(name: &str, status: Status, message: &str, values: Value) -> CheckResult {
    CheckResult {
        name: name.to_string(),
        status,
        message: message.to_string(),
        values,
    }
}

fn alert_count_status(count: usize, warning: u64, critical: u64) -> Status {
    let count = count as u64;
    if critical > 0 && count >= critical {
        Status::Critical
    } else if warning > 0 && count >= warning {
        Status::Warning
    } else {
        Status::Ok
    }
}

fn evaluate_docker_alerts(
    summary: &DockerHealthSummary,
    thresholds: &Thresholds,
) -> Vec<CheckResult> {
    vec![
        check_result(
            "docker_container_unhealthy",
            alert_count_status(
                summary.unhealthy,
                thresholds.docker_unhealthy_warning_count,
                thresholds.docker_unhealthy_critical_count,
            ),
            "docker container healthcheck failed",
            json!({
                "unhealthy": summary.unhealthy,
                "abnormal_containers": summary.abnormal_containers,
            }),
        ),
        check_result(
            "docker_container_restarting",
            alert_count_status(
                summary.restarting,
                thresholds.docker_restarting_warning_count,
                thresholds.docker_restarting_critical_count,
            ),
            "docker container is restarting",
            json!({
                "restarting": summary.restarting,
                "abnormal_containers": summary.abnormal_containers,
            }),
        ),
        check_result(
            "docker_container_exited",
            alert_count_status(
                summary.exited,
                thresholds.docker_exited_warning_count,
                thresholds.docker_exited_critical_count,
            ),
            "docker container is not running",
            json!({
                "exited": summary.exited,
                "abnormal_containers": summary.abnormal_containers,
            }),
        ),
        check_result(
            "docker_container_other_abnormal",
            alert_count_status(
                summary.other_abnormal,
                thresholds.docker_other_abnormal_warning_count,
                thresholds.docker_other_abnormal_critical_count,
            ),
            "docker container is in an unexpected state",
            json!({
                "other_abnormal": summary.other_abnormal,
                "abnormal_containers": summary.abnormal_containers,
            }),
        ),
    ]
}

fn collect_top_processes(limit: usize) -> Vec<Value> {
    if limit == 0 {
        return Vec::new();
    }
    let output = Command::new("ps")
        .args([
            "-eo",
            "pid,ppid,user,stat,pcpu,pmem,rss,etime,comm,args",
            "--sort=-pcpu",
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .skip(1)
        .take(limit)
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 10 {
                return None;
            }
            Some(json!({
                "pid": parts[0],
                "ppid": parts[1],
                "user": parts[2],
                "stat": parts[3],
                "cpu_percent": parts[4],
                "mem_percent": parts[5],
                "rss_kib": parts[6],
                "elapsed": parts[7],
                "command": parts[8],
                "args": parts[9..].join(" "),
            }))
        })
        .collect()
}

fn selected_mounts(args: &Args) -> Vec<PathBuf> {
    if !args.mounts.is_empty() {
        return args.mounts.clone();
    }
    vec![PathBuf::from("/"), PathBuf::from("/opt/dlami/nvme")]
}

fn selected_disk_devices(args: &Args) -> Vec<String> {
    if !args.disk_devices.is_empty() {
        return args.disk_devices.clone();
    }
    vec!["nvme0n1".to_string()]
}

fn collect_report(
    args: &Args,
    alert_thresholds: Option<&Thresholds>,
) -> io::Result<(ProbeReport, Vec<CheckResult>)> {
    let started = Instant::now();
    let (cpu_start, disk_start, net_start) = read_delta_inputs(&args.proc_root)?;
    let timeout = Duration::from_secs_f64(args.command_timeout.max(0.5));
    let mut gpu_samples = Vec::new();
    let mut gpu_processes = Vec::new();
    let gpu_mode = if args.skip_gpu {
        GpuMode::Disabled
    } else {
        args.gpu_mode
    };
    let mut gpu_available = false;
    let mut gpu_error: Option<GpuQueryError> = None;
    let mut docker_health = None;
    let mut errors = BTreeMap::new();

    if gpu_mode != GpuMode::Disabled {
        match collect_gpu_metrics(timeout) {
            Ok(metrics) => {
                gpu_available = true;
                gpu_samples.push(metrics);
            }
            Err(error) => {
                gpu_error = Some(error);
            }
        }
    }

    let samples = args.samples.max(1);
    let interval = Duration::from_secs_f64(args.interval.max(0.2));
    for _ in 1..samples {
        thread::sleep(interval);
        if gpu_mode != GpuMode::Disabled && gpu_available {
            match collect_gpu_metrics(timeout) {
                Ok(metrics) => gpu_samples.push(metrics),
                Err(error) => {
                    gpu_error = Some(error);
                    gpu_available = false;
                }
            }
        }
    }
    if samples == 1 {
        thread::sleep(interval);
    }

    if args.gpu_processes && gpu_available {
        match collect_gpu_processes(&args.proc_root, timeout) {
            Ok(processes) => {
                gpu_processes = processes;
            }
            Err(error) => {
                errors.insert("gpu_processes".to_string(), error.to_string());
            }
        }
    }

    if !args.skip_docker {
        let docker_timeout = Duration::from_secs_f64(args.docker_timeout.max(0.2));
        match collect_docker_health(docker_timeout) {
            Ok(summary) => {
                docker_health = Some(summary);
            }
            Err(error) => {
                errors.insert("docker".to_string(), error.to_string());
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    let (cpu_end, disk_end, net_end) = read_delta_inputs(&args.proc_root)?;
    let stat_text = read_proc_text(&args.proc_root, "stat")?;
    let cpu_count = parse_cpu_count(&stat_text);
    let loadavg = parse_loadavg(&read_proc_text(&args.proc_root, "loadavg")?)?;
    let uptime_seconds = read_proc_text(&args.proc_root, "uptime")
        .map(|text| parse_uptime_seconds(&text))
        .unwrap_or(0.0);
    let process_count = count_processes(&args.proc_root).unwrap_or(0);
    let primary_ip = primary_ip();
    let pressure = read_pressure(&args.proc_root);
    let meminfo = parse_meminfo(&read_proc_text(&args.proc_root, "meminfo")?);

    let mut disk_usages = Vec::new();
    for mount in selected_mounts(args) {
        if let Ok(usage) = disk_usage(&mount) {
            disk_usages.push(usage);
        }
    }

    let mut disk_rates = BTreeMap::new();
    for device in selected_disk_devices(args) {
        let Some(start) = disk_start.get(&device) else {
            continue;
        };
        let Some(end) = disk_end.get(&device) else {
            continue;
        };
        disk_rates.insert(device, calculate_disk_rates(start, end, elapsed));
    }

    let mut network_rates = BTreeMap::new();
    for (name, start) in &net_start {
        if name == "lo" {
            continue;
        }
        if let Some(end) = net_end.get(name) {
            network_rates.insert(name.clone(), calculate_network_rates(start, end, elapsed));
        }
    }

    let cpu_busy_percent = calculate_cpu_busy_percent(cpu_start, cpu_end);
    let mut alert_checks = Vec::new();
    if let Some(thresholds) = alert_thresholds {
        alert_checks.extend(evaluate_cpu(
            cpu_busy_percent,
            &loadavg,
            cpu_count,
            &pressure,
            thresholds,
        ));
        alert_checks.extend(evaluate_memory(&meminfo, thresholds));
        alert_checks.extend(evaluate_disk_usage(&disk_usages, thresholds));
        alert_checks.extend(evaluate_disk_io(
            &disk_rates,
            thresholds,
            args.ebs_iops_limit,
            args.ebs_throughput_mib_limit,
        ));
        alert_checks.extend(evaluate_network(&network_rates, thresholds));
        alert_checks.extend(evaluate_gpu(&gpu_samples, thresholds));
        if let Some(summary) = &docker_health {
            alert_checks.extend(evaluate_docker_alerts(summary, thresholds));
        }
        if let Some(error) = &gpu_error {
            if gpu_mode == GpuMode::Required {
                alert_checks.push(check_result(
                    "gpu_collection",
                    Status::Critical,
                    "required gpu collector failed",
                    json!({"kind": error.kind, "error": error.message}),
                ));
            }
        }
    }
    if let Some(error) = &gpu_error {
        match gpu_mode {
            GpuMode::Required => {
                errors.insert("gpu".to_string(), error.to_string());
            }
            GpuMode::Auto => {
                if !matches!(
                    error.kind,
                    GpuQueryErrorKind::CommandMissing | GpuQueryErrorKind::NoDevices
                ) {
                    errors.insert("gpu".to_string(), error.to_string());
                }
            }
            GpuMode::Disabled => {}
        }
    }
    let gpu_metrics: Vec<_> = gpu_samples.iter().flatten().cloned().collect();
    let identity = collect_machine_identity(args.instance_name.as_deref(), args.host_id.as_deref());
    let report = ProbeReport {
        schema_version: 3,
        timestamp_unix: now_unix(),
        hostname: identity.hostname.clone(),
        identity,
        deployment: collect_deployment_metadata(args),
        elapsed_seconds: elapsed,
        metrics: json!({
            "cpu": {
                "busy_percent": cpu_busy_percent,
            },
            "cpu_count": cpu_count,
            "loadavg": loadavg,
            "pressure": pressure,
            "memory": {
                "total_bytes": meminfo.get("MemTotal").copied().unwrap_or(0),
                "available_bytes": meminfo.get("MemAvailable").copied().unwrap_or(0),
            },
            "uptime_seconds": uptime_seconds,
            "process_count": process_count,
            "primary_ip": primary_ip,
            "disk_usage": disk_usages,
            "disk_io": disk_rates,
            "network": network_rates,
            "gpu": gpu_metrics,
            "gpu_available": gpu_available,
            "gpu_mode": format!("{:?}", gpu_mode).to_ascii_lowercase(),
            "gpu_processes": gpu_processes,
            "docker": docker_health,
            "top_processes": collect_top_processes(args.top_processes),
        }),
        errors,
    };
    Ok((report, alert_checks))
}

fn read_alert_state(path: &Path) -> AlertState {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_alert_state(path: &Path, state: &AlertState) -> io::Result<()> {
    let text = serde_json::to_string(state)?;
    fs::write(path, text)
}

fn should_send_warning_alert(config: &AlertConfig, now: u64) -> bool {
    let Some(path) = &config.state_file else {
        return true;
    };
    let state = read_alert_state(path);
    now.saturating_sub(state.last_warning_sent_unix) >= config.warning_alert_interval_seconds
}

fn mark_warning_alert_sent(config: &AlertConfig, now: u64) {
    let Some(path) = &config.state_file else {
        return;
    };
    let state = AlertState {
        last_warning_sent_unix: now,
    };
    let _ = write_alert_state(path, &state);
}

fn webhook_url(args: &Args) -> Option<String> {
    args.webhook_url
        .clone()
        .or_else(|| env::var("LINUX_HEALTHY_AGENT_WEBHOOK_URL").ok())
        .or_else(|| env::var("FEISHU_WEBHOOK_URL").ok())
        .filter(|value| !value.trim().is_empty())
}

fn load_alert_thresholds(path: &Path) -> Result<Thresholds, String> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read alert thresholds {}: {error}",
            path.display()
        )
    })?;
    serde_json::from_str(&text).map_err(|error| {
        format!(
            "failed to parse alert thresholds {}: {error}",
            path.display()
        )
    })
}

fn alert_config(args: &Args) -> Result<Option<AlertConfig>, String> {
    if !args.enable_alerts && !env_flag("LINUX_HEALTHY_AGENT_ENABLE_ALERTS") {
        return Ok(None);
    }

    let webhook_url = webhook_url(args).ok_or_else(|| {
        "alerts are enabled but no webhook was configured; set --webhook-url, FEISHU_WEBHOOK_URL, or LINUX_HEALTHY_AGENT_WEBHOOK_URL".to_string()
    })?;
    let thresholds_file = path_option_or_env(
        &args.alert_thresholds_file,
        "LINUX_HEALTHY_AGENT_ALERT_THRESHOLDS_FILE",
    )
    .ok_or_else(|| {
        "alerts are enabled but no thresholds file was configured; set --alert-thresholds-file or LINUX_HEALTHY_AGENT_ALERT_THRESHOLDS_FILE".to_string()
    })?;
    let thresholds = load_alert_thresholds(&thresholds_file)?;

    Ok(Some(AlertConfig {
        webhook_url,
        thresholds,
        state_file: path_option_or_env(
            &args.alert_state_file,
            "LINUX_HEALTHY_AGENT_ALERT_STATE_FILE",
        ),
        warning_alert_interval_seconds: args.warning_alert_interval_seconds,
        timeout: Duration::from_secs_f64(args.alert_timeout.max(0.5)),
    }))
}

fn alert_message(report: &ProbeReport, checks: &[CheckResult], status: Status) -> String {
    let critical: Vec<&CheckResult> = checks
        .iter()
        .filter(|check| check.status == Status::Critical)
        .collect();
    let warnings: Vec<&CheckResult> = checks
        .iter()
        .filter(|check| check.status == Status::Warning)
        .collect();
    let mut lines = vec![
        format!("Linux Healthy Agent alert status: {:?}", status),
        format!("machine: {}", report.identity.display_name),
        format!("hostname: {}", report.identity.hostname),
        format!("kernel: {}", report.identity.kernel),
        format!("machine_id: {}", report.identity.machine_id_short),
        format!("timestamp_unix: {}", report.timestamp_unix),
    ];
    if !critical.is_empty() {
        lines.push(format!("critical_checks: {}", critical.len()));
        for check in critical.iter().take(8) {
            lines.push(format!("- {}: {}", check.name, check.message));
        }
    }
    if !warnings.is_empty() {
        lines.push(format!("warning_checks: {}", warnings.len()));
        for check in warnings.iter().take(8) {
            lines.push(format!("- {}: {}", check.name, check.message));
        }
    }
    lines.join("\n")
}

fn send_feishu_alert(url: &str, text: &str, timeout: Duration) -> io::Result<()> {
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let body = json!({
        "msg_type": "text",
        "content": {
            "text": text,
        },
    });
    agent
        .post(url)
        .send_json(body)
        .map(|_| ())
        .map_err(|error| io::Error::other(error.to_string()))
}

fn maybe_send_alert(config: Option<&AlertConfig>, report: &ProbeReport, checks: &[CheckResult]) {
    let Some(config) = config else {
        return;
    };
    let status = worst_status(checks);
    if status == Status::Ok {
        return;
    }

    let now = now_unix();
    let should_send = if status == Status::Critical {
        true
    } else {
        should_send_warning_alert(config, now)
    };
    if !should_send {
        return;
    }

    if send_feishu_alert(
        &config.webhook_url,
        &alert_message(report, checks, status),
        config.timeout,
    )
    .is_ok()
        && status == Status::Warning
    {
        mark_warning_alert_sent(config, now);
    }
}

fn write_output_file(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("linux-healthy-agent.json");
    let tmp_path = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(text.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, path)
}

fn run() -> i32 {
    let args = Args::parse();
    let alert_config = match alert_config(&args) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let alert_thresholds = alert_config.as_ref().map(|config| &config.thresholds);
    let mut probe_failed = false;
    let (report, alert_checks) = match collect_report(&args, alert_thresholds) {
        Ok(collected) => collected,
        Err(error) => {
            probe_failed = true;
            let report = ProbeReport {
                schema_version: 3,
                timestamp_unix: now_unix(),
                hostname: hostname(),
                identity: collect_machine_identity(
                    args.instance_name.as_deref(),
                    args.host_id.as_deref(),
                ),
                deployment: collect_deployment_metadata(&args),
                elapsed_seconds: 0.0,
                metrics: json!({}),
                errors: BTreeMap::from([("probe".to_string(), error.to_string())]),
            };
            let checks = vec![check_result(
                "probe_failure",
                Status::Critical,
                "probe failed",
                json!({"error": error.to_string()}),
            )];
            (report, checks)
        }
    };
    maybe_send_alert(alert_config.as_ref(), &report, &alert_checks);
    let output = if args.pretty {
        serde_json::to_string_pretty(&report)
    } else {
        serde_json::to_string(&report)
    };
    match output {
        Ok(text) => {
            println!("{text}");
            if let Some(path) = &args.output_file {
                if let Err(error) = write_output_file(path, &text) {
                    eprintln!("failed to write output file {}: {error}", path.display());
                    return 2;
                }
            }
        }
        Err(error) => {
            eprintln!("failed to serialize report: {error}");
            return 2;
        }
    }
    if probe_failed {
        2
    } else {
        0
    }
}

fn main() {
    std::process::exit(run());
}
