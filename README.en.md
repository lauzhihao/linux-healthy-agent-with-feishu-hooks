# Linux Healthy Agent

[中文](README.md)

Linux Healthy Agent is a read-only, low-overhead Linux health probe with
Feishu bot webhook alerts. It is designed for regular EC2 instances, GPU
instances, bare-metal Linux hosts, and Docker hosts.

## Features

- Written in Rust and can be built as a single static binary.
- Read-only by default. It does not modify service state or run write tests.
- Collects CPU, load, CPU PSI, memory, disk usage, disk I/O, and network
  errors/drops.
- Adaptive GPU mode: checks NVIDIA GPUs when present, skips GPU checks by
  default when no GPU exists.
- Works with NVIDIA GPU hosts such as G7E, A100, and H100.
- Docker health check is enabled by default and only reports abnormal
  containers.
- Feishu webhook alerts with warning throttling and immediate critical alerts.
- Stable JSON output for systemd timers, cron, logs, and integrations.

## Safety Boundary

By default, the agent does not:

- Write disk data or create temporary logs.
- Stop, freeze, interrupt, restart, or pause business processes or containers.
- Attach to processes with tools such as `ptrace`, `gdb`, `strace`, or `perf`.
- Write `/proc/sys`, `/sys`, or cgroup control files.
- Run Docker control operations.

It reads:

- `/proc/stat`
- `/proc/loadavg`
- `/proc/meminfo`
- `/proc/diskstats`
- `/proc/net/dev`
- `/proc/pressure/*`
- mount metadata through `statvfs`
- optional `nvidia-smi`
- one lightweight `docker ps --all --format "{{json .}}"` by default

Warning alert throttling needs a state file. Use tmpfs such as `/run`:

```bash
--alert-state-file /run/linux-healthy-agent-alert.json
```

## One-Line Install

For servers, install the prebuilt static binary from GitHub Releases. No Rust
toolchain or source build is required:

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh | sudo sh
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --version v0.1.0
```

Overwrite an existing binary:

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --force
```

Install and enable the systemd timer:

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --with-systemd
```

The installer:

- Checks that the system is Linux x86_64.
- Downloads the `x86_64-unknown-linux-musl` static binary from GitHub Releases.
- Downloads and verifies SHA256.
- Installs to `/usr/local/bin/linux-healthy-agent`.
- Does not write any real webhook URL.
- Installs systemd timer only when `--with-systemd` is passed.

Safer install flow:

```bash
curl -fsSLO https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh
less install.sh
sudo sh install.sh
```

## Build From Source

Install Rust, then run:

```bash
cargo build --release
```

Build a Linux x86_64 static binary:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

Artifact:

```bash
target/x86_64-unknown-linux-musl/release/linux-healthy-agent
```

Check static linkage:

```bash
file target/x86_64-unknown-linux-musl/release/linux-healthy-agent
ldd target/x86_64-unknown-linux-musl/release/linux-healthy-agent
```

`ldd` should report `statically linked` or `not a dynamic executable`.

## Quick Start

Default run:

```bash
./linux-healthy-agent
```

Pretty JSON:

```bash
./linux-healthy-agent --pretty
```

Minimal low-overhead run:

```bash
./linux-healthy-agent --gpu-mode disabled --skip-docker --top-processes 0 \
  --interval 1 --samples 1
```

## GPU Mode

Default:

```bash
./linux-healthy-agent --gpu-mode auto
```

Modes:

- `auto`: check NVIDIA GPUs when present; do not alert when no GPU exists.
- `required`: GPU must exist, otherwise critical.
- `disabled`: skip GPU checks and do not call `nvidia-smi`.

Recommended for GPU hosts:

```bash
./linux-healthy-agent --gpu-mode required
```

GPU process attribution is disabled by default:

```bash
./linux-healthy-agent --gpu-processes
```

## Docker Health

Docker health check is enabled by default and runs once:

```bash
docker ps --all --format "{{json .}}"
```

Alert rules:

- `unhealthy`: critical.
- `restarting`: critical.
- `exited`, `dead`, or other non-running states: warning.
- Healthy running containers are not listed one by one.

Disable Docker checks:

```bash
./linux-healthy-agent --skip-docker
```

## Feishu Alerts

Never commit webhook URLs to Git. Use an environment variable:

```bash
export FEISHU_WEBHOOK_URL="https://open.feishu.cn/open-apis/bot/v2/hook/..."
./linux-healthy-agent --alert-state-file /run/linux-healthy-agent-alert.json
```

Or pass it with CLI:

```bash
./linux-healthy-agent --webhook-url "$FEISHU_WEBHOOK_URL"
```

When multiple machines share one Feishu bot, set a machine identity:

```bash
export LINUX_HEALTHY_AGENT_INSTANCE_NAME="prod-gpu-eu-01"
./linux-healthy-agent
```

Or use CLI:

```bash
./linux-healthy-agent --instance-name prod-gpu-eu-01
```

Alert messages include:

- `machine`
- `hostname`
- `kernel`
- `machine_id`

Alert policy:

- `critical`: sent immediately every time.
- `warning`: throttled to once every 3600 seconds by default.

Change warning throttle:

```bash
./linux-healthy-agent --warning-alert-interval-seconds 1800
```

## systemd Timer

Examples are under `examples/systemd/`.

Use an environment file for the webhook:

```bash
sudo install -m 0755 linux-healthy-agent /usr/local/bin/linux-healthy-agent
sudo install -m 0644 examples/systemd/linux-healthy-agent.service \
  /etc/systemd/system/linux-healthy-agent.service
sudo install -m 0644 examples/systemd/linux-healthy-agent.timer \
  /etc/systemd/system/linux-healthy-agent.timer
sudo install -m 0600 /dev/null /etc/linux-healthy-agent.env
sudoedit /etc/linux-healthy-agent.env
```

`/etc/linux-healthy-agent.env` example:

```bash
FEISHU_WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/REPLACE_ME
LINUX_HEALTHY_AGENT_INSTANCE_NAME=prod-gpu-eu-01
```

Enable the timer:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now linux-healthy-agent.timer
```

With the installer:

```bash
sudo sh scripts/install.sh --with-systemd
```

Then edit the local environment file:

```bash
sudoedit /etc/linux-healthy-agent.env
```

Do not commit real webhook URLs to Git.

## Release

Maintainers can create a release by pushing a tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The release workflow uploads:

- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz`
- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz.sha256`

## Exit Code

- `0`: ok
- `1`: warning
- `2`: critical

## Development

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

MIT
