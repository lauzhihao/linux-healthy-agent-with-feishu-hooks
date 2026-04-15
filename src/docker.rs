use serde::Serialize;
use serde_json::Value;
use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

#[derive(Clone, Debug, Serialize)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DockerHealthSummary {
    pub total: usize,
    pub running: usize,
    pub unhealthy: usize,
    pub restarting: usize,
    pub exited: usize,
    pub other_abnormal: usize,
    pub abnormal_containers: Vec<DockerContainer>,
}

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

fn json_string(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn parse_docker_ps(text: &str) -> Vec<DockerContainer> {
    let mut containers = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        containers.push(DockerContainer {
            id: json_string(&row, "ID"),
            name: json_string(&row, "Names"),
            image: json_string(&row, "Image"),
            state: json_string(&row, "State"),
            status: json_string(&row, "Status"),
        });
    }
    containers
}

pub fn summarize_containers(containers: &[DockerContainer]) -> DockerHealthSummary {
    let mut summary = DockerHealthSummary {
        total: containers.len(),
        running: 0,
        unhealthy: 0,
        restarting: 0,
        exited: 0,
        other_abnormal: 0,
        abnormal_containers: Vec::new(),
    };

    for container in containers {
        let state = container.state.to_ascii_lowercase();
        let status = container.status.to_ascii_lowercase();
        let unhealthy = status.contains("unhealthy");
        let restarting = state == "restarting" || status.contains("restarting");
        let exited = state == "exited" || state == "dead";
        let running = state == "running";

        if running {
            summary.running += 1;
        }
        if unhealthy {
            summary.unhealthy += 1;
        }
        if restarting {
            summary.restarting += 1;
        }
        if exited {
            summary.exited += 1;
        }

        let abnormal = unhealthy || restarting || exited || (!running && !state.is_empty());
        if abnormal {
            if !unhealthy && !restarting && !exited {
                summary.other_abnormal += 1;
            }
            summary.abnormal_containers.push(container.clone());
        }
    }
    summary
}

pub fn collect_docker_health(timeout: Duration) -> io::Result<DockerHealthSummary> {
    let output = run_command(
        &["docker", "ps", "--all", "--format", "{{json .}}"],
        timeout,
    )?;
    let containers = parse_docker_ps(&output);
    Ok(summarize_containers(&containers))
}
