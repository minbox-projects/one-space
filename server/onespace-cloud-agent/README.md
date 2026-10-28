# OneSpace Cloud Agent (`onespace-cloud-agent`)

`onespace-cloud-agent` 是 OneSpace 桌面端的**云端常驻伴生服务与自主研发 Agent**，专门部署在具备公网 IP 的 Linux 服务器（如 DMIT）上，适配低配（1核 2G 内存）环境，提供：

1. **Linux AI 网关**：独立运行在云端，提供多服务商轮询、模型池、模板自动同步，端云同源，本地合盖关机云端仍能持续请求。
2. **Mac 合盖即走的长程重构会话**：基于 `tmux` + `opencode`，远程会话与网络状态解耦，断网不丢，随时开盖恢复。
3. **24/7 代码库监控与自动 PR 助手**：支持监控任意 GitHub 仓库的 Issue（模式 3：带 `bot:task` 标签），自动分析需求、无头调用 `opencode run` 批量编写代码、轻量单测、自动提交分支并创建 PR。
4. **多渠道通知**：支持钉钉（含安全加签）与企业微信机器人，任务接单、完成或异常时即时推送。
5. **磁盘空间守护者 (Disk Guardian)**：自动修剪工作目录与历史会话，确保在 12GB~20GB 磁盘限制下长期稳健运行。

---

## 目录结构

```
server/onespace-cloud-agent/
├── agent.py                 # 主守护进程入口（管理 Gateway、Worker、Guardian）
├── config.example.json      # 配置文件模板（服务商、模型池、监听仓库、通知等）
├── deploy.sh                # Mac 端执行的自动化远程安装/更新部署脚本
├── core/
│   ├── gateway.py           # 轻量 Linux AI 网关（多 Provider 轮询、模型池、模板同步）
│   ├── issue_worker.py      # GitHub Issue 监听、任务接单与自动 PR 引擎
│   ├── session_runner.py    # tmux 会话管理与合盖防断逻辑
│   ├── disk_guardian.py     # 磁盘空间监控与自动修剪机制
│   └── notifier.py          # 钉钉 / 企业微信群通知推送
├── systemd/
│   └── onespace-cloud-agent.service  # Linux 开机自启服务配置
└── tests/                   # 完整单元测试集
```

---

## 快速部署

在 Mac 本地终端运行一键部署脚本：

```bash
# 默认部署到 ~/.ssh/config 中配置的 dmit-server
./server/onespace-cloud-agent/deploy.sh dmit-server
```

首次部署完成后，根据提示登录 GitHub：
```bash
ssh -t dmit-server "gh auth login"
```

---

## 核心使用场景

### 场景 1：Mac 合盖即走的长程重构
在任意时刻，直接在服务器上为目标项目开启会话：
```bash
ssh -t dmit-server "cloud-session https://github.com/your-name/your-repo.git"
```
* 进入 `opencode` 界面后下达大型任务。
* 此时直接断开 SSH 或合上 MacBook 屏幕，云端将持续执行。
* 再次连接时，执行 `ssh -t dmit-server "cloud-session your-repo"` 即可无缝恢复现场。

### 场景 2：24/7 代码库监控与自动提 PR
1. 在 `config.json` 中的 `watch_repositories` 加入你想监听的 GitHub 仓库。
2. 在该仓库提出一个 Issue，打上 `bot:task` 标签。
3. Cloud Agent 会自动接单并在 Issue 回复，完成代码修改后自动提交 PR 并通过钉钉/企微群通知您。

---

## 常用管理命令

```bash
# 查看云端守护服务运行日志
ssh dmit-server "journalctl -u onespace-cloud-agent -f"

# 重启云端服务
ssh dmit-server "systemctl restart onespace-cloud-agent"

# 查看活跃的云端会话
ssh dmit-server "cloud-session list"

# 检查磁盘与健康状态
ssh dmit-server "curl -s http://127.0.0.1:17688/health && df -h /"
```
