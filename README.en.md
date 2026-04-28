# Linux Healthy Agent

[中文](README.md)

Linux Healthy Agent is a read-only, low-overhead Linux metric snapshot agent.
By default it only emits raw point-in-time metrics for the local host. It does
not judge health and does not emit warning/critical status. Feishu bot webhook
alerts are an explicit optional mode.

## Features

- Written in Rust and can be built as a single static binary.
- Read-only by default. It does not modify service state or run write tests.
- Collects CPU, load, CPU PSI, memory, disk usage, disk I/O, and network
  errors/drops.
- Adaptive GPU mode: collects NVIDIA GPU metrics when present and does not
  record an error when no GPU exists by default.
- Works with NVIDIA GPU hosts such as G7E, A100, and H100.
- Docker status summary is enabled by default and only reports abnormal
  container summaries.
- Optional Feishu webhook alerts. Alert mode requires a webhook and a complete
  thresholds file.
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

## One-Line Install

For servers, install the prebuilt static binary from GitHub Releases. No Rust
toolchain or source build is required:

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh | sudo sh
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --version v0.1.4
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

- `auto`: collect NVIDIA GPU metrics when present; do not record an error when
  no GPU exists.
- `required`: GPU must exist, otherwise record a collection error in
  `errors.gpu`.
- `disabled`: skip GPU checks and do not call `nvidia-smi`.

Recommended for GPU hosts:

```bash
./linux-healthy-agent --gpu-mode required
```

GPU process attribution is disabled by default:

```bash
./linux-healthy-agent --gpu-processes
```

## Docker Status

Docker status summary is enabled by default and runs once:

```bash
docker ps --all --format "{{json .}}"
```

Default JSON only emits a container status summary. It does not judge host or
container health. Healthy running containers are not listed one by one.

Disable Docker checks:

```bash
./linux-healthy-agent --skip-docker
```

## Feishu Alerts

Disabled by default. The agent reads thresholds and sends Feishu alerts only
when `--enable-alerts` or `LINUX_HEALTHY_AGENT_ENABLE_ALERTS=true` is set.
When alerts are enabled, both a webhook and a complete thresholds file are
required; missing either one makes startup fail.

Never commit webhook URLs to Git. Use environment variables:

```bash
export LINUX_HEALTHY_AGENT_ENABLE_ALERTS=true
export FEISHU_WEBHOOK_URL="https://open.feishu.cn/open-apis/bot/v2/hook/..."
export LINUX_HEALTHY_AGENT_ALERT_THRESHOLDS_FILE=/etc/linux-healthy-agent-alert-thresholds.json
./linux-healthy-agent --alert-state-file /run/linux-healthy-agent-alert.json
```

Or pass the settings with CLI:

```bash
./linux-healthy-agent \
  --enable-alerts \
  --webhook-url "$FEISHU_WEBHOOK_URL" \
  --alert-thresholds-file /etc/linux-healthy-agent-alert-thresholds.json
```

The thresholds file must list every field explicitly. See
`examples/linux-healthy-agent-alert-thresholds.json`.

When multiple machines share one Feishu bot, set a machine identity:

```bash
export LINUX_HEALTHY_AGENT_INSTANCE_NAME="prod-gpu-eu-01"
./linux-healthy-agent
```

Or use CLI:

```bash
./linux-healthy-agent --instance-name prod-gpu-eu-01
```

## Host Fleet snapshots

For object storage publishing, mount the object-storage prefix as a local
directory and let the agent write the latest snapshot directly into that mount:

```bash
./linux-healthy-agent \
  --host-id aws-eu-south-2-gpu-01 \
  --provider aws \
  --cloud-region eu-south-2 \
  --zone eu-south-2a \
  --fleet-region EU \
  --role "GPU · Inference" \
  --output-file /mnt/host-fleet/raw/linux-health/aws/eu-south-2/aws-eu-south-2-gpu-01/latest.json
```

Equivalent environment variables:

```bash
LINUX_HEALTHY_AGENT_HOST_ID=aws-eu-south-2-gpu-01
LINUX_HEALTHY_AGENT_PROVIDER=aws
LINUX_HEALTHY_AGENT_CLOUD_REGION=eu-south-2
LINUX_HEALTHY_AGENT_ZONE=eu-south-2a
LINUX_HEALTHY_AGENT_FLEET_REGION=EU
LINUX_HEALTHY_AGENT_ROLE=GPU · Inference
```

`--output-file` writes through a temporary file in the same directory and
renames it into place. The stdout JSON output is preserved.

With the example systemd sandbox settings, add the mount path to
`ReadWritePaths`; the current example already includes `/mnt/host-fleet`.
Before rollout, verify that the mount supports same-directory temporary writes
and rename. If `--output-file` points at the mount, add
`RequiresMountsFor=/mnt/host-fleet` in the service or drop-in so missing mounts
fail visibly instead of writing to local disk. The agent does not provide direct
object-storage upload support; when the dashboard cannot find data, check the
mount, path, permissions, and aggregation job first.

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
# Alerts are disabled by default. Enabling alerts also requires a webhook and a
# complete thresholds file.
# LINUX_HEALTHY_AGENT_ENABLE_ALERTS=true
# FEISHU_WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/REPLACE_ME
# LINUX_HEALTHY_AGENT_ALERT_THRESHOLDS_FILE=/etc/linux-healthy-agent-alert-thresholds.json
# LINUX_HEALTHY_AGENT_ALERT_STATE_FILE=/run/linux-healthy-agent-alert.json
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
git tag v0.1.4
git push origin v0.1.4
```

The release workflow uploads:

- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz`
- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz.sha256`

## Exit Code

- `0`: collection, serialization, and output write succeeded
- `2`: configuration error, collection failure, serialization failure, or output
  write failure

Default mode does not return non-zero because CPU, memory, disk, GPU, or other
metric values crossed thresholds. Those threshold decisions belong to the
frontend or to explicitly enabled alert mode.

## Development

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

MIT
