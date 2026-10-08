# Agent Note: Automatic Template Sync Notifies the Message Center on Real Mapping Changes

Status: implemented

[English](2026-09-24-ai-gateway-template-auto-sync-notification.md) | 中文

## Problem

[模板自动刷新](2026-09-23-template-auto-refresh.md)按持久化间隔同步每个有 URL 的服务商模板，并把结果传播到绑定它的服务商，但整个过程是静默的。除非打开模板并查看映射清单，操作者无法得知上游清单何时为某个绑定服务商真正新增或退役了模型，因此需要一条通知——但要带一个刻意收窄的触发条件：它只能因真实的映射新增而触发，绝不为仅字段差异、为手动「同步模型列表」操作或为失败的刷新触发。它也不能成为第二份同步实现，且不能扩展后端命令面或消息 schema。

## Decision

`src/components/AiGateway/useTemplateAutoRefresh.ts` 中的调度器在逐模板循环之前读取一次 `aiGatewayGetConfig()` 作为滚动基线，在每次成功的 `aiGatewaySyncProviderTemplate` 之后重新读取，并按 `upstream_model` 对 `template_id` 匹配的服务商做差量比较。`computeTemplateSyncChange(previous, current, templateId)` 只把 `upstream_model` 在上一版配置中不存在的映射视为新增；模板同步删除的映射不是新增，因此仅发生退役的同步不合格，改由关系后果中所述的后端按服务商范围 warning 记录。仅字段差异（显示名、生效协议、服务商名、`base_url`）、同步前就已禁用的映射、被模板跳过或列入 `ignored_models` 的模型、未绑定到该模板的服务商、以及没有任何绑定服务商的模板都不合格，因此没有任何合格变化的模板不产生任何内容。

合格的模板恰好产生一条由 `buildTemplateSyncMessage(view, change)` 构建的消息载荷：`{ source: "ai_gateway", category: "template_sync", severity: "info", target: { tab: "ai-gateway" } }`，且不带 `dedupe_key`。标题通过 `aiGatewayTemplateSyncNotificationTitle` 给出模板名；摘要为受影响服务商数与新增数（`aiGatewayTemplateSyncNotificationProviderCount`、`...AddedCount`，每个为零的计数分句省略）中非空部分以 `"; "` 连接；明细为逐服务商一行、以换行连接（`aiGatewayTemplateSyncNotificationDetailProvider`），逐条列出新增映射的 `local_model`，为空时回退到 `upstream_model`。

`src/i18n.ts` 为两种语言提供剩余五个键（`aiGatewayTemplateSyncNotificationTitle`、`...ProviderCount`、`...AddedCount`、`...DetailProvider` 与 `messageSource_ai_gateway`）；两个语言包中不再使用的 `aiGatewayTemplateSyncNotificationDisabledCount` 键已删除，`src/components/MessageCenter.tsx` 新增经 `messageSource_ai_gateway` 解析的 `ai_gateway` 来源标签。

失败隔离是显式的。批次之前读取配置失败会抑制该批次的全部通知，而每个可同步模板仍会被同步；单次同步之后读取配置失败只抑制该模板的消息，并让滚动基线对下一个模板保持不变；创建消息失败只抑制该模板的消息。两种失败都不会阻塞或回滚兄弟同步。手动「同步模型列表」路径不创建 `template_sync` info 消息；自 [Gateway Alert Pills Dismiss per Instance and Archive Provider-Scoped Warnings](2026-10-07-gateway-alert-badges-and-message-center.md) 起，它通过同一个共享的 `ai_gateway_sync_provider_template` 命令记录后端按服务商范围的 `template_mappings_retired` warning，因此手动与自动路径对一次退役的记录完全一致。

## Alternatives considered

- 每次自动同步成功都通知：未采纳，因为未变化的清单会在每个 tick 都产生消息，操作者无法把真实清单变化与例行漂移区分开。
- 新增按模板的 `dedupe_key`：未采纳，因为重复的真实变化是各自值得独立记录的事件；去重键会把后来的新增静默合并进更早的消息里，掩盖第二次变化。
- 在 Rust 后端生成 `template_sync` info 通知：未采纳，因为批次循环、手动同步进行中注册表与失败存储都已经在前端调度器中，后端 info 通知只会多出一条执行路径和一个新的发出点，却没有行为收益；退役记录是例外，现在在后端命令中执行，以便在页面未打开时仍保留。
- 既为手动「同步模型列表」操作发出 `template_sync` info 通知：未采纳，因为该操作由操作者主动发起且已同步报告其结果，info 消息只会重复操作者刚刚请求的反馈；当一次同步确实退役了映射时，手动路径仍会记录后端退役 warning。
- 用比较整个映射对象而不是 `upstream_model` 存在性来检测变化：未采纳，因为那样仅字段传播（显示名、生效协议）会被误报为模型变化。

## Consequences

- 每个变化模板每次同步恰好一条聚合消息：受影响服务商数聚合该模板所有合格的绑定服务商，一个批次对每个变化模板各创建一条消息。
- 重复新增产生独立消息：不设置 `dedupe_key`，因此连续两个合格周期会创建两条独立消息；这是有意且已接受的取舍。
- 手动同步与自动失败不产生 `template_sync` info 消息：手动路径不调用 info 通知辅助函数，失败的同步只记录其内联失败原因而不创建消息；后端退役 warning 仅在配置写入成功后记录。
- 消息文案跟随当前语言：保留下来的键在两种语言包中都存在，标题、摘要与明细在创建时解析。
- 无后端命令、持久化格式或消息 schema 变更：info 通知复用既有 `messages_create` 命令，且只读取既有加密网关配置。
- 验证：交付的行为测试为 `src/components/AiGateway/useTemplateAutoRefresh.test.ts`（新增与非合格差量（含仅退役不产生消息）、聚合、信封、无去重重复、失败隔离与双语渲染）、`src/components/AiGateway/AiGateway.test.tsx`（手动路径不创建 `template_sync` 消息）与 `src/i18n.test.ts`（保留下来的键在两种语言包中都存在、已删除的禁用计数字键不存在）。
- 关系：本记录以通知决策扩展 [Provider Templates Refresh Automatically on a Persisted Interval](2026-09-23-template-auto-refresh.md)，其间隔、调度器与失败展示决策继续有效并被原样复用。部分取代：[Gateway Alert Pills Dismiss per Instance and Archive Provider-Scoped Warnings](2026-10-07-gateway-alert-badges-and-message-center.md) 以按服务商范围的后端 `template_mappings_retired` warning 取代本记录的禁用通知条款，并把其手动路径与「Rust 后端生成通知被拒绝」的陈述限定为 `template_sync` info 类别，而本记录的新增决策继续有效。部分取代：[A Provider Template Sync Deletes Retired Mappings](../architecture/2026-10-08-provider-template-retired-mapping-removal.md) 以删除语义取代本记录的「禁用并保留」退役措辞，而本记录的仅新增与失败隔离决策继续有效。不改动其他既有记录：它不取代 [Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](2026-09-23-template-terminal-resync.md)，因为通知以只读方式观察同步结果，不改变共享命令的 best-effort 终端刷新。
- `MEMORY.md` 在同一变更的 `模板自动刷新` 条目中记录合格的 `template_sync` info 消息与手动同步或失败不产生 info 消息的行为；本记录无需改动导航 JSON 及其生成的 Markdown，因为其辅助函数是模块私有的，且没有任何公共符号、路径或归属发生变化。
