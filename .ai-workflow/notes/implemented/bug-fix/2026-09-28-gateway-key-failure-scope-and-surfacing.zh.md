# Agent Note: Gateway Key Authentication Marks Are Credential-Scoped and Surfaced per Provider

Status: implemented

[English](2026-09-28-gateway-key-failure-scope-and-surfacing.md) | 中文

## Problem

2026-09-28 当天，两个正文从未提及凭据问题的 403 响应把健康的上游 key 标记为鉴权失败：12:53 的 `MODEL_NOT_IN_PLAN: GPT-6 Sol available in Pro and above plans or extra on demand usage` 拒绝，以及 14:12 的 Cloudflare `error code: 1010` 拒绝。被标记的 key 会被移出所属密钥池，因此两次事件都耗尽了可用凭据，并且该事件产生了 101 条已记录的 502，而这些请求本可由同一批 key 继续服务。操作者也无法看见或修复损害：无可用 key 的 502 与合成终止日志行只说 `no usable upstream key` 而没有计数，无候选请求的合成终止行存的是空错误消息，已打开的 AI 网关页面因 key 运行时标记不发配置更新事件而保持过期的密钥状态，没有任何消息或 toast 公布鉴权失败，服务商卡片也只显示 key 计数，无法查看或重新启用被标记的 key。

## Decision

`selection::is_authentication_error_message` 以显式凭据信号控制 403 的密钥标记，匹配不区分大小写——`invalid api key`、`api key not valid`、`incorrect api key`、`invalid key`、`unauthorized`、`unauthenticated`、`authentication`、`auth failed`、`invalid token`、`token expired`、`invalid credentials` 与 `invalid authorization`——缺失或空文本绝不算信号。`runtime_http::key_failure_kind` 始终把 401 标记为 authentication、把额度归类的 429 标记为 quota，仅在该谓词命中时把 403 标记为 authentication。`selection::classify_failure` 仍把 401/403 映射为 `DisableImmediately`，因此无凭据文本的 403 绝不标记、轮换、禁用或移除 key：它只把所解析的映射行以 `HTTP 403` 原因即时自动禁用，服务商的兄弟模型继续用同一批 key 服务。凭据 401/403 保持既有 key 域行为。

两个无可用 key 的原因都改为按服务商的 `excluded_key_summary`：排除的 key 按 authentication failed、quota exhausted、user disabled 的固定顺序计数，只列出非零类别，例如 `no usable upstream key (1 authentication failed, 1 quota exhausted)`；`enabled = false` 即使带有过期运行时标记也计为 user disabled；当没有任何候选服务商拥有启用 key 时，原因为 `no usable upstream key (no enabled key)`。完整的 502 消息只在某个被报告服务商排除了鉴权失败的 key 时追加 `re-enable authentication-failed keys manually in the AI Gateway`。`ForwardCapture` 现在携带该 502 消息，`synthetic_terminal_row` 记录它，因此无候选路径与无已完成尝试路径——非流式与流式一致——写入非空的终止 `error_message`，而不是空消息。

`ai_gateway.rs` 声明 `AI_GATEWAY_KEY_AUTH_FAILED_EVENT = "ai-gateway-key-auth-failed"` 与 snake_case 的 `GatewayKeyAuthFailedPayload`（provider_id、provider_name、key_id、key_name、reason、marked_at）。`persist_key_runtime_state` 在变更前后比较持久化的 key，并恰好在持久化状态真实变化时发出既有的 `ai-gateway-config-update` 事件——标记、TTL 清除与 key 探测重设，非流式与流式路径一致——而没有任何变化的变更不发事件。从非鉴权类别转入 authentication 标记时，该处集中发出通知事件并恰好构建一条消息中心输入：source `ai_gateway`、category `key_auth_failed`、severity warning、本地化标题 `AI 网关密钥鉴权失败` / `AI Gateway key authentication failed`、点名服务商、key 与脱敏原因的本地化摘要、按服务商的去重键 `ai_gateway_key_auth_failed:<provider_id>`，以及目标页签 `ai-gateway`（实体 id 为服务商 id）；去重窗口内的重复转换只递增该条目的出现次数，而不是新增条目。quota 标记只产生配置更新事件——没有通知事件、负载或消息。任何负载、消息、日志行或 toast 都不携带 key 值。`messages.rs` 新增运行时无关的 `create_message_with_app` 与 `list_messages_with_app`，`messages_create`、`messages_list` 与 `record_message_silent` 都委托给它们，因此测试可经 `tauri::test::mock_app()` 句柄在隔离的应用目录中持久化与列出消息。

`src/lib/aiGateway.ts` 导出事件字面量、`GatewayKeyAuthFailedEvent` 负载类型、`AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS = 1500` 与 `providerMarkedKeys`（保持池顺序，仅 `auto_marked === true`）。`AiGateway/index.tsx` 仅在 Tauri 环境订阅并按服务商聚合：首个事件开启窗口，窗口内同服务商事件只递增计数、不推送，窗口到期恰好推送一条点名服务商与计数的 warning toast，之后的事件开启新窗口并推送新 toast，卸载时清理计时器；toast 不携带 key 值。`UpstreamProviderList.tsx` 为每个服务商渲染 marked-key 区块（key 名称、派生状态标签、标记时间、脱敏原因与调用既有单 key 重新启用命令的 Re-enable 按钮），同时保留 key 计数汇总；`ProviderDetailDialog.tsx` 在既有状态徽标与标记时间旁显示脱敏原因。新增双语键为 `aiGatewayProviderMarkedKeys`（en "Marked keys" / zh "已标记密钥"）、`aiGatewayProviderKeyReasonLabel`（en "Reason" / zh "原因"）、`aiGatewayKeyAuthFailedToastTitle`（en "Key authentication failed" / zh "密钥鉴权失败"）与 `aiGatewayKeyAuthFailedToastDescription`（en `{{provider}} has {{count}} key(s) failing authentication; re-enable them in the AI Gateway.` / zh `{{provider}} 有 {{count}} 个密钥鉴权失败，请在 AI Gateway 中手动重新启用。`）。

验证：Rust 行为套件覆盖 401/403 分类矩阵与轮换、映射域的无凭据 403、按服务商诊断与提示条件、记录的终止消息、配置更新与通知发出、出现次数为 2 的按服务商消息条目与字面量固定；聚焦 Vitest 套件覆盖 marked-key 卡片、重新启用调用、详情原因与按服务商的 toast 聚合、过期与脱敏。`cargo test --manifest-path src-tauri/Cargo.toml` 以 0 退出且 1075 个 lib 测试通过、2 个既有 ignored，`npm test` 1235 个测试通过，`npx tsc -b` 与 `npm run lint` 以 0 退出。两个截断正文的 403 套件改为映射域，因为其 fixture 交付的正文与 content-length 不匹配，无可读凭据文本时规范要求不标记 key，而 401 截断用例保持 key 域。

## Alternatives considered

- 像此前一样对任意 401/403 标记所尝试的 key：未采纳，因为 2026-09-28 的事件表明套餐范围与服务策略类 403（`MODEL_NOT_IN_PLAN`、`error code: 1010`）会耗尽健康密钥池并产生 101 条已记录的 502；标记必须继续以显式凭据信号为门槛。
- 只把 `invalid api key` 这一精确短语当作凭据信号：未采纳，因为各上游对凭据拒绝的措辞不同；保守的信号列表覆盖已观察到的措辞，而缺失、空或非凭据文本仍然绝不标记。
- 无凭据 403 时保持所解析映射行启用、交给调用方重试：未采纳，因为该模型在上游策略拒绝期间确实无法服务，所以所解析行在操作者重新启用前离开服务。这是已接受的取舍：真正 key 域但无凭据文本的 403 不再标记 key，因此反复失败会让兄弟映射行逐个禁用；这些行仍可用既有命令重新启用，而正文报告凭据问题的服务商仍会标记 key。
- 为每次 key 转换各发一条消息中心条目，或每次转换都追加新条目：未采纳，因为拥有多个被吊销 key 的服务商会淹没消息中心；按服务商的去重键把转换聚合为一条带出现次数的条目。按服务商聚合在消息中只保留最新的 key、原因与计数，因此服务商卡片列出每个被标记的 key 以保留单个 key 的历史。
- 每个事件推送一条 toast：未采纳，因为同一请求内多个 key 失败会堆叠 toast；1500 毫秒的按服务商窗口恰好推送一条带计数的 toast，窗口之后的新标记再开新窗口。
- 对每次 key 变更（包括无变化变更）都发配置更新事件：未采纳，因为没有任何变化的变更绝不能重写文件或发出刷新信号；只有真实的持久化状态变化才发事件。
- 在负载、消息或 toast 中包含 key 值或完整上游正文：未采纳，因为契约禁止任何事件、消息、日志行或 toast 携带 key 值；负载只点名服务商与 key 标签并携带脱敏原因。

## Consequences

- 无凭据文本的 403 不触碰密钥池：不标记、不轮换、不禁用、不移除任何 key，只有所解析的映射行立即以 `HTTP 403` 原因自动禁用，服务商的兄弟模型继续用同一批 key 服务。凭据 401/403 保持 key 域轮换，额度归类的 429 标记不变。
- 无可用 key 的 502 与终止日志行现在按固定顺序列出每个被排除服务商的非零 key 计数，并且只在排除了鉴权失败 key 时追加手动重新启用提示；没有启用 key 的服务商写 `no enabled key`，无候选与无已完成尝试路径记录其消息而不是空错误。
- key 运行时状态变化即时可见：标记、TTL 清除或 key 探测重设只要真实改变持久化状态就发出配置更新事件，无变化保持静默，已打开的 AI 网关页面无需手动刷新即可更新。
- 鉴权转换在去重窗口内按服务商恰好通知一次——一条消息中心条目与一条聚合 toast——而额度标记保持静默；任何事件、消息、日志行或 toast 都不包含 key 值。
- 服务商卡片在保留计数汇总的同时列出每个运行时标记 key 的名称、派生状态、标记时间、脱敏原因与 Re-enable 按钮，详情编辑器在徽标与标记时间旁显示原因。
- 没有持久化字段、schema 版本或迁移变化：变更前已被标记鉴权失败的 key 保持标记，并变得可见且可重新启用；回滚只还原路由、诊断、发出与界面，存储配置与消息历史保持有效。
- 取代评估：部分取代。[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md)、[Gateway Key Health Rotates Bare 429s, Recovers Quota Marks after a TTL, Retries a Single Candidate and Pins the Query Source Key](../architecture/2026-09-25-gateway-key-health-lifecycle.md)、[Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](../architecture/2026-09-23-gateway-auto-disable-recovery.md) 与 [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md) 被保留并交叉链接。本记录只取代它们「任意 401/403 都标记所尝试的 key」（无凭据文本的 403 现属映射域）与「key 运行时标记绝不发出配置更新事件」的陈述；其有序选择、持久标记、额度 TTL 恢复、探测规则、重试调度、源 key 梯子、翻转广播与弹窗合并全部继续有效。
- `MEMORY.md`、`docs/USAGE.md` 与 `ai-gateway` / `ai-gateway-backend` 导航条目在同一变更中承载凭据限定的 403 规则、诊断、通知契约与卡片操作，`navigation.md` 已按权威 JSON 重新生成。
