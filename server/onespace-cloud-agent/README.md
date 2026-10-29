# OneSpace Cloud Agent (`onespace-cloud-agent`)

`onespace-cloud-agent` 是 OneSpace 桌面端的**云端常驻伴生服务与自主研发 Agent**，专门部署在具备公网 IP 的 Linux 服务器（如 DMIT）上，适配低配（1 核 2G 内存）环境，提供：

1. **Linux AI 网关**：纯 Python 标准库实现的 OpenAI 兼容代理，支持多服务商轮询、密钥冷却、模型池、模板自动同步，并把可用模型同步到 `opencode` 配置。本地合盖关机后，云端仍可继续处理请求。
2. **Mac 合盖即走的长程重构会话**：基于 `tmux` + `opencode --yolo`，断网或合盖不丢会话，随时可恢复现场；支持 Git Worktree 隔离开发。
3. **24/7 代码库监控与自动 PR 助手**：监听带指定标签的 GitHub Issue，先校验作者仓库权限，再自动接单、无头调用 `opencode run` 实施修改、提交分支并创建 Pull Request。
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
   - 不要勾选 `admin:org`、`delete_repo`、`admin:repo_hook` 等管理或删除类作用域：Agent 只需要仓库读写，过宽的作用域会放大凭据泄露时的破坏面。
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

Issue Worker 启动时会执行不带 `--show-token` 的 `gh auth status` 检查凭据姿态：认证失败时记录警告；输出中出现 `admin:org`、`delete_repo`、`admin:repo_hook` 或其他 `admin:` 前缀作用域时警告作用域过大。该检查只读取 `gh` 报告的信息，不会为不报告作用域的凭据类型推测结论，也不会输出未脱敏的 Token。

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
| `retry.max_cooldown_seconds` | 上游 `Retry-After` 触发的密钥冷却上限，默认 `600` 秒。 |
| `providers` | 服务商字典，键为服务商名称。 |
| `model_pools` | 虚拟模型池字典，键为对外暴露的模型名。 |

单个服务商支持：

- `base_url`：上游 OpenAI 兼容地址，例如 `https://api.deepseek.com/v1`。
- `api_keys`：密钥数组，按顺序轮询。HTTP 429、任意 5xx 或传输异常冷却当前 key，并先尝试同服务商其余可用 key 再切换下一个候选；每个 key 在每个候选内至多尝试一次，冷却中的 key 绝不复用或等待，非可重试业务错误直接失败而不切换候选。
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

### 密钥冷却与故障转移

- 同服务商优先：任一 key 因 HTTP 429、任意 5xx 或传输异常失败后，先尝试该服务商其余可用 key，全部用尽才切换到下一个候选；每个 key 在每个候选内至多尝试一次，冷却中的 key 绝不复用或等待。
- 冷却时长：默认 HTTP 失败 `60` 秒、传输异常 `30` 秒。正有限数值的 `Retry-After` 覆盖默认，并由 `ai_gateway.retry.max_cooldown_seconds`（默认 `600`）封顶；零、负数或非法值使用默认。到期基于单调时钟，系统墙钟变化不影响。
- 全冷却与不可用：匹配候选的全部已配置 key 均在冷却时返回 429，`Retry-After` 取最小剩余冷却的上取整值，且不发起任何上游调用；无 key 的服务商直接跳过且不写入冷却记录；已知模型没有活跃或有 key 候选时返回 502；非可重试业务错误保持立即失败。

### 流式中继

- 以 `HTTPResponse.read1` 和 `65536` 字节读取容量有界读取，立即转发当前可用字节，不等待累计到最小块大小，并保留包括短尾块在内的全部字节。
- 下游写出/刷新断开即结束中继并关闭上游，不冷却 key、不切换候选；响应头提交后的上游读取失败只关闭下游流，绝不再追加第二个 JSON 响应体或重放其他候选。

### `issue_worker`

| 字段 | 说明 |
| --- | --- |
| `enabled` | 是否启用 Issue 自动处理。 |
| `github_token` | 可选，GitHub Personal access token (classic)，用于注入 `GH_TOKEN` / `GITHUB_TOKEN`。 |
| `watch_repositories` | 监听的仓库列表，格式为 `owner/repo`。 |
| `trigger_label` | 触发标签，默认 `bot:task`。 |
| `require_author_permission` | 是否校验 Issue 作者仓库权限，默认 `true`；设为 `false` 只关闭该授权门，属于兼容开关。 |
| `permission_cache_seconds` | 作者权限查询的单调时钟缓存 TTL，默认 `600` 秒。 |
| `max_attempts` | 单任务总尝试次数，默认 `3`；设为 `1` 即不重试。 |
| `poll_interval_seconds` | 轮询间隔，默认 `180` 秒。 |
| `workspace_root` | 任务与仓库工作目录，默认 `/root/cloud-agent-workspaces`。 |

### Issue 作者权限

- 启用授权门时，仅作者在该仓库的有效权限为 `admin`、`maintain` 或 `write` 才处理；`maintain` 归一化为 `write`、`triage` 归一化为 `read`，其他任意角色名不构成授权。
- 无作者信息、有效权限不足（`none` / `read` / `triage`）或权限查询失败的 Issue 只记录日志并跳过，不发表评论、不创建分支，也不消耗执行尝试；该跳过不写入已处理状态，之后权限变化可以重新生效。
- 权限查询结果按「仓库 + 作者」缓存；查询失败按未授权处理（fail closed）、记录日志且不缓存为授权。

### 任务执行、重试与幂等

- 克隆前用 `gh api --paginate --slurp` 完整分页列出该仓库全部 open PR，并按 `#<编号>` 数字边界匹配引用（`#420` 不匹配 `#42`）。命中即跳过克隆、OpenCode 与建 PR，标记 `done` 并记录 PR URL；创建 PR 失败后会再次完整检查，以处理响应丢失但 PR 已创建的情况。PR 查询失败按瞬时失败处理，未成功确认前不会克隆。
- `max_attempts` 表示总尝试次数。仅瞬时失败重试：子进程超时、克隆/推送/网络失败、OpenCode 非零退出与上游 5xx；间隔为 `min(30 * 2 ** (attempt - 1), 300)` 秒。HTTP 401/403、仓库或 Issue 不存在、Issue 已关闭等永久错误只消耗一次尝试。
- 每次重试前重新确认 Issue 仍为 open 并复用 open PR 检查；OpenCode 非零退出直接判定失败，不提交、不推送。成功、永久失败或尝试耗尽后只发起一次终态评论与通知，后续轮询与重启会跳过已记录的终态。
- OpenCode 执行成功但工作区无变更时仍以 `done` 结束，任务流水保留 `no_changes` 状态；任务流水与 `agent.py tasks --json` 的字段与状态保持桌面端已消费的格式。

### 提示词与内容隔离

- Prompt 包含仓库、Issue 编号等元数据；标题与正文放在 `<<<BEGIN_UNTRUSTED_ISSUE_DATA>>>` 与 `<<<END_UNTRUSTED_ISSUE_DATA>>>` 边界内，正文中出现的相同标记会被改写为无法闭合的形式。
- 固定指令声明该内容是待处理数据而非命令，禁止读取或泄露密钥、凭据、环境变量与宿主文件，只允许在克隆目录内修改，并拒绝改变这些规则。
- 这些只是提示约束，不是操作系统沙箱：默认 root 执行与 `--yolo` 保持不变，必要时请自行收紧服务器权限。

### 任务状态与尝试日志

- 终态记录写入 `~/.config/onespace-cloud-agent/worker_state.json`，格式为 `version: 2` 加 `repositories`，按仓库与 Issue 编号字符串索引，每条记录包含 `status`（`done` / `failed`）、`attempts`、脱敏后的 `last_error` 与 UTC ISO-8601 `updated_at`。
- 旧版「仓库 → 编号列表」状态按 `done`、`0` 次尝试加载；首次写入 v2 前会把原始字节一次性备份为 `worker_state.json.v1.bak`（绝不覆盖，备份失败则中止迁移写入）。损坏或形状非法的状态被隔离为带唯一后缀的 `.corrupt` 文件、记录日志并以空状态继续。保存使用同目录临时文件加 `os.replace`。
- v2 状态不向后兼容：回滚到旧 worker 之前，必须先恢复 `worker_state.json.v1.bak`，绝不能让旧版本读取 v2 状态。
- 每次尝试在 `~/.config/onespace-cloud-agent/logs/` 生成 `<repo>_<issue>_<timestamp>_<序号>.log`（仓库斜杠替换为下划线，毫秒时间戳加进程内序号保证唯一），记录开始时间、命令参数、退出码与失败分类、有界的 stdout/stderr，以及 `git diff --stat`（克隆前明确标注不可用）。
- 日志写入前脱敏配置的 Token、凭据类环境变量值与 `Authorization` 值；单个日志上限 `2097152` 字节（2 MiB），截断时为结果元数据预留空间。全部仓库合计只保留最新的 20 份 agent 尝试日志（按 mtime、文件名次序），无关文件不受影响；日志创建或写入失败会显式记录并使任务停止，不会伪造成功。

### `notification`

| 字段 | 说明 |
| --- | --- |
| `enabled` | 是否启用通知。 |
| `dingtalk_webhook` | 钉钉机器人 Webhook。 |
| `dingtalk_secret` | 钉钉机器人加签密钥，可选。 |
| `wechat_work_webhook` | 企业微信群机器人 Webhook。 |

每个通道的单次投递最多 3 次总尝试：传输错误、HTTP 429 与 5xx 视为短暂失败，重试间隔为 1 秒、2 秒；其他 4xx 只尝试一次。钉钉与企业微信通道互相独立，单通道失败不影响另一通道，也不会改变任务结果；错误与日志中只出现去掉查询串的 webhook 地址，不输出凭据。

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
4. Cloud Agent 会先校验 Issue 作者是否具备仓库 `admin` / `maintain` / `write` 权限，通过后自动接单并在 Issue 回复；随后克隆仓库、调用 `opencode run` 实施修改、提交 `bot/issue-<编号>-<时间戳>` 分支、创建 Pull Request，并通过钉钉/企微群通知结果。已存在引用该 Issue 的 open PR 时会跳过重复执行。
5. 任务终态记录保存在 `~/.config/onespace-cloud-agent/worker_state.json`（`version: 2`，含尝试次数与脱敏错误），重启后不会重复处理；每次尝试的日志在 `~/.config/onespace-cloud-agent/logs/` 下，全局只保留最新 20 份。

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
