#!/bin/sh
set -eu

REPO="lauzhihao/linux-healthy-agent-with-feishu-hooks"
BINARY_NAME="linux-healthy-agent"
TARGET="x86_64-unknown-linux-musl"
PREFIX="/usr/local"
VERSION="${VERSION:-latest}"
WITH_SYSTEMD=0
FORCE=0

usage() {
    cat <<'USAGE'
Linux Healthy Agent installer

Usage:
  install.sh [options]

Options:
  --version <version>     Install a specific release tag, for example v0.1.0.
  --prefix <path>         Install prefix. Default: /usr/local.
  --with-systemd          Install and enable systemd timer.
  --force                 Overwrite existing binary.
  -h, --help              Show this help.

Environment:
  VERSION                 Same as --version.

Examples:
  curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh | sudo sh
  curl -fsSL https://raw.githubusercontent.com/lauzhihao/linux-healthy-agent-with-feishu-hooks/main/scripts/install.sh | sudo sh -s -- --with-systemd
USAGE
}

log() {
    printf '%s\n' "$*"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || fail "--version requires a value"
            VERSION="$2"
            shift 2
            ;;
        --prefix)
            [ "$#" -ge 2 ] || fail "--prefix requires a value"
            PREFIX="$2"
            shift 2
            ;;
        --with-systemd)
            WITH_SYSTEMD=1
            shift
            ;;
        --force)
            FORCE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

OS="$(uname -s)"
ARCH="$(uname -m)"
[ "$OS" = "Linux" ] || fail "only Linux is supported"
case "$ARCH" in
    x86_64|amd64)
        ;;
    *)
        fail "unsupported architecture: $ARCH"
        ;;
esac

need_cmd curl
need_cmd head
need_cmd id
need_cmd mktemp
need_cmd sed
need_cmd tar
need_cmd sha256sum
need_cmd install

if [ "$VERSION" = "latest" ]; then
    VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' \
        | head -n 1)"
    [ -n "$VERSION" ] || fail "failed to resolve latest release"
fi

ASSET_BASE="${BINARY_NAME}-${TARGET}"
ARCHIVE="${ASSET_BASE}.tar.gz"
SHA_FILE="${ARCHIVE}.sha256"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

log "Installing ${BINARY_NAME} ${VERSION} for ${TARGET}"
curl -fsSL "${BASE_URL}/${ARCHIVE}" -o "${TMP_DIR}/${ARCHIVE}"
curl -fsSL "${BASE_URL}/${SHA_FILE}" -o "${TMP_DIR}/${SHA_FILE}"

(
    cd "$TMP_DIR"
    sha256sum -c "$SHA_FILE"
)

tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$TMP_DIR"
[ -x "${TMP_DIR}/${BINARY_NAME}" ] || fail "archive did not contain executable ${BINARY_NAME}"

DEST_DIR="${PREFIX}/bin"
DEST="${DEST_DIR}/${BINARY_NAME}"
if [ -e "$DEST" ] && [ "$FORCE" -ne 1 ]; then
    fail "$DEST already exists; pass --force to overwrite"
fi

install -d "$DEST_DIR"
install -m 0755 "${TMP_DIR}/${BINARY_NAME}" "$DEST"
log "Installed $DEST"

if [ "$WITH_SYSTEMD" -eq 1 ]; then
    [ "$(id -u)" -eq 0 ] || fail "--with-systemd requires root"
    command -v systemctl >/dev/null 2>&1 || fail "systemctl not found"
    install -d /etc/systemd/system
    cat >/etc/systemd/system/linux-healthy-agent.service <<SERVICE
[Unit]
Description=Linux Healthy Agent
Documentation=https://github.com/lauzhihao/linux-healthy-agent-with-feishu-hooks

[Service]
Type=oneshot
Environment=LINUX_HEALTHY_AGENT_OUTPUT_FILE=/run/linux-healthy-agent/latest.json
EnvironmentFile=-/etc/linux-healthy-agent.env
ExecStart=${DEST} --output-file \${LINUX_HEALTHY_AGENT_OUTPUT_FILE}
Nice=10
IOSchedulingClass=best-effort
IOSchedulingPriority=7
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/run /mnt/host-fleet
SERVICE
    cat >/etc/systemd/system/linux-healthy-agent.timer <<'TIMER'
[Unit]
Description=Run Linux Healthy Agent periodically

[Timer]
OnBootSec=1min
OnUnitActiveSec=1min
AccuracySec=10s
Unit=linux-healthy-agent.service

[Install]
WantedBy=timers.target
TIMER
    if [ ! -e /etc/linux-healthy-agent.env ]; then
        umask 077
        cat >/etc/linux-healthy-agent.env <<'ENVFILE'
# Optional alert mode. Disabled unless LINUX_HEALTHY_AGENT_ENABLE_ALERTS is true.
# Do not commit real webhook URLs to Git.
# LINUX_HEALTHY_AGENT_ENABLE_ALERTS=false
# FEISHU_WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/REPLACE_ME
# LINUX_HEALTHY_AGENT_ALERT_THRESHOLDS_FILE=/etc/linux-healthy-agent-alert-thresholds.json
# LINUX_HEALTHY_AGENT_ALERT_STATE_FILE=/run/linux-healthy-agent-alert.json
# Optional. Used as the machine label in Feishu alerts.
# LINUX_HEALTHY_AGENT_INSTANCE_NAME=prod-gpu-eu-01
# Optional. Used by Host Fleet snapshot aggregation.
# LINUX_HEALTHY_AGENT_HOST_ID=aws-eu-south-2-gpu-01
# LINUX_HEALTHY_AGENT_PROVIDER=aws
# LINUX_HEALTHY_AGENT_CLOUD_REGION=eu-south-2
# LINUX_HEALTHY_AGENT_ZONE=eu-south-2a
# LINUX_HEALTHY_AGENT_FLEET_REGION=EU
# LINUX_HEALTHY_AGENT_ROLE=GPU · Inference
# Optional. Set to an object-storage mounted path for Host Fleet publishing.
# LINUX_HEALTHY_AGENT_OUTPUT_FILE=/mnt/host-fleet/raw/linux-health/aws/eu-south-2/aws-eu-south-2-gpu-01/latest.json
ENVFILE
    fi
    systemctl daemon-reload
    systemctl enable --now linux-healthy-agent.timer
    log "Installed and enabled linux-healthy-agent.timer"
fi

"$DEST" --version
