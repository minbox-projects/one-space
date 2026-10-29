# OneSpace Cloud Agent (`onespace-cloud-agent`)

`onespace-cloud-agent` 是 OneSpace 桌面端的**云端常驻伴生服务与自主研发 Agent**，专门部署在具备公网 IP 的 Linux 服务器（如 DMIT）上，适配低配（1 核 2G 内存）环境，提供：

1. **Linux AI 网关**：纯 Python 标准库实现的 OpenAI 兼容代理，支持多服务商轮询、密钥冷却、模型池、模板自动同步，并把可用模型同步到 `opencode` 配置。本地合盖关机后，云端仍可继续处理请求。
2. **Mac 合盖即走的长程重构会话**：基于 `tmux` + `opencode --yolo`，断网或合盖不丢会话，随时可恢复现场；支持 Git Worktree 隔离开发。
3. **24/7 代码库监控与自动 PR 助手**：监听带指定标签的 GitHub Issue，自动接单、无头调用 `opencode run` 实施修改、提交分支并创建 Pull Request。
4. **多渠道通知**：支持钉钉自定义机器人（含安全加签）与企业微信群机器人，任务接单、完成或异常时即时推送。
5. **磁盘空间守护者（Disk Guardian）**：自动清理任务工作目录、修剪过期 OpenCode 历史与缓存，在 12GB~20GB 磁盘限制下长期稳定运行。

---

## 目录结构

```text
server/onespace-cloud-agent/
├── agent.py                 # 主守护进程入口（管理 Gateway、Worker、Guardian）
├── config.example.json      # 配置文件模板（网关、Issue Worker、通知）
├── deploy.sh                # Mac 端执行的自动化远程安装/更新部署脚本
├── core/
│   ├── gateway.py           # 轻量 Linux AI 网关（多 Provider 轮询、密钥冷却、模型池、模板同步）
│   ├── issue_worker.py      # GitHub Issue 监听、任务接单与自动 PR 引擎
│   ├── session_runner.py    # tmux 会话管理与 Git Worktree 隔离逻辑
│   ├── disk_guardian.py     # 磁盘空间监控与自动修剪机制
│   └── notifier.py          # 钉钉 / 企业微信群通知推送
├── systemd/
│   └── onespace-cloud-agent.service  # Linux 开机自启服务配置
└── tests/                   # 单元测试集
```

---

## 环境要求

- Linux 服务器（Debian/Ubuntu 等），Python 3，无需安装第三方 Python 依赖。
- `git`、`tmux`、GitHub CLI（`gh`）。
- `opencode` 已安装并位于 `/root/.opencode/bin/opencode`（`deploy.sh` 会配置 PATH 与软链接）。
- 可免密 SSH 登录目标服务器，且服务器可访问 GitHub 与各 AI 服务商 API。

---

## 快速部署

在 Mac 本地终端运行一键部署脚本：

```bash
# 默认部署到 ~/.ssh/config 中配置的 dmit-server
./server/onespace-cloud-agent/deploy.sh dmit-server
```

脚本会同步代码、初始化 `config.json`、安装 `cloud-session` / `oc` 快捷命令，并注册、启动 systemd 服务。部署完成后请填写远端 `/root/onespace-cloud-agent/config.json` 中的真实 API Key、监听仓库与通知配置，然后执行：

```bash
ssh dmit-server "systemctl restart onespace-cloud-agent"
```

### 登录 GitHub（Personal access tokens classic）

Linux 服务器通常没有图形界面，**不要使用需要浏览器的 `gh auth login` 交互式 OAuth 流程**。请改用 Personal access tokens (classic) 完成授权：

1. 在 GitHub 打开 [Personal access tokens (classic)](https://github.com/settings/tokens)，点击 **Generate new token (classic)**。
2. 按需勾选权限：
   - `repo`：读写仓库、Issue、Pull Request（私有仓库必需）。
   - `workflow`：修改 `.github/workflows` 下的工作流文件时必需。
   - `read:org`：需要访问组织仓库时建议勾选。
3. 生成后复制 `ghp_...` 开头的 Token（只显示一次，请妥善保存）。
4. 在本地 Mac 终端把 Token 通过标准输入交给服务器上的 `gh`，避免 Token 出现在远端命令历史中：

```bash
# 在本地 Mac 执行；用环境变量避免 Token 写入 shell 历史
export GH_PAT="ghp_your_personal_access_token"
printf '%s' "$GH_PAT" | ssh dmit-server "gh auth login --with-token"

# 让 git push / git pull 复用 gh 凭据
ssh dmit-server "gh auth setup-git"

# 确认登录状态
ssh dmit-server "gh auth status"
unset GH_PAT
```

也可以在远端 `config.json` 中填写 `issue_worker.github_token`，Issue Worker 启动时会自动注入 `GH_TOKEN` 与 `GITHUB_TOKEN` 环境变量。生产环境更推荐上面的 `gh auth login --with-token`，这样 `gh` 与 `git` 都使用同一份授权。

---

## 配置说明

配置模板见 `config.example.json`。

### `ai_gateway`

| 字段 | 说明 |
| --- | --- |
| `port` | 网关监听端口，默认 `17688`，仅监听 `127.0.0.1`。 |
| `auto_sync_interval_seconds` | 服务商模板自动同步间隔，默认 `3600` 秒。 |
| `auth.token` | 受保护端点的 Bearer 凭据，默认 `sk-onespace-cloud-agent`。 |
| `auth.require` | 是否校验 Bearer 凭据，默认 `true`；设为 `false` 只跳过凭据校验，Origin 拒绝仍然生效。 |
| `max_body_bytes` | POST 请求体上限，默认 `10485760`（10 MiB）；超限在读取请求体前返回 413。 |
| `providers` | 服务商字典，键为服务商名称。 |
| `model_pools` | 虚拟模型池字典，键为对外暴露的模型名。 |

单个服务商支持：

- `base_url`：上游 OpenAI 兼容地址，例如 `https://api.deepseek.com/v1`。
- `api_keys`：密钥数组，按顺序轮询；失败后进入冷却。
- `models`：本地模型映射。每项可写 `upstream_model`，以及可选的 `display_name`、`context_window`、`reasoning_efforts`。
- `is_active`：是否启用该服务商，默认 `true`。
- `template_url`：可选的模型列表地址，用于定时拉取并补充模型。

模型池示例：

```json
"model_pools": {
  "cloud-coder": {
    "description": "默认自主编程候选池",
    "candidates": [
      { "provider": "deepseek", "model": "deepseek-chat" }
    ]
  }
}
```

`candidates` 按顺序尝试，失败自动切换到下一个候选。

### 网关入口规则

- **Origin 拒绝优先于一切处理**：任何携带 `Origin` 请求头的请求，包括空值、`/health` 与 `OPTIONS`，都在认证、读取请求体和转发之前返回 403；不带 `Origin` 的 `OPTIONS` 返回 204；任何成功、错误或流式响应都不会下发通配符 CORS 头。
- **Bearer 认证**：`GET /models`、`GET /v1/models`、`POST /chat/completions`、`POST /v1/chat/completions` 需要 `Authorization: Bearer <token>`，缺失或错误返回 401 JSON 且不请求上游；`GET /health`、`GET /v1/health` 无需认证。同步到 OpenCode 的 `provider.gateway.options.apiKey` 使用当前配置的 `ai_gateway.auth.token`。
- **请求体边界**：`Content-Length` 缺失、为 0、负数或非数字时返回 400；大于 `ai_gateway.max_body_bytes` 时在读取请求体前返回 413；空请求体与非法 JSON 返回 400。
- **严格模型解析**：只有模型池名、活跃服务商的 `provider/model` 前缀，以及活跃服务商上的显式模型映射会被解析。未知、空或纯空白模型名返回 404 且 `code` 为 `model_not_found`，不请求上游；已知模型池没有任何可用候选时返回 502；未知模型不再广播到全部活跃服务商。
- **目录一致性**：模板同步先构建新的目录代次，再加锁发布；解析、模型列表与 OpenCode 投影各自读取一个完整代次。模板同步是加性合并，保留既有模型映射和密钥运行状态（含冷却）。

### `issue_worker`

| 字段 | 说明 |
| --- | --- |
| `enabled` | 是否启用 Issue 自动处理。 |
| `github_token` | 可选，GitHub Personal access token (classic)，用于注入 `GH_TOKEN` / `GITHUB_TOKEN`。 |
| `watch_repositories` | 监听的仓库列表，格式为 `owner/repo`。 |
| `trigger_label` | 触发标签，默认 `bot:task`。 |
| `poll_interval_seconds` | 轮询间隔，默认 `180` 秒。 |
| `workspace_root` | 任务与仓库工作目录，默认 `/root/cloud-agent-workspaces`。 |

### `notification`

| 字段 | 说明 |
| --- | --- |
| `enabled` | 是否启用通知。 |
| `dingtalk_webhook` | 钉钉机器人 Webhook。 |
| `dingtalk_secret` | 钉钉机器人加签密钥，可选。 |
| `wechat_work_webhook` | 企业微信群机器人 Webhook。 |

---

## 核心使用场景

### 场景 1：Mac 合盖即走的长程重构（支持 Git Worktree 隔离）

在任意时刻，直接在服务器上为目标项目开启会话：

```bash
# 1. 单参数：克隆/进入主仓库并启动或恢复默认会话
ssh -t dmit-server "cloud-session https://github.com/your-name/your-repo.git"

# 2. 双参数：基于 Git Worktree 为特定分支创建独立隔离工作区（并发无冲突）
ssh -t dmit-server "cloud-session your-repo my-feature-branch"

# 3. 无参数：只有一个活跃会话时直接恢复；多个时列出；没有时在默认 workspace 启动
ssh -t dmit-server "cloud-session"

# 4. 会话管理
ssh dmit-server "cloud-session list"
ssh -t dmit-server "cloud-session attach <session-name>"
ssh dmit-server "cloud-session kill <session-name>"
```

- 单参数会优先恢复同名活跃会话；若不存在，则克隆 Git URL 或使用 `workspace_root` 下的目录，并创建 `dev_<repo>` 会话。
- 双参数会在 `<workspace_root>/<repo>-worktrees/<branch>` 创建或复用 Worktree，并创建 `dev_<repo>_<branch>` 会话。
- 会话中的 `opencode` 默认使用 `--yolo` 全权限自动授权模式。
- 进入 `opencode` 后下达任务，直接断开 SSH 或合上屏幕，云端会继续执行；再次执行相同命令即可恢复现场。

### 场景 2：24/7 代码库监控与自动提 PR

1. 在 `config.json` 的 `watch_repositories` 中加入要监听的 GitHub 仓库。
2. 确保 GitHub 已通过 Personal access token (classic) 授权，且 Token 具备 `repo`（如需修改工作流还需 `workflow`）权限。
3. 在目标仓库创建 Issue，并打上 `bot:task` 标签。
4. Cloud Agent 会自动接单并在 Issue 回复；随后克隆仓库、调用 `opencode run` 实施修改、提交 `bot/issue-<编号>-<时间戳>` 分支、创建 Pull Request，并通过钉钉/企微群通知结果。
5. 已处理的 Issue 记录保存在 `~/.config/onespace-cloud-agent/worker_state.json`，重启后不会重复处理。

### 场景 3：从本地或 OpenCode 调用云端 AI 网关

网关提供 OpenAI 兼容接口：

```bash
# 健康检查（公开）
curl -s http://127.0.0.1:17688/health

# 查看模型列表（模型池与各服务商模型，需 Bearer 凭据）
curl -s http://127.0.0.1:17688/v1/models \
  -H "Authorization: Bearer sk-onespace-cloud-agent"

# 对话请求：可直接使用模型池名称，或使用 provider/model 形式
curl -s http://127.0.0.1:17688/v1/chat/completions \
  -H "Authorization: Bearer sk-onespace-cloud-agent" \
  -H "Content-Type: application/json" \
  -d '{"model":"cloud-coder","messages":[{"role":"user","content":"hello"}]}'
```

支持的端点：

- `GET /health`、`GET /v1/health`（公开）
- `GET /models`、`GET /v1/models`（需 Bearer 凭据）
- `POST /chat/completions`、`POST /v1/chat/completions`（需 Bearer 凭据，支持流式 SSE）

服务启动时以及每次模板同步后，网关会自动把模型池和活跃服务商模型同步到 `~/.config/opencode/opencode.json`，并统一使用单一 `Cloud Agent Gateway` 提供商（示例中的 `apiKey` 取自 `ai_gateway.auth.token`）：

```json
{
  "provider": {
    "gateway": {
      "name": "Cloud Agent Gateway",
      "npm": "@ai-sdk/openai-compatible",
      "options": {
        "baseURL": "http://127.0.0.1:17688/v1",
        "apiKey": "sk-onespace-cloud-agent"
      }
    }
  }
}
```

同步只读取并写入 `opencode.json`：仅替换 `provider.gateway`，保留其他 provider 与所有既有顶层配置；缺少 `$schema` 或默认模型时可初始化。若同目录存在 `opencode.jsonc`，网关保持其字节不变并记录警告——OpenCode 可能优先读取 JSONC，需要手动合并或删除。写入使用同目录临时文件加 `os.replace`，整个读取/合并/写入事务串行化；既有 JSON 非法或不是对象时保持原文件不变并记录警告。

> 桌面端「同步 AI 网关到云端」会用本地网关配置整体覆盖远端 `ai_gateway` 段，云端的自定义 `auth` 与 `max_body_bytes` 会被重置为默认值，需要在该操作后重新补回。

---

## 常用管理命令

```bash
# 查看云端守护服务运行日志
ssh dmit-server "journalctl -u onespace-cloud-agent -f"

# 查看/重启云端服务
ssh dmit-server "systemctl status onespace-cloud-agent"
ssh dmit-server "systemctl restart onespace-cloud-agent"

# 编辑配置
ssh dmit-server "nano /root/onespace-cloud-agent/config.json"

# 查看 GitHub 授权状态
ssh dmit-server "gh auth status"

# 查看活跃的云端会话
ssh dmit-server "cloud-session list"

# 检查网关健康状态与磁盘空间
ssh dmit-server "curl -s http://127.0.0.1:17688/health && df -h /"
```

---

## 运行测试

在 `server/onespace-cloud-agent` 目录下执行：

```bash
python3 -m unittest discover -s tests -v
```
