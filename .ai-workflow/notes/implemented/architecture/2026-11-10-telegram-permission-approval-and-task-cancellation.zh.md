# Agent Note: Telegram Permission Approval and Task Cancellation

Status: implemented

[English](2026-11-10-telegram-permission-approval-and-task-cancellation.md) | 中文

## Problem

Telegram 已可通过 `/planning` 与 `/coding` 启动规划与编码，但 OpenCode 子进程发出的权限请求没有远程审批路径：无人值守时未答复的请求会阻塞任务，而默认批准一切又会静默授权破坏性的 shell 命令。`/cancel` 过去只清除桥接状态，本地子进程可能继续运行，迟到结果仍可能被报告为成功。托管会话还有相关缺口：回答注入失败会丢失提问，完工判定可能在 agent 仍等待答复时就报告成功。

## Decision

`src-tauri/src/remote_bot/permission.rs` 提供纯分类器 `classify_permission(permission_type, command)`，在不读取设置、不做 I/O 的情况下返回 `PermissionRisk::Routine` 或 `PermissionRisk::HighRisk`。只有显式白名单算作常规：shell 请求中的只读巡检、测试/构建/lint/格式化工具、不含 `--amend` 提交的安全本地 Git、`mkdir`/`touch` 与 `cd`/`sleep`，以及非 shell 的权限类型 `read`、`list`、`glob`、`grep`、`edit`、`write`、`patch` 与 `apply_patch`；后者在开启自动批准时不发卡片直接批准。复合片段、替换、未列入的重定向、包装器、未知命令或权限类型以及空输入都属于高危，因此歧义输入一律 fail closed。shell 命令开头的任何 `KEY=VALUE` 环境变量赋值同样属于高危并 fail closed，因为赋值键可改变命令解析、执行钩子或解释器与构建行为，普通赋值因此不再被当作透明的包装器。

`opencode_bridge.rs` 在每次请求时读取持久化配置中的 `auto_approve_permissions`。开启时常规请求直接回 `once` 且不发卡片；开启且高危以及关闭时的全部请求，桥接都会发送 Telegram 审批卡片并在内存中跟踪该请求。卡片投递失败绝不自动批准，请求保持待决策直到超时。高危卡片只提供 Allow once 与 Deny；常规卡片另有 Always allow。首个决策回复 `once`、`always` 或 `deny`，改写卡片并使后续点击成为空操作；Always 转发给 OpenCode，OneSpace 不保存第二份授权，高危 Always 回调被忽略。`PERMISSION_TIMEOUT` 为 30 分钟：轮询清理到期请求并回一次 `deny`，只有 deny 回复成功才提交这次超时，改写卡片并通知聊天；回复失败则保留该请求及其原始期限，由后续轮询重试，且不改写卡片也不通知聊天。已决、已取消或任务结束的请求不会超时，迟到点击被忽略。

桥接任务在 Unix 上运行于自有进程组（`process_group(0)`）。`/cancel` 先发 SIGTERM，并在独立的 2 秒 `CANCEL_GRACE` 内升级为 SIGKILL，如实报告 `Cancelled`、`ForcedCancelled` 或 `NothingToCancel`。读取循环观察取消标记，任务世代守卫抑制迟到的成功或失败卡片，待决权限与提问卡片改写为已取消，任务状态与待决权限被清空。外部与托管会话以及共享 `opencode serve` 不受影响。管理器停止、重载、禁用、重启或保存以及应用退出都会调用 bridge `shutdown`，一并终止桥接自有的 `opencode serve`；`shutdown_runtime_services` 调用 `remote_bot::shutdown_runtime()`。

托管提问先注入、仅在成功后清除，因此失败或重复回答仍可重试，而数据库 call id 已消失的内存提问会被清除。内存或数据库存在待答提问时空闲计数复位且跳过完工判定。`HostedSessionInfo.active_question_summary` 显示于 `/host` 与 `/sessions <hosted session>`，并提供 `host_question:<session_id>` 的 "View pending question" 动作，重发当前卡片或报告提问已过期。

## Alternatives considered

- 就地批准每一个权限。已拒绝：轮询看到请求时高危命令会被静默授权，操作者被排除在真正重要的决策之外。
- 按聊天维护 OneSpace 白名单或第二份授权存储。已拒绝：这会重复 OpenCode 自身的会话记忆，并形成两个可能在动作是否仍被批准上互相矛盾的权威。
- 可配置超时或按请求的过期 UI。已拒绝：固定的 30 分钟预算正是防止未答复请求无限阻塞 agent 的边界；暴露它等于允许事实上无限等待。
- 只设内存取消标记而不终止子进程。已拒绝：孤儿进程会在聊天被告知任务已停止后继续写文件并占用资源。
- 终止任务但保留桥接自有的服务。已拒绝：停止、重载与退出必须不留下桥接拥有的进程，且存活的服务会继续暴露旧项目目录。
- 在注入回答之前移除托管提问。已拒绝：注入失败后会销毁 agent 正在等待什么的唯一记录，且没有重试路径。
- 在提问待答时把托管会话标记为完成。已拒绝：这会在 agent 仍被阻塞时报告成功，并隐藏需要回答的信号。

## Consequences

- 非 shell 的 `read`、`list`、`glob`、`grep`、`edit`、`write`、`patch` 与 `apply_patch` 请求算作常规，`auto_approve_permissions` 开启时自动批准；歧义 shell 输入按高危 fail closed，白名单只能通过显式修改分类器与配套测试扩充。
- 未答复请求在 30 分钟后被拒绝一次，卡片与聊天都显示超时；它绝不会无限待决。
- 常规卡片可授予 Always，但高危路径从不提供，并忽略过期或伪造的高危 Always 回调。
- 待决权限、提问、任务世代与取消状态仅存内存；`auto_approve_permissions` 仍是唯一设置，无 schema 或迁移变更，Telegram 回调为增量新增。
- `/cancel` 只终止桥接自有的任务进程组；外部与托管会话以及共享 `opencode serve` 继续运行，结果文案区分正常取消、强制终止与无可取消。
- 管理器生命周期操作与应用退出会终止桥接自有的任务与服务，空闲桥接的关闭是幂等的。
- 托管提问在注入失败后仍保留，并在回答或判定过期之前阻止完工判定，待答提问在 `/host` 与会话详情中持续可见且可重新发送。
- 行为由 `src-tauri/src/remote_bot/tests.rs` 中的 `permission_*`、`cancel_*` 与 `hosted_*` 测试以及聚焦的 `RemoteBotSettingsPanel.test.tsx` 设置面板测试覆盖；设置文案说明开关行为、30 分钟超时、高危不提供 Always 以及保存或重启会终止当前远程任务。
