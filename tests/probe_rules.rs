use linux_healthy_agent::checks::{evaluate_gpu, evaluate_memory};
use linux_healthy_agent::docker::{parse_docker_ps, summarize_containers};
use linux_healthy_agent::gpu::parse_gpu_metrics;
use linux_healthy_agent::identity::collect_machine_identity;
use linux_healthy_agent::model::{CpuTimes, DiskStat, GpuMetric, Status, Thresholds};
use linux_healthy_agent::procfs::{
    calculate_cpu_busy_percent, calculate_disk_rates, count_processes, parse_diskstats,
    parse_meminfo, parse_uptime_seconds,
};
use std::fs;

#[test]
fn cpu_busy_is_calculated_from_proc_stat_delta() {
    let start = CpuTimes {
        user: 100,
        nice: 0,
        system: 50,
        idle: 850,
        iowait: 0,
        irq: 0,
        softirq: 0,
        steal: 0,
    };
    let end = CpuTimes {
        user: 160,
        nice: 0,
        system: 90,
        idle: 900,
        iowait: 0,
        irq: 0,
        softirq: 0,
        steal: 0,
    };

    let busy = calculate_cpu_busy_percent(start, end);

    assert!((busy - 66.666_666).abs() < 0.001);
}

#[test]
fn memory_warning_uses_large_host_absolute_floor() {
    let meminfo = parse_meminfo(
        "MemTotal:       1048576000 kB\n\
         MemFree:          1024000 kB\n\
         MemAvailable:    73400320 kB\n",
    );

    let checks = evaluate_memory(&meminfo, &Thresholds::default());

    assert_eq!(checks[0].status, Status::Warning);
}

#[test]
fn diskstats_rates_are_calculated_from_sector_delta() {
    let start = DiskStat {
        name: "nvme0n1".to_string(),
        read_ios: 100,
        read_sectors: 2000,
        write_ios: 50,
        write_sectors: 1000,
        io_ms: 1000,
    };
    let end = DiskStat {
        name: "nvme0n1".to_string(),
        read_ios: 160,
        read_sectors: 6096,
        write_ios: 90,
        write_sectors: 5096,
        io_ms: 1500,
    };

    let rates = calculate_disk_rates(&start, &end, 2.0);

    assert_eq!(rates["total_iops"].as_f64().unwrap(), 50.0);
    assert_eq!(rates["read_mib_per_second"].as_f64().unwrap(), 1.0);
    assert_eq!(rates["write_mib_per_second"].as_f64().unwrap(), 1.0);
    assert_eq!(rates["busy_percent"].as_f64().unwrap(), 25.0);
}

#[test]
fn diskstats_parser_finds_named_device() {
    let stats = parse_diskstats("259 0 nvme0n1 10 0 20 0 30 0 40 0 0 50 0 0 0 0 0 0\n");

    assert_eq!(stats["nvme0n1"].read_ios, 10);
    assert_eq!(stats["nvme0n1"].write_ios, 30);
}

#[test]
fn uptime_parser_reads_first_field() {
    assert_eq!(parse_uptime_seconds("12345.67 8910.11\n"), 12345.67);
    assert_eq!(parse_uptime_seconds("not-a-number 8910.11\n"), 0.0);
}

#[test]
fn process_count_only_counts_numeric_proc_dirs() {
    let root =
        std::env::temp_dir().join(format!("linux-healthy-agent-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("1")).unwrap();
    fs::create_dir_all(root.join("2048")).unwrap();
    fs::create_dir_all(root.join("self")).unwrap();
    fs::write(root.join("uptime"), "1.0 2.0\n").unwrap();

    assert_eq!(count_processes(&root).unwrap(), 2);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn gpu_idle_warning_requires_all_samples_idle() {
    let sample_one = vec![GpuMetric {
        index: 0,
        name: "NVIDIA RTX PRO 6000".to_string(),
        uuid: "GPU-test".to_string(),
        memory_total_mib: 100000.0,
        memory_used_mib: 90000.0,
        utilization_gpu_percent: 0.0,
        utilization_memory_percent: 0.0,
        temperature_c: 30.0,
        power_draw_w: Some(80.0),
        power_limit_w: Some(600.0),
    }];
    let sample_two = vec![GpuMetric {
        index: 0,
        name: "NVIDIA RTX PRO 6000".to_string(),
        uuid: "GPU-test".to_string(),
        memory_total_mib: 100000.0,
        memory_used_mib: 89000.0,
        utilization_gpu_percent: 5.0,
        utilization_memory_percent: 0.0,
        temperature_c: 31.0,
        power_draw_w: Some(82.0),
        power_limit_w: Some(600.0),
    }];

    let checks = evaluate_gpu(&[sample_one, sample_two], &Thresholds::default());
    let idle = checks
        .iter()
        .find(|check| check.name == "gpu_memory_idle")
        .unwrap();

    assert_eq!(idle.status, Status::Warning);
}

#[test]
fn gpu_metric_csv_parser_handles_nvidia_smi_output() {
    let metrics = parse_gpu_metrics(
        "0, NVIDIA RTX PRO 6000, GPU-test, 97887, 91967, 5284, \
         0, 0, 29, 83.08, 600.00\n",
    );

    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].index, 0);
    assert!((metrics[0].memory_used_percent() - 93.951).abs() < 0.01);
}

#[test]
fn empty_gpu_csv_has_no_gpu_checks() {
    let metrics = parse_gpu_metrics("");
    let checks = evaluate_gpu(&[metrics], &Thresholds::default());

    assert!(checks.is_empty());
}

#[test]
fn docker_summary_flags_unhealthy_and_restarting_containers() {
    let containers = parse_docker_ps(
        r#"{"ID":"a1","Names":"api","Image":"app:latest","State":"running","Status":"Up 10 minutes (unhealthy)"}
{"ID":"b2","Names":"worker","Image":"worker:latest","State":"restarting","Status":"Restarting (1) 5 seconds ago"}"#,
    );

    let summary = summarize_containers(&containers);

    assert_eq!(summary.total, 2);
    assert_eq!(summary.unhealthy, 1);
    assert_eq!(summary.restarting, 1);
    assert_eq!(summary.abnormal_containers.len(), 2);
}

#[test]
fn docker_summary_omits_healthy_running_containers_from_abnormal_list() {
    let containers = parse_docker_ps(
        r#"{"ID":"a1","Names":"api","Image":"app:latest","State":"running","Status":"Up 10 minutes"}"#,
    );

    let summary = summarize_containers(&containers);

    assert_eq!(summary.total, 1);
    assert_eq!(summary.running, 1);
    assert_eq!(summary.abnormal_containers.len(), 0);
}

#[test]
fn machine_identity_prefers_explicit_instance_name() {
    let identity = collect_machine_identity(Some("prod-gpu-eu-01"), Some("host-123"));

    assert_eq!(identity.host_id, "host-123");
    assert_eq!(identity.display_name, "prod-gpu-eu-01");
    assert!(!identity.hostname.is_empty());
    assert!(!identity.kernel.is_empty());
    assert!(!identity.machine_id_short.is_empty());
}

#[test]
fn machine_identity_falls_back_when_instance_name_is_blank() {
    let identity = collect_machine_identity(Some("   "), Some("   "));

    assert!(!identity.host_id.trim().is_empty());
    assert!(!identity.display_name.trim().is_empty());
    assert!(!identity.hostname.is_empty());
}
