# Agent Note: Cloud Agent Hardening Guarantees

Status: implemented

[English](2026-10-29-cloud-agent-hardening.md) | 中文

## Problem

自托管的云端智能体在一台经 SSH 访问的小型 Linux 主机上运行 OpenAI 兼容网关、GitHub Issue 自动化、长期存活的 tmux/OpenCode 会话、磁盘维护与部署。加固之前，网关接受未认证请求与浏览器 Origin 请求并回以通配符 CORS，把未知模型名广播给全部活跃服务商，每个候选只尝试一个 key 且会复用冷却中的 key，并以阻塞缓冲读取上游流、延迟小数据块。Issue 自动化对任何带标签的 Issue 直接执行而不校验作者的仓库权限，把 Issue 正文当作指令拼入提示词，对瞬时失败与永久失败不加区分且没有重试预算，PR 创建响应丢失时可能重复执行，只保留旧版已处理列表而没有原子写入或损坏处理，并写入无上限、可能泄露凭据的执行记录。会话命令拼接 shell 字符串，不检查 Worktree 注册即复用目录，清理默认删除会话数据。守护进程依赖 systemd 的默认停止行为，部署在校验远端配置之前就安装代码。

## Decision

网关在任何其他处理之前完成认证与 Origin 拒绝。`GET /models`、`GET /v1/models`、`POST /chat/completions` 与 `POST /v1/chat/completions` 需要 `Authorization: Bearer <token>`，以常量时间比对 `ai_gateway.auth.token`（默认 `sk-onespace-cloud-agent`）；`/health` 与 `/v1/health` 保持公开，`ai_gateway.auth.require = false` 只跳过凭据校验。任何携带 `Origin` 请求头的请求，包括空值与 `/health`、`OPTIONS`，都在认证、读取请求体与转发之前返回 403；不带 `Origin` 的 `OPTIONS` 返回 204，且任何响应路径都不下发通配符 CORS。声明体大于 `ai_gateway.max_body_bytes`（默认 10485760 字节）时在读取与上游调用之前返回 413；`Content-Length` 缺失、为零、负数或非数字，以及空请求体与非法 JSON，返回 400。

模型解析是严格的：只有模型池名、活跃服务商的 `provider/model` 前缀以及活跃服务商上的显式映射会被解析。未知、空与纯空白模型名返回 404 与 `model_not_found`，绝不访问上游；已知模型池没有可用候选时返回 502，使运维不可用与未知模型名保持可区分。模板同步构建一个替换用目录代次并加锁发布，因此解析、列表与 OpenCode 投影各自读取一个完整代次；服务商 key 状态跨代次共享，发布绝不重置冷却。

故障转移先耗尽同一服务商再前进。遇到 HTTP 429、任意 5xx 或传输失败时，当前 key 被冷却，先尝试该服务商其余可用 key，再切换到下一个候选；每个 key 在每个候选内至多尝试一次，且绝不为冷却中的 key 等待或复用它。冷却基于单调时钟：默认 HTTP 失败 60 秒、传输失败 30 秒，正有限数值的 `Retry-After` 覆盖默认值，上限为 `ai_gateway.retry.max_cooldown_seconds`（默认 600），非法值回退默认。当匹配候选的全部已配置 key 都在冷却时，网关返回 429，`Retry-After` 取最小剩余冷却的上取整值，且不发起任何上游调用。

流式中继保持有界。中继以 `HTTPResponse.read1` 和 65536 字节读取容量读取，并立即转发当前可用字节而不是等待填满整块，同时保留短尾块。响应头提交后，上游读取失败只关闭下游：不再写入第二个 JSON 响应体，也不经其他候选重放。下游写出或刷新失败会结束中继并关闭上游，且不冷却该 key。

Issue 执行在任何工作区、分支、评论与通知之前完成授权。`issue_worker.require_author_permission` 默认为 true，只有有效仓库权限为 `admin`、`maintain` 或 `write` 才允许处理；`maintain` 归一化为 `write`、`triage` 归一化为 `read`，其他任意角色名不构成授权。权限查询按仓库与作者在单调时钟上缓存 `issue_worker.permission_cache_seconds`（默认 600）；查询失败按未授权处理（fail closed）、记录日志且绝不缓存为授权，被拒绝的预检不消耗尝试也不写入已处理状态。启动时 worker 执行不带 `--show-token` 的 `gh auth status`，在认证失败或作用域过大时告警，文档记录的最小权限指引为：`repo`，仅在修改工作流时加 `workflow`，仅在组织仓库时加 `read:org`。

Issue 内容按不可信数据处理。标题与正文位于显式不可信数据边界内，正文中出现的相同标记无法闭合该边界，并附带仓库与 Issue 元数据；固定指令声明该文本是数据而非命令，禁止读取或泄露密钥、环境变量、宿主文件与凭据，并把修改限制在克隆目录内。这些是提示约束，不是操作系统沙箱；root 执行与 `--yolo` 保持不变，主机加固仍由操作者负责。重试经过分类且有界：瞬时超时、克隆/推送/网络失败、OpenCode 非零退出与上游 5xx 最多消耗 `issue_worker.max_attempts` 次总尝试（默认 3），退避为 `min(30 * 2 ** (attempt - 1), 300)` 秒；401/403、仓库或 Issue 不存在、Issue 已关闭属于永久失败，只消耗一次尝试。每次克隆前都以数字边界在完整分页的 open PR 列表中检查引用该 Issue 编号的正文，并在重试前与 PR 创建失败后重复检查，因此响应丢失不会导致第二次执行。

终态记录是持久的。`~/.config/onespace-cloud-agent/worker_state.json` 保存 `version: 2` 记录，按仓库与 Issue 编号字符串索引，包含 `status`（`done` 或 `failed`）、整数 `attempts`、脱敏后的 `last_error` 与 UTC `updated_at`；旧版「仓库→编号列表」按 done、0 次尝试加载，首次写 v2 前把原始旧版字节一次性保留为 `worker_state.json.v1.bak`，保存使用同目录临时文件加 `os.replace`，损坏或形状非法的状态被隔离为带唯一后缀的文件后才使用空状态。尝试记录位于 `~/.config/onespace-cloud-agent/logs/`，记录开始时间、命令参数、退出码与分类、有界的 stdout/stderr 以及 `git diff --stat` 或克隆前明确的不可用说明，脱敏配置的 Token、凭据类环境变量值与 `Authorization` 值，单个文件上限 2097152 字节，并只保留全局最新的 20 份尝试记录且不影响无关文件。终态通知对传输错误、429 与 5xx 最多总尝试三次，退避 1 秒与 2 秒，其他 4xx 不重试，钉钉与企业微信通道互相独立，绝不改变任务结果，并从错误中脱敏 webhook 凭据。

会话与 Worktree 使用字面 argv 与显式归属。以 `-` 开头的分支值在任何 Git 调用前被拒绝，其余原始分支经 `git check-ref-format --branch` 校验；路径与合法分支以字面参数交给 Git，OpenCode 可执行文件从 PATH 解析并保留既有 `/root/.opencode/bin` 回退，默认 tmux 初始化命令恰为转义后的可执行文件加 `--yolo`，项目路径只经 tmux `-c` 传入，`--no-yolo` 与额外参数经 `shlex.split` 再转义且没有兜底 shell。仅当 Git 同时注册了匹配的路径与分支时才复用或恢复 Worktree。清理从每个 tmux 面板路径推导 Worktree 归属，只移除 `<workspace_root>/*-worktrees` 下已注册、干净且无会话使用的 Worktree，不使用强制移除且不删除分支；过期清扫只考虑目录 mtime 早于 `guardian.worktree_retention_days`（默认 7 天）的已注册、干净且无会话使用的 Worktree。

数据清理默认保留会话。`guardian.data_roots` 默认为 `~/.local/share/opencode` 与 `~/.cache/opencode`，常规清理只删除 `log` 与 `tool-output` 下早于 `guardian.cache_retention_days`（默认 7 天）的过期条目，并且不把符号链接跟随到这些边界之外。`guardian.cleanup_sessions` 默认为 false 并保留全部会话文件；显式 true 才额外允许以同一保留期删除 `sessions` 下过期的旧版文件，`opencode.db*`、`repos` 与 `snapshot` 在两种模式下都受保护。删除失败会记录日志并继续处理，低剩余空间只在既有 2 GB 阈值上告警，不缩短保留期。

守护进程掌握关闭流程。磁盘巡检每 `runtime.disk_check_interval_seconds`（默认 300 秒）执行一次，任务执行期间同样进行；网关线程意外退出会请求非零进程退出，交由 systemd 重启策略处理。`SIGTERM` 与 `SIGINT` 只记录停止状态与一个共享的单调时钟截止时间：不再开始新的 Issue 或重试，网关对当前任务保持可用最多 `runtime.shutdown_grace_seconds`（默认 300 秒），任务提前完成时正常退出，到期只终止自有的 Issue 子进程组并写入一条带实际尝试次数的 `failed` 关闭终态。命令超时、退避与通知等待只消耗剩余预算；预算不足时记录跳过投递，状态写入失败按失败报告而不声称已持久化，独立 SSH/tmux 会话不受影响。systemd 单元使用 `KillMode=mixed` 与 `TimeoutStopSec=infinity`，使自身默认停止行为无法抢占守护进程掌握的宽限期。

部署先校验再发布。`deploy.sh` 先运行 `bootstrap.sh` 完成平台门禁（Linux、root、可访问 systemd、amd64/arm64，且仅在缺少运行时依赖时才要求 Ubuntu 22.04/24.04 或 Debian 12），并只从官方渠道安装缺失的 `python3`、`git`、`tmux`、`gh` 与 `opencode`，保留既有可执行文件；GitHub 认证不是门禁。随后它上传到同一文件系统的暂存目录，并在任何安装、启动器改动或重启之前用 `python3 -m json.tool` 校验既有远端配置或随包默认配置。它把当前安装快照到配置的 Agent 目录之外，在该配置路径发布暂存目录，发布失败时恢复快照，保留既有配置字节，仅在首次安装时从 `config.default.json` 创建 `config.json`，按配置目录生成启动器与 systemd 单元，并仅在重启确认服务处于 active 之后写入携带包 `VERSION` 与 UTC `installed_at` 的 `install-info.json`。只有激活与元数据发布都成功后，才把快照修剪到最新三份；预检、校验、上传或发布失败时，活跃安装与既有快照都不被改动，不发布成功记录，也不会产生虚假的首个备份。桌面端的一键检查与部署交接，包括共享的包 `VERSION` 以及未知版本与未记录时间的区分，记录于 [Cloud Agent Desktop Deployment](../feature/2026-10-30-cloud-agent-desktop-deployment.md)。

## Alternatives considered

- 在桌面端修复同步或增加独立的云端覆盖存储：未采纳，因为桌面改动与第二份配置权威都在本服务之外，因此把重置记录为该独立操作之后的操作步骤。
- 以非 root 用户运行服务或放入 OS 沙箱：未采纳，因为这会改变保留的 root 与 `--yolo` 执行模型及其运维前提；提示约束被记录为隔离手段，而非沙箱。
- 并发调度多个 Issue：未采纳，因为一次一个 Issue 让单台小型主机保持可预测，且既有调度面保持不变。
- 引入新的配置或持久化框架：未采纳，因为标准库原语、既有 JSON 文件与原子替换已覆盖这些有界需求。
- 增加自动生产回滚命令：未采纳，因为恢复保留操作者选择的快照与显式验证，而不是猜测要还原的版本。
- 为浏览器客户端放宽请求体或认证规则：未采纳，因为浏览器 Origin 访问正是本次移除的攻击面；只有不带 Origin 的 `OPTIONS` 返回 204。
- 默认删除会话数据：未采纳，因为会话属于用户数据；应急删除需要显式开关与操作者备份。

## Consequences

- 当前默认值：网关仅监听回环地址的 17688 端口，token 为 `sk-onespace-cloud-agent`，请求体上限 10 MiB，冷却 60/30 秒且上限 600 秒，Issue 轮询 180 秒，总尝试 3 次，缓存与 Worktree 保留 7 天，磁盘巡检 5 分钟，关闭宽限 300 秒，会话清理关闭。
- 网关客户端必须在四个受保护路由上发送 Bearer 凭据；生成的 `provider.gateway` 凭据使用当前配置的 token，因此自定义 token 必须两端一起更新。
- 数据归属与恢复：worker 状态、尝试记录与任务流水都位于 `~/.config/onespace-cloud-agent/` 下，旧版备份只写一次且绝不覆盖，损坏状态被隔离而不是被静默覆盖。
- v2 状态不向后兼容：旧 worker 必须收到已还原的 `worker_state.json.v1.bak` 或已记录的空白状态，绝不能读取 v2 文件。
- 同目录的 `opencode.jsonc` 被逐字节保留并给出警告；合并或删除它仍是操作者的手工操作，因为 OpenCode 可能优先读取它。
- 既有桌面端网关同步仍会整体覆盖远端 `ai_gateway` 段，因此自定义认证、请求体上限与冷却设置必须在其后重新补回。
- 应急删除的会话无法通过回退代码找回，需要操作者的数据备份；常规清理不删除任何会话数据。
- Linux systemd 冒烟检查保持显式：本地测试使用伪时钟、伪线程与伪传输，不证明真实主机上的服务启停、停止超时协作、快照保留或首次部署。
- 本决策的验证为完整的服务端 Python 单元测试套件，加上笔记校验与工作流计划校验，以及已记录的部署检查；除已声明的界限、上限与保留期外，不声称任何内存、延迟或磁盘容量保证。
- 相关活跃笔记：没有需要本记录整体或部分取代的记录。
