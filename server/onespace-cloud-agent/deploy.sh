#!/usr/bin/env bash
# onespace-cloud-agent 一键远程部署与同步脚本
set -euo pipefail

REMOTE_HOST="${1:-dmit-server}"
LOCAL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE_DEST="/root/onespace-cloud-agent"

echo "=================================================="
echo "   OneSpace Cloud Agent 部署工具 (目标: ${REMOTE_HOST})   "
echo "=================================================="

# 1. 连通性预检
echo "[1/6] 检查 SSH 连通性..."
if ! ssh -o ConnectTimeout=5 "${REMOTE_HOST}" "uname -m" > /dev/null 2>&1; then
    echo "错误: 无法免密连接到 ${REMOTE_HOST}，请检查 ~/.ssh/config 配置。" >&2
    exit 1
fi
echo "SSH 连通正常。"

# 2. 准备远端目录与同步代码
echo "[2/6] 正在同步代码文件到远程服务器 (${REMOTE_DEST})..."
ssh "${REMOTE_HOST}" "mkdir -p ${REMOTE_DEST}/core ${REMOTE_DEST}/systemd"
scp -q "${LOCAL_DIR}/agent.py" "${REMOTE_HOST}:${REMOTE_DEST}/agent.py"
scp -q "${LOCAL_DIR}/config.example.json" "${REMOTE_HOST}:${REMOTE_DEST}/config.example.json"
scp -q "${LOCAL_DIR}/core/"*.py "${REMOTE_HOST}:${REMOTE_DEST}/core/"
scp -q "${LOCAL_DIR}/systemd/onespace-cloud-agent.service" "${REMOTE_HOST}:${REMOTE_DEST}/systemd/"

# 3. 初始化配置文件（若无则复制模板）
echo "[3/6] 检查远端配置文件..."
ssh "${REMOTE_HOST}" << 'EOF'
if [ ! -f /root/onespace-cloud-agent/config.json ]; then
    cp /root/onespace-cloud-agent/config.example.json /root/onespace-cloud-agent/config.json
    echo "已为您在远端初始化 /root/onespace-cloud-agent/config.json，请记得填入真实 API Key 与仓库配置。"
fi
EOF

# 4. 配置 cloud-session 与 oc 快捷命令 (对齐本地 oc="opencode --yolo" 默认全授权)
echo "[4/6] 配置全局快捷命令 /usr/local/bin/cloud-session 与 /usr/local/bin/oc..."
ssh "${REMOTE_HOST}" << 'EOF'
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

if ! grep -q 'alias oc=' /root/.bashrc 2>/dev/null; then
    echo 'alias oc="opencode --yolo"' >> /root/.bashrc
fi
EOF

# 5. 注册与启动 systemd 服务
echo "[5/6] 注册并启动 systemd 系统服务..."
ssh "${REMOTE_HOST}" << 'EOF'
cp /root/onespace-cloud-agent/systemd/onespace-cloud-agent.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable onespace-cloud-agent.service
systemctl restart onespace-cloud-agent.service
EOF

# 6. 验证服务状态与 GitHub 登录状态
echo "[6/6] 检查运行状态与 GitHub 授权..."
sleep 2
ssh "${REMOTE_HOST}" << 'EOF'
systemctl is-active onespace-cloud-agent.service || true
echo -n "AI 网关健康检查: "
curl -s http://127.0.0.1:17688/health || echo "正在拉起中..."
echo ""
if ! gh auth status > /dev/null 2>&1; then
    echo ""
    echo "⚠️  GitHub 尚未登录。请在 Mac 终端运行以下命令完成一次性登录："
    echo "    ssh -t dmit-server 'gh auth login'"
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
