# OneSpace Cloud Agent (`onespace-cloud-agent`)

`onespace-cloud-agent` 是 OneSpace 桌面端的**云端常驻伴生服务与自主研发 Agent**，专门部署在具备公网 IP 的 Linux 服务器（如 DMIT）上，适配低配（1 核 2G 内存）环境，提供：

1. **Linux AI 网关**：纯 Python 标准库实现的 OpenAI 兼容代理，支持多服务商轮询、密钥冷却、模型池、模板自动同步，并把可用模型同步到 `opencode` 配置。本地合盖关机后，云端仍可继续处理请求。
2. **Mac 合盖即走的长程重构会话**：基于 `tmux` + `opencode --yolo`，断网或合盖不丢会话，随时可恢复现场；支持 Git Worktree 隔离开发。
3. **24/7 代码库监控与自动 PR 助手**：监听带指定标签的 GitHub Issue，先校验作者仓库权限，再自动接单、无头调用 `opencode run` 实施修改、提交分支并创建 Pull Request；授权协作者可在 Issue 上评论 `/agent` 继续调整，并复用 Agent 自有的 open PR 分支。
4. **多渠道通知**：支持钉钉自定义机器人（含安全加签）与企业微信群机器人，任务接单、完成或异常时即时推送。
5. **磁盘空间守护者（Disk Guardian）**：按白名单修剪过期 OpenCode `log` 与 `tool-output` 缓存、释放任务自有临时目录、清理过期且空闲的已注册 Worktree；默认保留全部会话文件与 `opencode.db*`、`repos`、`snapshot`。

---

## 目录结构

```text
server/onespace-cloud-agent/
├── agent.py                 # 主守护进程入口（管理 Gateway、Worker、Guardian）
├── bootstrap.sh             # 远端平台检查与缺失运行依赖安装
├── VERSION                  # 服务包版本（安装记录与上传载荷共用）
├── config.default.json      # 首次安装的运行时配置种子（无凭据、自动任务关闭）
├── config.example.json      # 带占位凭据的配置示例，仅供人工参考
├── deploy.sh                # Mac 端执行的自动化远程安装/更新部署脚本
├── core/
│   ├── gateway.py           # 轻量 Linux AI 网关（多 Provider 轮询、密钥冷却、模型池、模板同步）
│   ├── issue_worker.py      # GitHub Issue 监听、任务接单与自动 PR 引擎
│   ├── session_runner.py    # tmux 会话管理与 Git Worktree 隔离逻辑
│   ├── disk_guardian.py     # 磁盘空间监控与自动修剪机制
│   └── notifier.py          # 钉钉 / 企业微信群通知推送
├── systemd/
│   └── onespace-cloud-agent.service  # Linux 开机自启服务配置
└── tests/                   # 单元测试与伪 SSH/SCP 部署行为测试
```

---

## 环境要求

- Linux 服务器，以 `root` 执行，systemd 可用（`systemctl` 存在且能正常报告版本），CPU 架构为 amd64/x86_64 或 arm64/aarch64。
- 运行依赖 `python3`、`git`、`tmux`、GitHub CLI（`gh`）与 `opencode`。`deploy.sh` 只安装缺失项，已存在且可执行的命令绝不被升级或替换。
- 缺失依赖时只接受 Ubuntu 22.04/24.04 LTS 与 Debian 12，并通过官方 apt 源与官方安装脚本补齐。当五个依赖都已存在时，其他满足上述平台条件的发行版同样可以部署，且不安装或升级任何软件包。
- 可免密 SSH 登录目标服务器；`deploy.sh` 以非交互模式（`BatchMode=yes`、10 秒连接超时）连接，不会提示输入密码或确认主机指纹。服务器需能访问官方软件源、`opencode.ai` 与 GitHub。
- GitHub 登录与模型密钥不是安装或服务在线的前提：首次安装生成的配置不含任何凭据，Issue Worker 与通知默认关闭。
- systemd 单元固定 `PATH=/root/.opencode/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`；`session_runner.py` 在会话 PATH 中找不到 `opencode` 时回退 `/root/.opencode/bin/opencode`。

---

## 快速部署

在 Mac 本地终端运行部署脚本，两个参数均可省略：

```bash
./server/onespace-cloud-agent/deploy.sh [host] [remote_agent_dir]
```

- `host`：`~/.ssh/config` 中的主机别名，默认 `dmit-server`。
- `remote_agent_dir`：远端 Agent 安装目录，默认 `/root/onespace-cloud-agent`。该目录在服务单元、`cloud-session`、`oc` 启动器与状态检查中都按字面使用，允许包含空格；暂存与备份目录分别是 `<remote_agent_dir>-staging` 与 `<remote_agent_dir>-backups`。
- 空主机名或安装目录 `/` 会在任何远端操作前被拒绝；本机 `VERSION` 缺失或为空时同样会在远端操作前失败。

```bash
# 默认主机与默认目录
./server/onespace-cloud-agent/deploy.sh

# 指定主机与含空格的安装目录
./server/onespace-cloud-agent/deploy.sh dmit-server "/root/my agent dir"
```

脚本按固定阶段顺序执行，任一阶段失败都以非零状态退出：

1. **platform**：确认 Linux、root、可用 systemd 与受支持的架构；只有运行依赖缺失时才进一步检查发行版。非 root、无 systemd、架构不受支持，或缺失依赖且发行版不在支持列表时，在安装软件包与上传载荷之前失败。
2. **dependencies**：只补齐缺失项。`python3`、`git`、`tmux` 缺失时经 apt 安装，并确保 `curl`、`ca-certificates`、`tar` 可用；`gh` 缺失时仅从其官方 apt 源安装；`opencode` 缺失时用官方安装脚本安装固定版本 `1.19.5` 并传 `--no-modify-path`。已存在的可执行文件（包括 `/root/.opencode/bin/opencode` 等路径上的 OpenCode）保持不变。此阶段不检查 GitHub 登录。
3. **upload**：用非交互 SCP 把受管载荷上传到与安装目录同一文件系统的 `<remote_agent_dir>-staging`。
4. **config**：远端已有 `config.json` 时先用 `python3 -m json.tool` 校验，再原样复制进暂存目录；首次安装则由 `config.default.json` 生成。非法 JSON 在任何发布、启动器或服务单元改动之前失败。
5. **publish**：存在既有安装时先把它复制为 `<remote_agent_dir>-backups/<时间戳>-<pid>` 快照，再用暂存目录发布到 `<remote_agent_dir>`；同一文件系统内的提升失败时把快照复制回原路径。随后写入 systemd 单元与 `/usr/local/bin/cloud-session`、`/usr/local/bin/oc` 启动器。
6. **service**：`daemon-reload`、`enable`、`restart`，并确认 `systemctl is-active` 实际返回 `active`；仅启动命令成功不足以判定成功。
7. **metadata**：服务确认活跃后，原子写入安装记录 `install-info.json`。
8. **cleanup**：把快照修剪到最新 3 份。修剪是尽力而为，失败不会使已经成功的部署失败。

上传的载荷为 `deploy.sh`、`bootstrap.sh`、`VERSION`、`config.default.json`、`config.example.json`、`agent.py`、`core/` 下 6 个 Python 文件与 `systemd/onespace-cloud-agent.service`；本机 `config.json`、`.env` 与测试文件不会上传。

### 阶段输出

脚本为每个阶段输出固定格式的行，便于桌面端与本机工具解析：

```text
ONESPACE_DEPLOY_PHASE phase=<phase> status=start
ONESPACE_DEPLOY_PHASE phase=<phase> status=ok
ONESPACE_DEPLOY_PHASE phase=<phase> status=error reason=<reason>
```

阶段名为 `platform`、`dependencies`、`upload`、`config`、`publish`、`service`、`metadata`、`cleanup`；`reason` 使用稳定标识（如 `platform_root_required`、`dependency_install_failed`、`config_invalid`、`promotion_failed`、`service_inactive`、`metadata_write_failed`）。输出不包含配置内容或密钥。

### 版本与安装记录

- `VERSION` 是服务包版本（当前 `0.1.0`），随载荷上传，也是安装记录中的版本来源。
- `install-info.json` 只在代码、启动器与服务单元发布完成且服务确认活跃后写入，内容为 `{"version": "...", "installed_at": "<UTC ISO-8601>"}`；失败的部署不会新增或替换该记录。
- 首次安装由 `config.default.json` 生成 `config.json`：网关监听回环地址、服务商与模型池为空、监听仓库为空、Issue Worker 与通知关闭，且不含任何示例上游或 GitHub 凭据。既有 `config.json` 在重试与重新部署中按字节保留；非法 JSON 会拒绝发布。

部署完成后请填写远端 `config.json` 中的真实 API Key、监听仓库与通知配置，然后执行：

```bash
ssh dmit-server "systemctl restart onespace-cloud-agent"
```

部署不做热加载：代码或配置改动都要重启服务才生效。

### 失败与恢复

- 依赖安装阶段失败不会回滚已经装好的系统软件包，也不会发布载荷；重试会重新检查并只补充仍然缺失的部分。
- SSH/SCP 超时或连接中断时，远端可能已执行了部分操作，脚本的退出状态不代表远端已回滚；重试或手工处理前应先检查实际远端状态（目录内容、`install-info.json` 与服务状态）。
- 载荷已发布但服务启动失败时不会自动回滚代码，需要操作者按快照手工恢复，并重新安装匹配的服务单元与启动器。
- 既有配置与用户数据（worker 状态、tmux 会话、Worktree 与会话文件）在部署中保留，部署不会删除它们。
- 需要回滚时，先停止服务，再从 `<remote_agent_dir>-backups/` 中选择操作者确认的代码/配置快照手工恢复，并重新安装匹配的服务单元与启动器；没有自动回滚开关。若目标版本是旧 worker，恢复后还必须先把 `~/.config/onespace-cloud-agent/worker_state.json.v1.bak` 还原为 `worker_state.json`，绝不能让旧版本读取 v2 状态；不存在该备份时按预装/空状态恢复，不伪造备份。恢复后依次确认未认证的 `/health`、携带 Bearer 凭据的模型列表与一次已知模型请求。默认清理不删除会话数据；被应急清理删除的会话文件只能依赖操作者事先的数据备份恢复，回退代码无法找回。

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

首次安装以 `config.default.json` 生成运行时配置；`config.example.json` 是带占位凭据的完整示例，仅供人工参考，`deploy.sh` 不会用 `config.example.json` 覆盖已有配置。

### `ai_gateway`

| 字段 | 说明 |
| --- | --- |
| `port` | 网关监听端口，默认 `17688`，仅监听 `127.0.0.1`。 |
| `auto_sync_interval_seconds` | 服务商模板自动同步间隔，默认 `3600` 秒。 |
| `auth.token` | 受保护端点的 Bearer 凭据；字段缺失时回退 `sk-onespace-cloud-agent`，`config.default.json` 不写入该字段。 |
| `auth.require` | 是否校验 Bearer 凭据；字段缺失时默认 `true`，首次安装生成的配置显式设为 `false`。设为 `false` 只跳过凭据校验，Origin 拒绝仍然生效。 |
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
| `require_author_permission` | 是否校验 Issue 作者仓库权限，默认 `true`；设为 `false` 只关闭该授权门，属于兼容开关。`/agent` 评论者的权限校验不受该开关影响，始终执行。 |
| `permission_cache_seconds` | 作者权限查询的单调时钟缓存 TTL，默认 `600` 秒。 |
| `max_attempts` | 单任务总尝试次数，默认 `3`；设为 `1` 即不重试。 |
| `poll_interval_seconds` | 轮询间隔，默认 `180` 秒。 |
| `workspace_root` | 任务与仓库工作目录，默认 `/root/cloud-agent-workspaces`。 |

### Issue 作者权限

- 启用授权门时，仅作者在该仓库的有效权限为 `admin`、`maintain` 或 `write` 才处理；`maintain` 归一化为 `write`、`triage` 归一化为 `read`，其他任意角色名不构成授权。
- 无作者信息、有效权限不足（`none` / `read` / `triage`）或权限查询失败的 Issue 只记录日志并跳过，不发表评论、不创建分支，也不消耗执行尝试；该跳过不写入已处理状态，之后权限变化可以重新生效。
- 权限查询结果按「仓库 + 作者」缓存；查询失败按未授权处理（fail closed）、记录日志且不缓存为授权。

### `/agent` 后续指令

- 已处理过的 open 且仍带触发标签的 Issue 收到以 `/agent` 开头的评论时触发一次后续任务：允许前导空白，命令不区分大小写，`/agent` 之后必须是结尾或空白；评论者在该仓库的有效权限必须为 `admin`、`maintain` 或 `write`（`maintain` 归一化为 `write`、`triage` 归一化为 `read`）。`/agentic` 或出现在评论中间的 `/agent` 不会触发。
- Issue 作者权限门先于评论评估执行：作者无权限或作者查询失败时不评估任何后续评论，评论游标也不前进；已确认无权限的评论者会被消费但不启动运行。机器人评论（`user.type` 为 `Bot`，或登录名以 `[bot]` 结尾且大小写不敏感）、含 Agent 标记的评论、已关闭或未打标签的 Issue 以及不以 `/agent` 开头的评论都不会触发。
- 只有被授权的人类评论进入历史；Issue 标题与正文、授权评论与后续请求都在不可信边界内脱敏。授权评论历史与后续请求共享 50,000 UTF-8 字节上限：超出时先丢弃最旧的历史，后续请求仍然超限时截断请求并在提示词中留下可见的截断标记；Issue 标题/正文与上一次结果元数据不计入该上限。
- 每次触发都重新克隆仓库，并以完整授权历史执行一次新的 `opencode run`；不恢复 OpenCode 会话，也不复用或保留上一次的工作目录与克隆。

### 后续 PR 复用与拒绝工作流

- 只有同时满足以下条件的 open PR 才被视为 Agent 自有 PR：处于 open 状态、正文在数字边界引用 `#<编号>`（`#420` 不匹配 `#42`）、head 仓库等于目标仓库，且 head 分支与该 Issue 持久化状态中的 `branch` 完全一致。仅有同名分支或仅有 Issue 引用都不构成归属。
- 命中 Agent 自有 PR 时，检出该分支、追加提交并推送到同一分支，不创建第二个 PR。没有 Agent 自有 PR 且没有其他引用该 Issue 的 open PR 时，从默认分支创建新分支，并创建一个正文包含 `Closes #<编号>` 的新 PR。
- 不存在 Agent 自有 PR、但存在引用该 Issue 的 open PR 时，绝不修改该 PR：只回复一次说明未做任何修改，并消费本次触发。已关闭的 PR 不参与复用，也不复用其分支。
- 拒绝或要求调整的受支持路径是：保持 Agent PR 为 open 且不合并，然后在 Issue 上再次评论 `/agent`。PR 会话评论与 `Request changes` 评审不会触发后续任务；关闭 PR 也不是调整信号，下一次 `/agent` 会从默认分支创建新分支与新 PR。

### 评论游标与幂等

- 每条 Issue 状态记录持久化 `last_comment_id`。被忽略的非触发评论会直接越过；只有终态（`done`、`failed` 或 `no_changes`）之后，才把触发游标推进到触发快照中观察到的最大评论 ID。
- 评论抓取失败或评论者权限查询失败不推进游标，留待后续轮询重试；终态前崩溃可能使同一触发至少重试一次。运行期间到达的评论其 ID 高于快照，交由后续轮询处理；已消费的评论即使之后被编辑也不会再次触发。
- 多条 `/agent` 评论出现在同一快照中时合并为一次运行；之后到达的评论等待后续运行。
- Agent 发表的每条 Issue 评论都附带隐藏标记 `<!-- onespace-cloud-agent -->`；带标记的评论既不会触发后续任务，也不会进入提示词历史，而接单、完成、无变更与失败等可见文案保持不变。

### 任务执行、重试与幂等

- 初次接单时，克隆前用 `gh api --paginate` 完整分页列出该仓库全部 open PR（逐页解析拼接的 JSON 响应，不依赖 `--slurp`），并按 `#<编号>` 数字边界匹配引用（`#420` 不匹配 `#42`）。命中即跳过克隆、OpenCode 与建 PR，标记 `done` 并记录 PR URL；创建 PR 失败后会再次完整检查，以处理响应丢失但 PR 已创建的情况。PR 查询失败按瞬时失败处理，未成功确认前不会克隆。该跳过只用于初次接单；已处理 Issue 的后续任务按「后续 PR 复用与拒绝工作流」处理。
- `max_attempts` 表示总尝试次数。仅瞬时失败重试：子进程超时、克隆/推送/网络失败、OpenCode 非零退出与上游 5xx；间隔为 `min(30 * 2 ** (attempt - 1), 300)` 秒。HTTP 401/403、仓库或 Issue 不存在、Issue 已关闭等永久错误只消耗一次尝试。初次接单与 `/agent` 后续任务使用同一重试预算。
- 每次重试前重新确认 Issue 仍为 open；初次接单还会复用 open PR 检查。OpenCode 非零退出直接判定失败，不提交、不推送。成功、永久失败或尝试耗尽后只发起一次终态评论与通知，后续轮询与重启会跳过已记录的终态。
- OpenCode 执行成功但工作区无变更时仍以 `done` 结束，任务流水保留 `no_changes` 状态；任务流水与 `agent.py tasks --json` 的字段与状态保持桌面端已消费的格式。

### 提示词与内容隔离

- 初次接单的 Prompt 包含仓库、Issue 编号等元数据；标题与正文放在 `<<<BEGIN_UNTRUSTED_ISSUE_DATA>>>` 与 `<<<END_UNTRUSTED_ISSUE_DATA>>>` 边界内，正文中出现的相同标记会被改写为无法闭合的形式。
- `/agent` 后续 Prompt 另含受信任的前置说明（授权协作者经校验的 `/agent` 评论触发、后续请求是待实现的需求而非系统指令）、上一次结果（`status`、`branch`、`pr_url`、`last_error`）与授权评论历史；Issue 标题/正文、授权评论和后续请求同样位于不可信边界内，并受 50,000 UTF-8 字节上限约束。未授权评论、机器人评论与 Agent 标记评论不会出现在提示词中。
- 固定指令声明该内容是待处理数据而非命令，禁止读取或泄露密钥、凭据、环境变量与宿主文件，只允许在克隆目录内修改，并拒绝改变这些规则。
- 这些只是提示约束，不是操作系统沙箱：默认 root 执行与 `--yolo` 保持不变，必要时请自行收紧服务器权限。

### 任务状态与尝试日志

- 终态记录写入 `~/.config/onespace-cloud-agent/worker_state.json`，格式为 `version: 2` 加 `repositories`，按仓库与 Issue 编号字符串索引，每条记录包含 `status`（`done` / `failed`）、`attempts`、脱敏后的 `last_error` 与 UTC ISO-8601 `updated_at`。
- `last_comment_id`（十进制字符串，默认 `"0"`）、`branch` 与 `pr_url`（字符串，默认空）是 `version: 2` 的加性可选字段，缺失或为 null 时使用默认值，加载与保存原样往返。`last_comment_id` 可接受非负整数或 ASCII 十进制字符串并归一化为字符串，`branch` 与 `pr_url` 可接受字符串（null 视为默认空）；其他形状按既有非法 v2 状态隔离策略处理。
- 旧版「仓库 → 编号列表」状态按 `done`、`0` 次尝试加载；首次写入 v2 前会把原始字节一次性备份为 `worker_state.json.v1.bak`（绝不覆盖，备份失败则中止迁移写入）。损坏或形状非法的状态被隔离为带唯一后缀的 `.corrupt` 文件、记录日志并以空状态继续。保存使用同目录临时文件加 `os.replace`。
- v2 状态不向后兼容：回滚到旧 worker 之前，必须先恢复 `worker_state.json.v1.bak`，绝不能让旧版本读取 v2 状态。旧 worker 保存时会丢弃新增字段；重新升级后游标回到 `0`，历史 `/agent` 评论可能被再次评估一次，这是显式授权操作可接受的代价。
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

### `runtime`

| 字段 | 说明 |
| --- | --- |
| `runtime.disk_check_interval_seconds` | 磁盘巡检间隔，默认 `300` 秒；每个周期执行一次 `DiskGuardian.run_routine_check`，任务执行期间同样继续巡检。 |
| `runtime.shutdown_grace_seconds` | 收到 `SIGTERM` / `SIGINT` 后的关闭宽限期，默认 `300` 秒。 |

### 进程生命周期与关闭

- `SIGTERM` / `SIGINT` 的处理函数只记录停止状态与一个共享的单调时钟关闭截止时间，不阻塞、不直接退出；此后不再开始新的 Issue，也不发起新的重试，当前任务在宽限期内仍可使用网关。
- 任务提前完成时立即正常退出。到达截止时间时只终止当前任务自有的 Issue 子进程组，并写入一条 `failed` 关闭终态（`shutdown grace period expired`，带实际已消耗的尝试次数）。
- 命令超时、退避与通知等待只消耗剩余宽限预算；预算不足时记录跳过投递，不声称已送达。状态写入失败按实记录，不伪造成成功持久化。
- 独立于守护进程的 SSH / tmux 会话不受影响。正常信号关闭以 `0` 退出；网关线程意外退出会请求非零退出，由 systemd 按 `Restart=always` 重启。
- systemd 单元设置 `KillMode=mixed` 与 `TimeoutStopSec=infinity`，systemd 不会以自身默认停止超时或初始信号抢占宽限期：有限宽限由守护进程自己掌握，leader 退出后由 systemd 清理残余服务进程。

### `guardian`

| 字段 | 说明 |
| --- | --- |
| `guardian.data_roots` | OpenCode 数据根目录列表，默认 `~/.local/share/opencode` 与 `~/.cache/opencode`。 |
| `guardian.cache_retention_days` | 缓存保留天数，默认 `7`；常规清理只删除 `log` 与 `tool-output` 下早于该期限的条目。 |
| `guardian.cleanup_sessions` | 是否允许应急清理历史会话文件，默认 `false`；设为 `true` 时才额外允许删除 `sessions` 下超过保留期的旧文件，数据库、仓库与快照仍保留。 |
| `guardian.worktree_retention_days` | 共享 Worktree 的过期清理阈值，默认 `7` 天，按 Worktree 目录 mtime 判断。 |

### Worktree 与会话清理

- 终止会话（`cloud-session kill`）后，只有同时满足以下条件的 Worktree 才会以非强制 `git worktree remove` 移除：位于 `<workspace_root>/*-worktrees` 下、没有被其他 tmux 面板使用（按面板当前路径判断，包含其子目录）、且工作区干净（含未跟踪文件）。分支保留，不会清理无关注册。
- 周期性过期清扫只处理已注册、干净、无会话使用、且目录 mtime 早于 `guardian.worktree_retention_days` 的 Worktree；脏工作区、仍被使用、未注册、位于受管根目录之外或无法检查的目录都会保留并记录日志。
- 磁盘守护只修剪白名单缓存：在 `guardian.data_roots` 的 `log` 与 `tool-output` 中删除超过 `guardian.cache_retention_days` 的条目；`guardian.cleanup_sessions` 默认为 `false` 时保留全部会话文件，只有显式设为 `true` 才额外允许删除 `sessions` 下超过同一保留期的旧文件。`opencode.db*`、`repos` 与 `snapshot` 在任何模式下都保留，符号链接不会被跟随到数据根目录之外。
- 单个条目删除失败只记录日志并继续处理其他条目；剩余空间低于 2 GB 时记录告警，但不会因此缩短保留期。任务自有临时目录的清理与共享 Worktree 清理相互独立。

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
- 双参数会在 `<workspace_root>/<repo>-worktrees/<清洗后的分支>` 创建或复用 Worktree，并创建 `dev_<repo>_<branch>` 会话；目录名沿用既有分支清洗规则，交付给 Git 的分支仍保持调用者给出的原值。
- 分支以 `-` 开头时在任何 Git 调用前被拒绝，其余分支用 argv `git check-ref-format --branch` 校验，因此包含空格等非法分支会由 Git 校验失败拒绝而不会进入 shell；路径与合法分支始终作为字面 argv 元素传递。仅当 Git 注册的路径与分支都与目标一致时才复用或恢复既有 Worktree，未注册目录或创建失败残留的目录会直接报错，不会启动 OpenCode。
- 会话中的 `opencode` 优先从当前 PATH 解析，未找到时回退 `/root/.opencode/bin/opencode`；默认初始化命令恰为 `shlex` 转义后的可执行文件加 `--yolo`，项目路径只通过 tmux `-c` 传入。`--no-yolo` 与 `--extra-args` 经 `shlex.split` 后再转义保留，不存在兜底 shell 命令，OpenCode 退出后会话也不会回落到 shell。
- 会话终止或过期清扫时的 Worktree 清理规则见「Worktree 与会话清理」：只移除受管根目录内、无其他面板使用且干净的已注册 Worktree，并保留分支。
- 进入 `opencode` 后下达任务，直接断开 SSH 或合上屏幕，云端会继续执行；再次执行相同命令即可恢复现场。

### 场景 2：24/7 代码库监控与自动提 PR

1. 在 `config.json` 的 `watch_repositories` 中加入要监听的 GitHub 仓库。
2. 确保 GitHub 已通过 Personal access token (classic) 授权，且 Token 具备 `repo`（如需修改工作流还需 `workflow`）权限。
3. 在目标仓库创建 Issue，并打上 `bot:task` 标签。
4. Cloud Agent 会先校验 Issue 作者是否具备仓库 `admin` / `maintain` / `write` 权限，通过后自动接单并在 Issue 回复；随后克隆仓库、调用 `opencode run` 实施修改、提交 `bot/issue-<编号>-<时间戳>` 分支、创建 Pull Request，并通过钉钉/企微群通知结果。已存在引用该 Issue 的 open PR 时，初次接单会跳过重复执行。
5. 需要拒绝或调整时，保持 Agent PR 为 open 且不合并，再由 `admin` / `maintain` / `write` 协作者在 Issue 上评论 `/agent` 并说明修改要求。后续任务会重新克隆、以完整授权历史执行，检出该 Agent PR 的分支追加提交并推送，不创建第二个 PR；若不存在 Agent 自有 PR 且没有其他引用该 Issue 的 open PR，则从默认分支新建分支与 PR，否则只回复一次并消费本次触发。
6. 任务终态记录保存在 `~/.config/onespace-cloud-agent/worker_state.json`（`version: 2`，含尝试次数、脱敏错误、可选分支/PR 与评论游标），重启后不会重复处理已消费的评论；每次尝试的日志在 `~/.config/onespace-cloud-agent/logs/` 下，全局只保留最新 20 份。

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

首次安装生成的配置显式关闭凭据校验；启用 `ai_gateway.auth.require` 后请使用配置的 token，未写入时回退 `sk-onespace-cloud-agent`。

服务启动时以及每次模板同步后，网关会自动把模型池和活跃服务商模型同步到 `~/.config/opencode/opencode.json`，并统一使用单一 `Cloud Agent Gateway` 提供商（示例中的 `apiKey` 取自 `ai_gateway.auth.token`，未写入时为其回退值）：

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

> 桌面端「同步 AI 网关到云端」会用本地网关配置整体覆盖远端 `ai_gateway` 段，云端的自定义 `auth`、`max_body_bytes` 与 `retry.max_cooldown_seconds` 会被重置为默认值，需要在该操作后重新补回。

---

## 本地冻结计划执行

桌面端 Cloud Agent「新任务」中的本地计划模式可选择已保存工作区及其 `.ai-workflow/plans/` 下的冻结计划，由云端执行并把结果交付为待人工审阅的 PR。

- 支持范围：仅单仓库计划——未拆分的冻结计划，或带有冻结任务集与 `tasks/execution-order.yaml` 的拆分计划；`workspace_repos` 中出现参与的子仓库时拒绝。计划还必须从未在本地开始（没有 `implementation.yaml`）。
- 代码来源：云端为每次执行新建自有克隆，先 `git fetch` 选定远端分支并记录完整基线 SHA；本地未提交改动与未推送提交都不上传，也不会隐式推送本地工作区。
- 上传与去重：整个计划目录按字节上传。在单个云端主机上，规范化 GitHub 或 GitHub Enterprise 仓库身份（统一 SSH/HTTPS 写法、主机大小写与 `.git` 后缀，并经 `gh repo view` 确认）加冻结计划 ID 标识一个任务；相同基线与字节的重复提交返回既有任务及其 PR/状态，基线或字节变化会被拒绝而不是覆盖已受理内容。桌面端在上传前生成并复用同一 `request_id`，用于丢失确认后的重试。
- 根路径映射：执行上下文声明原始本地项目根与等价云端根；任务 scope 与命令中的原根前缀在拼装时映射到云端克隆，冻结计划的字节与摘要保持不变。映射之外或仅本机可用的前置条件会阻塞并报告，而不会扩大范围或改写冻结文件。
- 云端前置条件：必须预先安装 `ai-workflow` CLI、`coding` skill、原生角色、Python、`tmux`、Git、已认证的 `gh` 与 OpenCode v1（以 `opencode-ai` 1.19.5 为基线，启动前探测 `/session`、`/permission`、`/question` 等接口）；服务不会安装或升级这些工具。
- 自动回答：提交即把本任务的工具授权委托为单次 `once` 批准，原生 `deny` 仍然生效，绝不设置全局 `always` 或修改宿主权限。问题只在每个问题均为单选且恰有一个选项标签以 `(Recommended)`、`(推荐)` 或 `（推荐）` 结尾时自动返回该原始标签；这是纯语法规则，不做语义筛选（包括提及范围或验收的问题），其余问题保持等待。
- 接管与取消：等待中的问题显示在任务流水线，可用 `opencode attach <url> --session <id> --dir <cloud-root>` 附着到同一会话人工回答。取消只终止该任务自有的进程并保留工作区；发布已开始后返回真实终态，而不是假装撤销已发生的副作用。
- 完成证据：只有观察到成功的 `coding` skill 调用、编码自行写出的 `implementation.yaml` 为 `completed`、其完整 SHA 等于交付分支 HEAD 且工作树干净才算完成。与基线无源码差异时以 `no_changes` 结束，不建空提交或 PR；否则普通非强制推送自有交付分支并创建一个 PR，创建前及响应不确定时按仓库/head/base 复用匹配 PR，绝不自动合并。
- 状态与恢复：任务状态位于云端 `~/.config/onespace-cloud-agent/plan-tasks/<job-id>/`（`task.json`、`request.json` 与不可变的 `upload/`）。关闭 OneSpace 或断开 SSH 不会停止已受理任务，重连后读取云端权威状态；控制器异常消失记为 interrupted 失败且不自动重跑。恢复或重新执行需要人工处理或新的冻结计划 ID。
- 回滚：先停止或取消自有计划任务，再回退服务端代码；保留上传、任务状态、克隆与已发布 PR 供排查，不自动删除远端分支或关闭 PR。

> 提示词与工作流约束（包括权限回答规则）都不是操作系统沙箱；云端仍按既有执行权限运行，主机加固仍是操作者职责。本地模拟与无 provider 的协议冒烟不构成真实 Linux 主机与真实模型端到端运行的证据，后者仍是单独的部署检查。

---

## 常用管理命令

```bash
# 查看云端守护服务运行日志
ssh dmit-server "journalctl -u onespace-cloud-agent -f"

# 查看/重启云端服务
ssh dmit-server "systemctl status onespace-cloud-agent"
ssh dmit-server "systemctl restart onespace-cloud-agent"

# 优雅停止：按 runtime.shutdown_grace_seconds 结束当前任务后退出
ssh dmit-server "systemctl stop onespace-cloud-agent"

# 编辑配置
ssh dmit-server "nano /root/onespace-cloud-agent/config.json"

# 查看 GitHub 授权状态
ssh dmit-server "gh auth status"

# 查看活跃的云端会话
ssh dmit-server "cloud-session list"

# 检查网关健康状态与磁盘空间
ssh dmit-server "curl -s http://127.0.0.1:17688/health && df -h /"
```

状态自检直接调用已安装的 `agent.py`，并用配置中的 `ai_gateway.port` 请求回环 `/health`：

```bash
ssh dmit-server "/usr/bin/python3 /root/onespace-cloud-agent/agent.py status /root/onespace-cloud-agent/config.json --json"
```

`service_active` 只有在 `systemctl is-active` 返回 `active` 时才为 `true`；systemd 不可用、命令异常或服务非活跃都会报告 `false`，不会因为无法确认而回退为在线。安装目录不同时请替换路径。

---

## 运行测试

在 `server/onespace-cloud-agent` 目录下执行：

```bash
python3 -m unittest discover -s tests -v
```

这些测试使用伪时钟、伪造线程与伪进程在本地运行；`tests/test_deploy.py` 以伪 SSH/SCP、伪 apt/curl 与伪 systemd 执行真实脚本，覆盖前置检查边界、缺失依赖安装、已安装依赖复用、含空格路径、配置按字节保留、快照恢复、激活/记录失败边界与上传载荷清单，但不连接任何真实服务器。真实 Linux 主机上的 systemd 启停、优雅关闭与部署冒烟仍需在单独授权的目标上执行，不能由本地测试结果替代。
