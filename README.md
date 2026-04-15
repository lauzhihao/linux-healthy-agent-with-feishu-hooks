# Linux Healthy Agent

[English](README.en.md)

Linux Healthy Agent 是一个只读、低扰动的 Linux 健康探针，内置飞书机器人
webhook 告警能力。它适合部署在普通 EC2、GPU 实例、裸机或容器宿主机上，
用于发现基础资源压力、GPU 空占、Docker 容器异常等问题。

## 特性

- Rust 编写，可构建为单个静态 binary。
- 默认只读，不修改服务状态，不安装依赖，不执行写入型压测。
- 采集 CPU、load、CPU PSI、内存、磁盘容量、磁盘 I/O、网络错误/丢包。
- GPU 自适应：有 NVIDIA GPU 就检查，没有 GPU 默认不告警。
- 支持 G7E、A100、H100 等 NVIDIA GPU 机器。
- Docker 健康检查默认开启，只采集异常摘要，不输出所有正常容器。
- 飞书 webhook 告警，支持 warning 限频和 critical 即时告警。
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

warning 告警限频需要状态文件。建议放在 tmpfs，例如 `/run`，避免写入磁盘：

```bash
--alert-state-file /run/linux-healthy-agent-alert.json
```

## 一键安装

推荐服务器直接安装 Release 中的静态 binary，无需安装 Rust 或从源码编译：

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh | sudo sh
```

安装指定版本：

```bash
curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh \
  | sudo sh -s -- --version v0.1.0
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

- `auto`：有 NVIDIA GPU 就检查，没有 GPU 不告警。
- `required`：GPU 必须存在，否则 critical。
- `disabled`：完全跳过 GPU，不调用 `nvidia-smi`。

GPU 机器建议：

```bash
./linux-healthy-agent --gpu-mode required
```

GPU 进程归因默认关闭，需要排查时开启：

```bash
./linux-healthy-agent --gpu-processes
```

## Docker 健康检查

Docker 健康检查默认开启，仅执行一次：

```bash
docker ps --all --format "{{json .}}"
```

告警规则：

- `unhealthy`：critical。
- `restarting`：critical。
- `exited`、`dead`、其他非 running：warning。
- 正常 running 容器不会逐条输出。

完全跳过 Docker：

```bash
./linux-healthy-agent --skip-docker
```

## 飞书告警

不要把 webhook 写入代码或提交到 Git。推荐使用环境变量：

```bash
export FEISHU_WEBHOOK_URL="https://open.feishu.cn/open-apis/bot/v2/hook/..."
./linux-healthy-agent --alert-state-file /run/linux-healthy-agent-alert.json
```

也可以使用 CLI 参数：

```bash
./linux-healthy-agent --webhook-url "$FEISHU_WEBHOOK_URL"
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

告警消息会包含：

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
FEISHU_WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/REPLACE_ME
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
git tag v0.1.0
git push origin v0.1.0
```

Release workflow 会上传：

- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz`
- `linux-healthy-agent-x86_64-unknown-linux-musl.tar.gz.sha256`

## Exit Code

- `0`：ok
- `1`：warning
- `2`：critical

## 开发

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

MIT
