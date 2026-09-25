# Agent Note: Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast

Status: implemented

[English](2026-09-23-gateway-auto-disable-recovery.md) | 中文

## Problem

2026-09-20 交付的逐映射运行健康把失败的模型移出服务，同时把恢复留给操作者：被失败阈值自动禁用的行只能经 `ai_gateway_reenable_provider_model` 或 `ai_gateway_reenable_provider_models` 恢复，因此一次暂时性的上游故障可能让健康模型停摆到有人察觉为止。这一手动恢复周边还有两个缺口。Mac 从睡眠唤醒时会集中结算一批并非上游过错的网络失败，而这些失败会计入阈值并可能自动禁用行。每次结算都会静默改写持久化的运行状态，因此已打开的页面会一直显示过期的服务商卡片、模型列表与运行状态卡，直到切换页签或重启，已打开的服务商弹窗也保留过期快照。本次工作因此需要决定：被阈值禁用的行如何自行重试、检测到的系统恢复如何与上游故障区分、以及持久化状态翻转如何在不引发刷新风暴的前提下到达界面。

## Decision

只有当服务商与映射行都 `enabled`、行仍 `auto_disabled`、`disabled_at` 存在且距今至少 `AUTO_DISABLE_PROBE_COOLDOWN_SECS`（60 秒）、`consecutive_failures` 达到 `FAILURE_THRESHOLD`、trim 后 `disabled_reason` 不以 `HTTP 401` 或 `HTTP 403` 开头、trim 后 `upstream_model` 非空、trim 后 `local_model` 等于请求模型、生效协议等于入站协议、且该服务商尚未作为本次请求的健康候选时，被暂时性失败阈值自动禁用的映射行才成为半开探测候选。`selection::find_probe_candidate` 独立于 `resolve_model_for_protocol`（后者仍跳过自动禁用行）扫描，接收显式 `now` 使 60/59 秒边界确定，最多返回一个 `ProbeCandidate`——服务商、`MappingTarget` 与上游模型——按最旧 `disabled_at` 优先，依次以服务商 id、本地模型、上游模型决胜。

鉴权失败保持仅手动恢复，且判别无需持久化标记。自密钥池变更起，401/403 属 key 域：标记所尝试的上游 key 并在同一请求内轮换，绝不禁用映射行、也不登记映射健康（[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md)）；映射行的 `consecutive_failures` 因此只由 `Retryable` 失败推进，额度耗尽 429 同为 key 域失败。旧构建持久化的行可能被此前的 401/403 路径即时禁用并按其前的计数规则达到过阈值，所以资格判定另行拒绝 trim 后以 `HTTP 401` 或 `HTTP 403` 为前缀的 `disabled_reason`——这正是遗留即时禁用路径写入的格式——而任何阈值禁用都不可能产生该前缀。`FAILURE_THRESHOLD`、失败分类与既有手动恢复命令均不变。

探测优先级最低。`attempt_non_streaming` 与 `attempt_streaming` 接收可选探测，且只在全部健康候选与全部有限重试都失败之后、且进程级 `try_acquire_probe_guard` 按 trim 后映射键成功时才尝试恰好一次（`ProbeGuard` 在 drop 时释放，取消亦不例外），绝不把探测排入重试或退避；流式请求只在首字节写出前阶段探测。探测成功即服务调用方并经 `clear_mapping_runtime_state` 清除该行（`auto_disabled`、`disabled_reason`、`disabled_at`、`consecutive_failures` 与 `last_error_at` 全部重置）。任意类别的探测失败都经 `rearm_mapping_probe_cooldown` 重设冷却：行保持自动禁用、保留 `consecutive_failures`、把 `disabled_at` 移到尝试时刻；除非该失败是被系统恢复宽限抑制的传输失败，`last_error_at` 与 `disabled_reason` 也会刷新。既有的 `all_providers_unavailable` 502 会点名被探测的服务商。

零候选请求在写出响应前先判定探测资格。存在合格候选时，请求以空的健康候选列表继续、把探测作为唯一尝试，因此写入一条正常的命名探测尝试行加恰好一条终止行，而不是合成无候选行；不存在合格候选时既有的合成 502 行不变。仅探测请求既不读取也不写入会话亲和绑定。探测绝不复活 `enabled` 为 false 的行，因为资格要求服务商与行都处于启用状态；探测失败也绝不清除 `auto_disabled`。

`app_runtime::runtime_services` 持有进程级 epoch 秒恢复时间戳（`0` 表示从未检测到恢复，仅进程内存），并提供 `SYSTEM_RESUME_GRACE_SECS = 60`、`mark_system_resume()`、`system_resume_grace_active(now)`（含 `T+60s` 边界，`T+61s` 与无信号时为 false）以及 `#[cfg(test)]` setter；`app_runtime` 以 `pub(crate)` 重导出两个生产函数。`ssh_tunnels::reconnect` 在两个既有检测点——sleep-gap 心跳与 macOS `NSWorkspaceDidWakeNotification` 观察者——调度 SSH 对账之前调用 `mark_system_resume()`。`runtime_http::handle_connection` 在请求开始时一次性评定宽限。`AttemptResult::Failure` 携带 `transport` 标志：网络发送、正文读取、流打开与中途流读取的错误路径置为 true，HTTP 状态失败保持 false；`RequestHealth` 保存宽限结果，`record_failure` 因此只在窗口内、类别为 `Retryable` 且属传输失败时跳过计数：`consecutive_failures` 与 `last_error_at` 保持不动、不会发生自动禁用。HTTP 状态失败（5xx）与由窗口外或无信号请求结算的失败照常计数；401/403 与额度耗尽 429 属 key 域失败、绝不登记映射健康（[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md)）；被抑制的探测失败只刷新 `disabled_at`。

`ai_gateway.rs` 定义 `AI_GATEWAY_CONFIG_UPDATED_EVENT` = `ai-gateway-config-update`。`runtime_http::start_server` 接收可选的 `tauri::AppHandle` 并存入进程级槽（`None` 清空该槽），`autostart` 转发句柄；`commands.rs` 暴露 `start_inner(app)`，Tauri 命令 `ai_gateway_start` 与 `ai_gateway_autostart` 作为薄封装新增注入的 `tauri::AppHandle`——命令名与前端参数形状不变，`ai_gateway_stop` 不受影响。整配置 `ai_gateway_save_config` 命令（含 `save_config_inner` 及其注册）已由版本门控清理移除（[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)），保留的配置写入保持同一条单次翻转广播规则。在一次成功写入翻转任意映射行的 `auto_disabled`（双向：阈值自动禁用与探测恢复）之后恰好发出一个事件。仅计数结算、探测重设冷却、key 运行时标记（绝不发出配置更新事件）与写入失败都不发事件；未捕获句柄时发送为空操作，绝不影响结算。

`src/lib/aiGateway.ts` 导出对应的 `AI_GATEWAY_CONFIG_UPDATED_EVENT`，AI 网关页以 `listen` 订阅（非 Tauri 环境跳过、卸载时释放 unlisten），使既有 `load()` 无需切换页签或重启即可刷新服务商卡片、模型列表与运行状态卡。网关页把最新配置中的对应服务商作为 `runtimeProvider` 传给服务商弹窗。`ProviderDetailDialog` 只把 trim 后 `(local_model, upstream_model)` 键匹配的行的五个运行时字段（`auto_disabled`、`disabled_reason`、`disabled_at`、`consecutive_failures`、`last_error_at`）合并进本地草稿，逐字段比较使无变化的快照不触发重渲染，绝不重置用户可编辑字段或未保存编辑，并由本地草稿派生自动禁用提示计数。

## Alternatives considered

- 保持恢复仅手动、依赖操作者发现自动禁用行：未采纳，因为暂时性上游故障随后会让健康模型一直不服务直到有人重新启用，而这正是本次变更要关闭的缺口。
- 同样探测被 401/403 即时禁用的行：未采纳，因为被拒绝的凭据不经操作者处理无法恢复，探测只会消耗延迟并制造日志噪音；同一理由如今支配有序密钥池中 auth 标记的 key，reason 前缀让旧构建写入的鉴权禁用行留在仅手动恢复集合内（[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md)）。
- 持久化一个标记行如何被禁用的判别字段：未采纳，因为它会给 `ai_gateway.json` 增加持久化字段与迁移，而计数语义加 `HTTP 401`/`HTTP 403` 的 reason 前缀已在两个方向上区分阈值禁用与鉴权禁用。
- 把探测追加进加权候选顺序：未采纳，因为探测绝不抢先于或延迟健康候选，注入的探测会扭曲平滑加权分布；探测只在全部健康候选与有限重试都失败之后运行。
- 在多个服务商同时失败时推断本地网络故障并抑制计数：未采纳，因为网关无法把本地网络失败与多个真实的上游失败区分开，误判会掩盖真实失败；显式的系统恢复信号是唯一的抑制触发条件。

## Consequences

- 被阈值禁用的行无需操作者介入即可恢复服务：60 秒冷却后，下一次需要它且任何健康候选都无法服务的请求会恰好探测它一次；成功即服务调用方并清除该行，失败则重新开始冷却，调用方收到既有的 `all_providers_unavailable` 502 并点名被探测的服务商。用户的 `enabled` 意图、手动重新启用命令与行的远端身份均不受影响，遗留的鉴权禁用行或被用户禁用的行绝不被探测。
- 计数语义：映射行 `consecutive_failures` 只由连续 `Retryable` 失败推进；自密钥池变更起，401/403 与额度耗尽 429 属 key 域失败，绝不禁用映射行、也不递增其计数（[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md)）；阈值自动禁用仍在 `FAILURE_THRESHOLD` 为 3 时触发，受影响的结算测试已按该理由更新。
- 休眠/唤醒噪音不再自动禁用行：由检测到恢复后 60 秒内开始的请求结算的 `Retryable` 传输失败不计数、不能触发自动禁用，而 HTTP 状态失败与窗口外失败照常计数，因此窗口结束后开始的真实故障仍会禁用行。检测使用既有的跨平台 sleep-gap 心跳与 macOS 唤醒观察者；Windows 与 Linux 的唤醒通知仍不在范围内。
- 打开中的界面保持实时：每次自动禁用或恢复翻转都在成功写入配置后广播一次，AI 网关页据此重新加载配置；仅计数结算不发事件，因此繁忙网关不会引发刷新风暴，已打开的服务商弹窗只合并运行时字段并保留未保存编辑。
- 探测资格本身不新增 schema 变更：它复用 `disabled_at`、`consecutive_failures` 与 `disabled_reason` 前缀，旧构建持久化的行在 reason 以 `HTTP 401` 或 `HTTP 403` 开头时保持仅手动恢复，恢复时间戳、探测守卫与会话绑定都只在进程内存。网关级 schema 版本与其版本门控迁移由 [Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 记录，本记录描述的持久化运行状态在其下仍然有效。新的 Tauri 事件是增量式的，因此没有监听的旧前端保持现有行为，回退后的构建停止探测与广播，而持久化的运行状态仍然有效可读。
- 取代评估：部分取代 [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md)，后者被保留并交叉链接；本次变更只替换其「恢复仅手动」的陈述、其此前的 401/403 计数规则、以及其对自动禁用行「任何尝试都被排除」的未加限定的表述，而行级运行时状态、阈值规则、结算点、手动恢复命令与弹窗界面继续有效。本记录「401/403 即时禁用映射行」与「额度耗尽 429 计入映射健康」的陈述由 [Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md) 取代：该记录标记所尝试的 key 并在请求内轮换；遗留 reason 前缀判别与本记录其余决策继续有效。[Gateway Session Affinity Pins a Session and Model to One Upstream](2026-09-20-gateway-session-affinity-routing.md) 被保留并交叉链接，因为仅探测请求以空候选列表尝试探测且仍既不读取也不写入绑定，其决策继续有效。[Gateway Per-Attempt Request Logging and Stored Error Text](2026-09-20-gateway-per-attempt-logging-and-error-text.md) 被保留并交叉链接，因为零候选请求只要存在合格探测就写入正常尝试行加其终止行，而不是合成无候选行，而其粒度、仅终止行统计与已存错误文本契约继续有效。[API Gateway Smooth Weighted Round Robin Routes Requests and Fallback Candidates](2026-09-20-api-gateway-weighted-routing.md) 不受影响：探测只在加权 fallback 序列耗尽后尝试、绝不进入加权调度，因此该记录没有任何事实变化，也不记录取代关系。快速失败、用量统计、价格、模板、终端同步与双语文档记录仍不相关。
- `MEMORY.md`、`docs/USAGE.md` 与 `ai-gateway` / `ai-gateway-backend` / `App Runtime Backend` 三个导航条目在同一变更中承载探测、宽限、计数与翻转广播标准，`navigation.md` 已按权威 JSON 重新生成。
