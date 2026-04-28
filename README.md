# Linux Healthy Agent

[English](README.en.md)

Linux Healthy Agent 是一个只读、低扰动的 Linux 指标快照采集器。默认模式只
输出所在机器的瞬时原始指标，不判断健康程度，不输出 warning/critical。
飞书机器人 webhook 告警是显式开启的可选模式。

## 特性

- Rust 编写，可构建为单个静态 binary。
- 默认只读，不修改服务状态，不安装依赖，不执行写入型压测。
- 采集 CPU、load、CPU PSI、内存、磁盘容量、磁盘 I/O、网络错误/丢包。
- GPU 自适应：有 NVIDIA GPU 就采集，没有 GPU 默认不记录错误。
- 支持 G7E、A100、H100 等 NVIDIA GPU 机器。
- Docker 健康检查默认开启，只采集异常摘要，不输出所有正常容器。
- 可选飞书 webhook 告警；开启时必须显式配置 webhook 和完整阈值文件。
- 输出稳定 JSON，适合 systemd timer、cron、日志系统或二次集成。

## 安全边界

默认运行不会：

- 写磁盘数据或创建临时日志。
- 暂停、冻结、中断、restart、pause 业务进程或容器。
- attach 业务进程，例如 `ptrace`、`gdb`、`strace`、`perf`。
- 写 `/proc/sys`、`/sys`、cgroup 控制文件。
- 调用 Docker 控制操作。

会读取：

- `/proc/stat`
- `/proc/loadavg`
- `/proc/meminfo`
- `/proc/diskstats`
- `/proc/net/dev`
- `/proc/pressure/*`
- 挂载点 `statvfs`
- 可选 `nvidia-smi`
- 默认一次轻量 `docker ps --all --format "{{json .}}"`

## 一键安装

推荐服务器直接安装 Release 中的静态 binary，无需安装 Rust 或从源码编译：

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh | sudo sh
```

安装指定版本：

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --version v0.1.4
```

覆盖已有 binary：

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --force
```

安装并启用 systemd timer：

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --with-systemd
```

安装脚本会：

- 检查系统为 Linux x86_64。
- 下载 GitHub Release 中的 `x86_64-unknown-linux-musl` 静态 binary。
- 下载并校验 SHA256。
- 安装到 `/usr/local/bin/linux-healthy-agent`。
- 不写入任何真实 webhook。
- 只有传 `--with-systemd` 时才安装 systemd timer。

更谨慎的安装方式：

```bash
curl -fsSLO https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh
less install.sh
sudo sh install.sh
```

## 从源码构建

安装 Rust 后：

```bash
cargo build --release
```

构建 Linux x86_64 静态 binary：

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

产物：

```bash
target/x86_64-unknown-linux-musl/release/linux-healthy-agent
```

验证静态链接：

```bash
file target/x86_64-unknown-linux-musl/release/linux-healthy-agent
ldd target/x86_64-unknown-linux-musl/release/linux-healthy-agent
```

预期 `ldd` 输出 `statically linked` 或 `not a dynamic executable`。

## 快速开始

默认运行：

```bash
./linux-healthy-agent
```

美化 JSON 输出：

```bash
./linux-healthy-agent --pretty
```

极低扰动基础巡检：

```bash
./linux-healthy-agent --gpu-mode disabled --skip-docker --top-processes 0 \
  --interval 1 --samples 1
```

## GPU 模式

默认：

```bash
./linux-healthy-agent --gpu-mode auto
```

模式说明：

- `auto`：有 NVIDIA GPU 就采集，没有 GPU 不记录错误。
- `required`：GPU 必须存在，否则在 `errors.gpu` 中记录采集错误。
- `disabled`：完全跳过 GPU，不调用 `nvidia-smi`。

GPU 机器建议：

```bash
./linux-healthy-agent --gpu-mode required
```

GPU 进程归因默认关闭，需要排查时开启：

```bash
./linux-healthy-agent --gpu-processes
```

## Docker 状态摘要

Docker 状态摘要默认开启，仅执行一次：

```bash
docker ps --all --format "{{json .}}"
```

默认 JSON 只输出容器状态摘要，不判断主机或容器健康程度。正常 running 容器不
会逐条输出。

完全跳过 Docker：

```bash
./linux-healthy-agent --skip-docker
```

## 飞书告警

默认关闭。只有显式传入 `--enable-alerts` 或设置
`LINUX_HEALTHY_AGENT_ENABLE_ALERTS=true` 后才会读取阈值并发送飞书告警。
开启后必须同时配置 webhook 和完整阈值文件，缺失任意一项都会启动失败。

不要把 webhook 写入代码或提交到 Git。推荐使用环境变量：

```bash
export LINUX_HEALTHY_AGENT_ENABLE_ALERTS=true
export FEISHU_WEBHOOK_URL="https://open.feishu.cn/open-apis/bot/v2/hook/..."
export LINUX_HEALTHY_AGENT_ALERT_THRESHOLDS_FILE=/etc/linux-healthy-agent-alert-thresholds.json
./linux-healthy-agent --alert-state-file /run/linux-healthy-agent-alert.json
```

也可以使用 CLI 参数：

```bash
./linux-healthy-agent \
  --enable-alerts \
  --webhook-url "$FEISHU_WEBHOOK_URL" \
  --alert-thresholds-file /etc/linux-healthy-agent-alert-thresholds.json
```

阈值文件必须显式列出所有字段。仓库内提供
`examples/linux-healthy-agent-alert-thresholds.json`，内容形如：

```json
{
  "cpu_busy_warning": 85.0,
  "cpu_busy_critical": 95.0,
  "load_per_cpu_warning": 1.0,
  "load_per_cpu_critical": 1.5,
  "psi_avg60_warning": 10.0,
  "psi_avg60_critical": 20.0,
  "mem_available_warning_percent": 10.0,
  "mem_available_critical_percent": 5.0,
  "mem_available_warning_gib": 64.0,
  "mem_available_critical_gib": 32.0,
  "mem_absolute_floor_min_total_gib": 256.0,
  "disk_usage_warning": 80.0,
  "disk_usage_critical": 90.0,
  "io_limit_warning_ratio": 0.8,
  "io_limit_critical_ratio": 0.9,
  "gpu_memory_warning": 85.0,
  "gpu_memory_critical": 95.0,
  "gpu_util_warning": 90.0,
  "gpu_util_critical": 98.0,
  "gpu_temp_warning": 80.0,
  "gpu_temp_critical": 90.0,
  "gpu_idle_memory_percent": 80.0,
  "gpu_idle_util_percent": 10.0,
  "network_errors_warning_delta": 1,
  "network_errors_critical_delta": 10,
  "docker_unhealthy_warning_count": 1,
  "docker_unhealthy_critical_count": 1,
  "docker_restarting_warning_count": 1,
  "docker_restarting_critical_count": 1,
  "docker_exited_warning_count": 1,
  "docker_exited_critical_count": 10,
  "docker_other_abnormal_warning_count": 1,
  "docker_other_abnormal_critical_count": 10
}
```

多台机器共用同一个飞书机器人时，建议设置机器标识：

```bash
export LINUX_HEALTHY_AGENT_INSTANCE_NAME="prod-gpu-eu-01"
./linux-healthy-agent
```

也可以使用 CLI 参数：

```bash
./linux-healthy-agent --instance-name prod-gpu-eu-01
```

## Host Fleet 快照

如果要把探针结果同步到对象存储，建议把对象存储挂载为本地目录，
然后让 agent 直接把最新快照写到挂载目录中：

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

等价环境变量：

```bash
LINUX_HEALTHY_AGENT_HOST_ID=aws-eu-south-2-gpu-01
LINUX_HEALTHY_AGENT_PROVIDER=aws
LINUX_HEALTHY_AGENT_CLOUD_REGION=eu-south-2
LINUX_HEALTHY_AGENT_ZONE=eu-south-2a
LINUX_HEALTHY_AGENT_FLEET_REGION=EU
LINUX_HEALTHY_AGENT_ROLE=GPU · Inference
```

`--output-file` 使用同目录临时文件 + rename 写入，stdout JSON 输出仍会保留。

如果使用 systemd 示例中的 sandbox 配置，需要把挂载目录加入
`ReadWritePaths`；当前示例已预留 `/mnt/host-fleet`。上线前要验证挂载层支持同目录临时文件写入和 rename。
如果 `--output-file` 指向挂载目录，建议在 service/drop-in 中加入
`RequiresMountsFor=/mnt/host-fleet`，避免挂载缺失时写到本机磁盘。
agent 不提供直传对象存储能力；监控页找不到数据时，优先检查挂载、路径、
权限和整理程序。

飞书告警消息会包含：

- `machine`
- `hostname`
- `kernel`
- `machine_id`

告警策略：

- `critical`：检测到即发送，不限频。
- `warning`：默认每 3600 秒最多发送一次。

修改 warning 限频：

```bash
./linux-healthy-agent --warning-alert-interval-seconds 1800
```

## systemd timer

示例文件在 `examples/systemd/`。

推荐把 webhook 放入环境文件：

```bash
sudo install -m 0755 linux-healthy-agent /usr/local/bin/linux-healthy-agent
sudo install -m 0644 examples/systemd/linux-healthy-agent.service \
  /etc/systemd/system/linux-healthy-agent.service
sudo install -m 0644 examples/systemd/linux-healthy-agent.timer \
  /etc/systemd/system/linux-healthy-agent.timer
sudo install -m 0600 /dev/null /etc/linux-healthy-agent.env
sudoedit /etc/linux-healthy-agent.env
```

`/etc/linux-healthy-agent.env` 示例：

```bash
# 默认关闭告警。开启时必须同时配置 webhook 和完整阈值文件。
# LINUX_HEALTHY_AGENT_ENABLE_ALERTS=true
# FEISHU_WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/REPLACE_ME
# LINUX_HEALTHY_AGENT_ALERT_THRESHOLDS_FILE=/etc/linux-healthy-agent-alert-thresholds.json
# LINUX_HEALTHY_AGENT_ALERT_STATE_FILE=/run/linux-healthy-agent-alert.json
LINUX_HEALTHY_AGENT_INSTANCE_NAME=prod-gpu-eu-01
```

启用：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now linux-healthy-agent.timer
```

如果使用一键安装：

```bash
sudo sh scripts/install.sh --with-systemd
```

然后编辑本机环境文件：

```bash
sudoedit /etc/linux-healthy-agent.env
```

不要把真实 webhook 提交到 Git。

## 发布 Release

维护者打 tag 后会自动构建 Release asset：

```bash
git tag v0.1.4
git push origin v0.1.4
```

Release workflow 会上传：

- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz`
- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz.sha256`

## Exit Code

- `0`：采集、序列化和写入成功
- `2`：配置错误、采集失败、序列化失败或写入失败

默认模式不会因为 CPU、内存、磁盘、GPU 等指标数值返回非 0；这些阈值判断归属
前端或显式开启的告警模式。

## 开发

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

MIT
