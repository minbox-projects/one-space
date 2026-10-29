#!/usr/bin/env bash
# onespace-cloud-agent 一键远程部署与同步脚本
set -euo pipefail

REMOTE_HOST="${1:-dmit-server}"
LOCAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE_DEST="/root/onespace-cloud-agent"
REMOTE_STAGING="/root/onespace-cloud-agent-staging"
REMOTE_BACKUPS="/root/onespace-cloud-agent-backups"
KEEP_SNAPSHOTS=3

echo "=================================================="
echo "   OneSpace Cloud Agent 部署工具 (目标: ${REMOTE_HOST})   "
echo "=================================================="

cleanup_staging() {
    ssh "${REMOTE_HOST}" "rm -rf '${REMOTE_STAGING}'" >/dev/null 2>&1 || true
}
trap cleanup_staging EXIT

# 1. 连通性预检
echo "[1/7] 检查 SSH 连通性..."
if ! ssh -o ConnectTimeout=5 "${REMOTE_HOST}" "uname -m" > /dev/null 2>&1; then
    echo "错误: 无法免密连接到 ${REMOTE_HOST}，请检查 ~/.ssh/config 配置。" >&2
    exit 1
fi
echo "SSH 连通正常。"

# 2. 远端依赖预检：使用服务 PATH 中的工具目录，缺失即失败
echo "[2/7] 检查远端 python3、git、tmux、gh 与 opencode..."
if ! ssh "${REMOTE_HOST}" <<'EOF'
old_ifs="$IFS"
IFS=:
first_entry=""
for entry in $PATH; do
    if [ -n "$entry" ]; then
        first_entry="$entry"
        break
    fi
done
IFS="$old_ifs"
case "$first_entry" in
    /root/*|/usr/local/*|/usr/sbin|/usr/bin|/sbin|/bin)
        PATH="/root/.opencode/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
        ;;
    *)
        PATH="/root/.opencode/bin:/usr/local/sbin:/usr/local/bin:$first_entry"
        ;;
esac
export PATH
for command_name in python3 git tmux gh opencode; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "错误: 远端缺少必需命令 $command_name (PATH=$PATH)" >&2
        exit 1
    fi
done
EOF
then
    echo "错误: 远端依赖预检失败，未做任何安装或升级。" >&2
    exit 1
fi
echo "远端依赖检查通过。"

# 3. 上传到与安装目录同一文件系统的暂存目录
echo "[3/7] 正在上传代码到远端暂存目录 (${REMOTE_STAGING})..."
if ! ssh "${REMOTE_HOST}" "rm -rf '${REMOTE_STAGING}' && mkdir -p '${REMOTE_STAGING}/core' '${REMOTE_STAGING}/systemd'"; then
    echo "错误: 无法创建远端暂存目录，活跃安装未改动。" >&2
    exit 1
fi
scp -q "${LOCAL_DIR}/agent.py" "${REMOTE_HOST}:${REMOTE_STAGING}/agent.py"
scp -q "${LOCAL_DIR}/config.example.json" "${REMOTE_HOST}:${REMOTE_STAGING}/config.example.json"
scp -q "${LOCAL_DIR}/core/"*.py "${REMOTE_HOST}:${REMOTE_STAGING}/core/"
scp -q "${LOCAL_DIR}/systemd/onespace-cloud-agent.service" "${REMOTE_HOST}:${REMOTE_STAGING}/systemd/"

# 4. 选择并校验远端配置（先于任何安装、启动器或重启改动）
echo "[4/7] 校验远端配置..."
if ssh "${REMOTE_HOST}" "test -f '${REMOTE_DEST}/config.json'"; then
    FIRST_DEPLOY=0
else
    FIRST_DEPLOY=1
fi
if ! ssh "${REMOTE_HOST}" <<EOF
set -e
if [ "${FIRST_DEPLOY}" = "0" ]; then
    cp "${REMOTE_DEST}/config.json" "${REMOTE_STAGING}/config.json"
    python3 -m json.tool "${REMOTE_STAGING}/config.json" > /dev/null
else
    python3 -m json.tool "${REMOTE_STAGING}/config.example.json" > /dev/null
fi
EOF
then
    echo "错误: 远端配置 JSON 校验失败，活跃安装与既有备份均未改动。" >&2
    exit 1
fi
echo "远端配置校验通过。"

# 5. 保存旧安装快照并在同一文件系统内发布
echo "[5/7] 保存快照并发布新版本..."
if ! ssh "${REMOTE_HOST}" <<EOF
set -e
if [ "${FIRST_DEPLOY}" = "0" ] && [ -d "${REMOTE_DEST}" ]; then
    mkdir -p "${REMOTE_BACKUPS}"
    snapshot="${REMOTE_BACKUPS}/\$(date +%Y%m%d%H%M%S)-\$\$"
    mkdir -p "\$snapshot"
    cp -a "${REMOTE_DEST}/." "\$snapshot/"
    rm -rf "${REMOTE_DEST}"
    if ! mv "${REMOTE_STAGING}" "${REMOTE_DEST}"; then
        echo "错误: 发布失败，正在恢复旧安装..." >&2
        mkdir -p "${REMOTE_DEST}"
        cp -a "\$snapshot/." "${REMOTE_DEST}/"
        exit 1
    fi
else
    rm -rf "${REMOTE_DEST}"
    if ! mv "${REMOTE_STAGING}" "${REMOTE_DEST}"; then
        echo "错误: 首次安装发布失败。" >&2
        exit 1
    fi
fi
if [ "${FIRST_DEPLOY}" = "1" ]; then
    cp "${REMOTE_DEST}/config.example.json" "${REMOTE_DEST}/config.json"
fi
EOF
then
    echo "错误: 代码发布失败，旧安装已保留或恢复。" >&2
    exit 1
fi

# 6. 配置 cloud-session 与 oc 快捷命令 (对齐本地 oc="opencode --yolo" 默认全授权)
echo "[6/7] 配置全局快捷命令 /usr/local/bin/cloud-session 与 /usr/local/bin/oc..."
ssh "${REMOTE_HOST}" <<'EOF'
cat << 'INNER_EOF' > /usr/local/bin/cloud-session
#!/usr/bin/env bash
exec /usr/bin/python3 /root/onespace-cloud-agent/core/session_runner.py "$@"
INNER_EOF
chmod +x /usr/local/bin/cloud-session

cat << 'INNER_EOF' > /usr/local/bin/oc
#!/usr/bin/env bash
export PATH=$PATH:/root/.opencode/bin
exec /root/.opencode/bin/opencode --yolo "$@"
INNER_EOF
chmod +x /usr/local/bin/oc
ln -sf /root/.opencode/bin/opencode /usr/local/bin/opencode

if ! grep -q 'alias oc=' /root/.bashrc 2>/dev/null; then
    echo 'alias oc="opencode --yolo"' >> /root/.bashrc
fi
EOF

# 7. 注册并重启 systemd 服务，随后清理过期快照
echo "[7/7] 注册并启动 systemd 系统服务..."
ssh "${REMOTE_HOST}" <<EOF
set -e
cp "${REMOTE_DEST}/systemd/onespace-cloud-agent.service" /etc/systemd/system/
systemctl daemon-reload
systemctl enable onespace-cloud-agent.service
systemctl restart onespace-cloud-agent.service
if [ -d "${REMOTE_BACKUPS}" ]; then
    count=0
    for snapshot in \$(ls -1dt "${REMOTE_BACKUPS}"/* 2>/dev/null); do
        count=\$((count + 1))
        if [ "\$count" -gt "${KEEP_SNAPSHOTS}" ]; then
            rm -rf "\$snapshot"
        fi
    done
fi
EOF

sleep 1
ssh "${REMOTE_HOST}" <<'EOF'
systemctl is-active onespace-cloud-agent.service || true
GATEWAY_PORT=$(python3 -c 'import json
try:
    with open("/root/onespace-cloud-agent/config.json", "r", encoding="utf-8") as f:
        port = json.load(f).get("ai_gateway", {}).get("port", 17688)
    port = int(port)
    print(port if port > 0 else 17688)
except Exception:
    print(17688)' 2>/dev/null || echo 17688)
echo -n "AI 网关健康检查: "
curl -s "http://127.0.0.1:${GATEWAY_PORT}/health" || echo "正在拉起中..."
echo ""
if ! gh auth status > /dev/null 2>&1; then
    echo ""
    echo "⚠️  GitHub 尚未登录。请先创建 Personal access token (classic)（勾选 repo；如需修改工作流再勾选 workflow），"
    echo "    然后在 Mac 本地终端通过标准输入完成授权，避免使用需要浏览器的交互式 OAuth："
    echo "    export GH_PAT=\"ghp_your_personal_access_token\""
    echo "    printf '%s' \"\$GH_PAT\" | ssh dmit-server 'gh auth login --with-token'"
    echo "    ssh dmit-server 'gh auth setup-git'"
    echo "    unset GH_PAT"
    echo "    也可以把 Token 填入 /root/onespace-cloud-agent/config.json 的 issue_worker.github_token。"
else
    echo "✅ GitHub 已处于登录授权状态。"
fi
EOF

echo ""
echo "=================================================="
echo "🎉 OneSpace Cloud Agent 部署/更新已顺利完成！"
echo "- 查看服务日志: ssh ${REMOTE_HOST} 'journalctl -u onespace-cloud-agent -f'"
echo "- 修改服务配置: ssh ${REMOTE_HOST} 'nano ${REMOTE_DEST}/config.json'"
echo "- 启动长程编程: ssh -t ${REMOTE_HOST} 'cloud-session <repo_url>'"
echo "=================================================="
