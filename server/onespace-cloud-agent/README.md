# OneSpace Cloud Agent (`onespace-cloud-agent`)

`onespace-cloud-agent` 是 OneSpace 桌面端的**云端常驻伴生服务与自主研发 Agent**，专门部署在具备公网 IP 的 Linux 服务器（如 DMIT）上，适配低配（1核 2G 内存）环境，提供：

1. **Linux AI 网关**：独立运行在云端，提供多服务商轮询、模型池、模板自动同步，端云同源，本地合盖关机云端仍能持续请求。
2. **Mac 合盖即走的长程编程会话**：基于 `tmux` + `opencode`，远程会话与网络状态解耦，断网不丢，随时开盖恢复。
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

部署脚本会同步代码到远端 `/root/onespace-cloud-agent`，初始化 `/root/onespace-cloud-agent/config.json`，安装 `cloud-session` 与 `oc` 命令，并注册启动 `onespace-cloud-agent.service`。

首次部署完成后，需要完成 GitHub 授权。由于 Linux 服务器通常没有图形浏览器，**请使用 Personal access tokens (classic) 方式登录**：

1. 在浏览器中打开 GitHub：`Settings` -> `Developer settings` -> `Personal access tokens` -> `Tokens (classic)` -> `Generate new token (classic)`。
2. 至少勾选 `repo` 权限（私有仓库、Issue、PR 操作必需）；按需勾选 `workflow` 等权限。
3. 在 Mac 终端通过 SSH 将 token 注入远端 `gh` CLI：

```bash
echo "ghp_xxxxxxxxxxxxxxxxxxxx" | ssh dmit-server "gh auth login --with-token"
```

4. 验证授权状态：

```bash
ssh dmit-server "gh auth status"
```

如果只使用 Issue Worker 自动 PR，也可以把 token 写入远端配置文件 `/root/onespace-cloud-agent/config.json` 的 `issue_worker.github_token` 字段；Agent 启动时会自动注入 `GH_TOKEN` / `GITHUB_TOKEN` 环境变量。手动执行 `gh` / `git` 时仍建议完成上面的 `gh auth login --with-token`。

---

## 配置说明

远端配置文件：`/root/onespace-cloud-agent/config.json`（由 `config.example.json` 初始化）。

关键字段：

- `ai_gateway.port`：Linux AI 网关监听端口，默认 `17688`。
- `ai_gateway.auto_sync_interval_seconds`：服务商模板自动同步间隔，默认 `3600` 秒。
- `ai_gateway.providers`：上游服务商配置，包含 `base_url`、`api_keys`、`models`、`is_active`、`template_url`。
- `ai_gateway.model_pools`：虚拟模型池，请求模型名命中池 ID 时按候选列表故障转移。
- `issue_worker.enabled`：是否启用 Issue 监听。
- `issue_worker.github_token`：GitHub PAT (classic)，Agent 启动时注入环境变量。
- `issue_worker.watch_repositories`：监听仓库列表，格式 `owner/repo`。
- `issue_worker.trigger_label`：触发标签，默认 `bot:task`。
- `issue_worker.poll_interval_seconds`：轮询间隔，默认 `180` 秒。
- `issue_worker.workspace_root`：任务工作目录根路径，默认 `/root/cloud-agent-workspaces`。
- `notification.enabled`：是否启用通知。
- `notification.dingtalk_webhook` / `dingtalk_secret`：钉钉机器人与加签密钥。
- `notification.wechat_work_webhook`：企业微信机器人 Webhook。

---

## 核心使用场景

### 场景 1：Mac 合盖即走的长程编程 (支持 Git Worktree 隔离模式)

在任意时刻，直接在服务器上为目标项目开启会话：

```bash
# 1. 单参数：基于主仓库主分支开启/恢复
ssh -t dmit-server "cloud-session https://github.com/your-name/your-repo.git"

# 2. 双参数：基于 Git Worktree 为特定分支创建独立隔离工作区（并发无冲突）
ssh -t dmit-server "cloud-session your-repo my-feature-branch"

# 3. 恢复已有会话：直接恢复唯一会话
ssh -t dmit-server "cloud-session"
```

补充命令：

```bash
# 查看活跃会话
ssh -t dmit-server "cloud-session list"

# 恢复指定会话
ssh -t dmit-server "cloud-session attach dev_your-repo"

# 终止指定会话
ssh -t dmit-server "cloud-session kill dev_your-repo"
```

- 进入 `opencode` 界面后下达大型任务。
- 此时直接断开 SSH 或合上 MacBook 屏幕，云端将持续执行。
- 再次连接时，执行 `ssh -t dmit-server "cloud-session your-repo"` 即可无缝恢复现场。
- 会话默认以 `--yolo` 全权限模式启动 `opencode`；也可直接使用远端 `oc` 命令。

### 场景 2：24/7 代码库监控与自动提 PR

1. 在 `config.json` 中的 `watch_repositories` 加入你想监听的 GitHub 仓库。
2. 在该仓库提出一个 Issue，打上 `bot:task` 标签。
3. Cloud Agent 会自动接单并在 Issue 回复，完成代码修改后自动提交 PR 并通过钉钉/企微群通知您。

Issue Worker 流程：扫描带标签的 open Issue -> 回复接单 -> 克隆仓库 -> 创建 `bot/issue-<number>-<timestamp>` 分支 -> 调用 `opencode run --yolo` -> 提交并推送 -> 创建 PR -> 回写 Issue -> 清理工作目录。任务状态持久化在 `~/.config/onespace-cloud-agent/worker_state.json`。

---

## Linux AI 网关

- 默认监听 `127.0.0.1:17688`，OpenAI 兼容接口地址为 `http://127.0.0.1:17688/v1`。
- 支持多服务商轮询、有序密钥池、失败冷却（HTTP 429/5xx 冷却 60 秒，网络异常冷却 30 秒）。
- 支持模型池（Model Pool）候选故障转移。
- 支持 `provider/model` 前缀路由与模型映射。
- 支持服务商模板自动同步（`template_url` 返回 `data` 或 `models` 列表）。
- 启动时自动同步模型列表到 `~/.config/opencode/opencode.json`，提供商名称为 `Cloud Agent Gateway`，API Key 为 `sk-onespace-cloud-agent`。

可用端点：

```text
GET  /health, /v1/health
GET  /models, /v1/models
POST /chat/completions, /v1/chat/completions
```

---

## 磁盘空间守护者

- 每次任务完成后立即清理临时工作目录。
- 定期修剪 `/root/.opencode` 下 `generated-images`、`cache`、`logs`、`sessions` 中超过 3 天的文件。
- 日常巡检剩余空间低于 2 GB 时，执行深度修剪（1 天阈值）、`apt-get clean` 与 `journalctl --vacuum-size=50M`。

---

## 通知

- 钉钉：支持自定义机器人 Webhook 与安全加签（`dingtalk_secret`）。
- 企业微信：支持群机器人 Webhook。
- 任务接单、完成（含 PR 链接）与异常时推送通知。

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

# 查看 GitHub 授权状态
ssh dmit-server "gh auth status"
```

---

## 测试

```bash
cd server/onespace-cloud-agent
python3 -m unittest discover -s tests
```
