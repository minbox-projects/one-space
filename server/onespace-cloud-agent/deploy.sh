#!/usr/bin/env bash
# onespace-cloud-agent 远程部署与同步脚本
set -euo pipefail

REMOTE_HOST="${1:-dmit-server}"
REMOTE_DEST="${2:-/root/onespace-cloud-agent}"
LOCAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE_STAGING="${REMOTE_DEST}-staging"
REMOTE_BACKUPS="${REMOTE_DEST}-backups"
KEEP_SNAPSHOTS=3
SSH_OPTS=(-o BatchMode=yes -o ConnectTimeout=10)

phase_start() {
    printf 'ONESPACE_DEPLOY_PHASE phase=%s status=start\n' "$1"
}

phase_ok() {
    printf 'ONESPACE_DEPLOY_PHASE phase=%s status=ok\n' "$1"
}

phase_error() {
    printf 'ONESPACE_DEPLOY_PHASE phase=%s status=error reason=%s\n' "$1" "$2"
    exit 1
}

cleanup_staging() {
    ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" "rm -rf '${REMOTE_STAGING}'" >/dev/null 2>&1 || true
}

trap cleanup_staging EXIT

if [ -z "$REMOTE_HOST" ] || [ "$REMOTE_DEST" = "/" ]; then
    phase_start platform
    phase_error platform invalid_target
fi

VERSION_VALUE="$(tr -d '[:space:]' < "${LOCAL_DIR}/VERSION")"
if [ -z "$VERSION_VALUE" ]; then
    phase_start platform
    phase_error platform invalid_version
fi

# ---------------------------------------------------------------------------
# Platform prerequisites
# ---------------------------------------------------------------------------
phase_start platform
if ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "BOOTSTRAP_PHASE=platform OS_RELEASE=/etc/os-release bash -s" \
    < "${LOCAL_DIR}/bootstrap.sh"; then
    phase_ok platform
else
    status=$?
    case "$status" in
        10) reason=platform_linux_required ;;
        11) reason=platform_root_required ;;
        12) reason=platform_systemd_required ;;
        13) reason=platform_arch_unsupported ;;
        14) reason=platform_distro_unsupported ;;
        *) reason=remote_unreachable ;;
    esac
    phase_error platform "$reason"
fi

# ---------------------------------------------------------------------------
# Runtime dependencies
# ---------------------------------------------------------------------------
phase_start dependencies
if ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "BOOTSTRAP_PHASE=dependencies OS_RELEASE=/etc/os-release bash -s" \
    < "${LOCAL_DIR}/bootstrap.sh"; then
    phase_ok dependencies
else
    status=$?
    case "$status" in
        10) reason=platform_linux_required ;;
        11) reason=platform_root_required ;;
        12) reason=platform_systemd_required ;;
        13) reason=platform_arch_unsupported ;;
        14) reason=platform_distro_unsupported ;;
        *) reason=dependency_install_failed ;;
    esac
    phase_error dependencies "$reason"
fi

# ---------------------------------------------------------------------------
# Upload the owned payload into same-filesystem staging
# ---------------------------------------------------------------------------
phase_start upload
if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "rm -rf '${REMOTE_STAGING}' && mkdir -p '${REMOTE_STAGING}/core' '${REMOTE_STAGING}/systemd'"; then
    phase_error upload staging_unavailable
fi

BASE_FILES=(
    "deploy.sh"
    "bootstrap.sh"
    "VERSION"
    "config.default.json"
    "config.example.json"
    "agent.py"
)
CORE_FILES=(
    "core/__init__.py"
    "core/disk_guardian.py"
    "core/gateway.py"
    "core/issue_worker.py"
    "core/notifier.py"
    "core/session_runner.py"
)

base_sources=()
for relative in "${BASE_FILES[@]}"; do
    base_sources+=("${LOCAL_DIR}/${relative}")
done
core_sources=()
for relative in "${CORE_FILES[@]}"; do
    core_sources+=("${LOCAL_DIR}/${relative}")
done

if ! scp -q -B "${base_sources[@]}" "${REMOTE_HOST}:${REMOTE_STAGING}/"; then
    phase_error upload upload_failed
fi
if ! scp -q -B "${core_sources[@]}" "${REMOTE_HOST}:${REMOTE_STAGING}/core/"; then
    phase_error upload upload_failed
fi
if ! scp -q -B "${LOCAL_DIR}/systemd/onespace-cloud-agent.service" \
    "${REMOTE_HOST}:${REMOTE_STAGING}/systemd/"; then
    phase_error upload upload_failed
fi
phase_ok upload

# ---------------------------------------------------------------------------
# Validate or prepare the remote configuration
# ---------------------------------------------------------------------------
phase_start config
if ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" "test -f '${REMOTE_DEST}/config.json'"; then
    FIRST_DEPLOY=0
else
    status=$?
    if [ "$status" -ne 1 ]; then
        phase_error config remote_config_check_failed
    fi
    FIRST_DEPLOY=1
fi

if [ "$FIRST_DEPLOY" -eq 0 ]; then
    if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
        "/usr/bin/python3 -m json.tool '${REMOTE_DEST}/config.json' >/dev/null 2>&1"; then
        phase_error config config_invalid
    fi
    if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
        "cp -p '${REMOTE_DEST}/config.json' '${REMOTE_STAGING}/config.json'"; then
        phase_error config config_copy_failed
    fi
else
    if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
        "cp -p '${REMOTE_STAGING}/config.default.json' '${REMOTE_STAGING}/config.json'"; then
        phase_error config config_create_failed
    fi
fi
phase_ok config

# ---------------------------------------------------------------------------
# Publish the staged payload, launchers and unit
# ---------------------------------------------------------------------------
phase_start publish
if ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "REMOTE_DEST='${REMOTE_DEST}' REMOTE_STAGING='${REMOTE_STAGING}' REMOTE_BACKUPS='${REMOTE_BACKUPS}' FIRST_DEPLOY='${FIRST_DEPLOY}' bash -s" <<'PUBLISH'
set -e
if [ "$FIRST_DEPLOY" = "0" ] && [ -d "$REMOTE_DEST" ]; then
    mkdir -p "$REMOTE_BACKUPS"
    snapshot="$REMOTE_BACKUPS/$(date +%Y%m%d%H%M%S)-$$"
    mkdir -p "$snapshot"
    cp -a "$REMOTE_DEST/." "$snapshot/"
    rm -rf "$REMOTE_DEST"
    if ! mv "$REMOTE_STAGING" "$REMOTE_DEST"; then
        mkdir -p "$REMOTE_DEST"
        cp -a "$snapshot/." "$REMOTE_DEST/"
        exit 2
    fi
else
    rm -rf "$REMOTE_DEST"
    if ! mv "$REMOTE_STAGING" "$REMOTE_DEST"; then
        exit 3
    fi
fi
PUBLISH
then
    :
else
    status=$?
    if [ "$status" -eq 2 ] || [ "$status" -eq 3 ]; then
        phase_error publish promotion_failed
    fi
    phase_error publish publish_failed
fi

if printf '%s' "$REMOTE_DEST" | grep -q '[[:space:]]'; then
    unit_exec="/usr/bin/python3 \"${REMOTE_DEST}/agent.py\" \"${REMOTE_DEST}/config.json\""
else
    unit_exec="/usr/bin/python3 ${REMOTE_DEST}/agent.py ${REMOTE_DEST}/config.json"
fi

UNIT_CONTENT="$(cat <<EOF
[Unit]
Description=OneSpace Cloud Agent (Gateway & Auto-PR Runner)
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=${REMOTE_DEST}
Environment=PATH=/root/.opencode/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
ExecStart=${unit_exec}
Restart=always
RestartSec=5
KillMode=mixed
TimeoutStopSec=infinity
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF
)"

CLOUD_SESSION_CONTENT="$(cat <<EOF
#!/usr/bin/env bash
exec /usr/bin/python3 '${REMOTE_DEST}/core/session_runner.py' "\$@"
EOF
)"

OC_CONTENT="$(cat <<EOF
#!/usr/bin/env bash
export ONESPACE_CLOUD_AGENT_DIR='${REMOTE_DEST}'
export PATH="/root/.opencode/bin:\$PATH"
exec opencode --yolo "\$@"
EOF
)"

if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "cat > '/etc/systemd/system/onespace-cloud-agent.service'" <<< "$UNIT_CONTENT"; then
    phase_error publish unit_install_failed
fi
if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "cat > '/usr/local/bin/cloud-session' && chmod 755 '/usr/local/bin/cloud-session'" \
    <<< "$CLOUD_SESSION_CONTENT"; then
    phase_error publish launcher_install_failed
fi
if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "cat > '/usr/local/bin/oc' && chmod 755 '/usr/local/bin/oc'" \
    <<< "$OC_CONTENT"; then
    phase_error publish launcher_install_failed
fi
phase_ok publish

# ---------------------------------------------------------------------------
# Start the service and confirm it is actually active
# ---------------------------------------------------------------------------
phase_start service
if ! ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "systemctl daemon-reload && systemctl enable onespace-cloud-agent.service && systemctl restart onespace-cloud-agent.service"; then
    phase_error service service_start_failed
fi

active_state=""
if ! active_state="$(ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "systemctl is-active onespace-cloud-agent.service" 2>/dev/null)"; then
    active_state=""
fi
if [ "$active_state" != "active" ]; then
    phase_error service service_inactive
fi
phase_ok service

# ---------------------------------------------------------------------------
# Publish installation metadata only after successful activation
# ---------------------------------------------------------------------------
phase_start metadata
if ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "REMOTE_DEST='${REMOTE_DEST}' INSTALL_INFO='${REMOTE_DEST}/install-info.json' VERSION_VALUE='${VERSION_VALUE}' bash -s" <<'METADATA'
set -e
tmp="$(mktemp "${REMOTE_DEST}/.install-info.XXXXXX")"
trap 'rm -f "$tmp"' EXIT
printf '{"version":"%s","installed_at":"%s"}\n' \
    "$VERSION_VALUE" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$tmp"
mv "$tmp" "$INSTALL_INFO"
trap - EXIT
METADATA
then
    phase_ok metadata
else
    phase_error metadata metadata_write_failed
fi

# ---------------------------------------------------------------------------
# Retain only the newest successful snapshots
# ---------------------------------------------------------------------------
phase_start cleanup
if ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" \
    "REMOTE_BACKUPS='${REMOTE_BACKUPS}' KEEP_SNAPSHOTS='${KEEP_SNAPSHOTS}' bash -s" <<'PRUNE'
if [ -d "$REMOTE_BACKUPS" ]; then
    count=0
    ls -1dt "$REMOTE_BACKUPS"/* 2>/dev/null | while IFS= read -r snapshot; do
        count=$((count + 1))
        if [ "$count" -gt "$KEEP_SNAPSHOTS" ]; then
            rm -rf "$snapshot"
        fi
    done
fi
PRUNE
then
    :
fi
phase_ok cleanup
