# Agent Note: Automatic Template Sync Notifies the Message Center on Real Mapping Changes

Status: implemented

[English](2026-09-24-ai-gateway-template-auto-sync-notification.md) | 中文

## Problem

[自动模板刷新](../feature/2026-09-23-template-auto-refresh.md) 按持久化间隔同步每个 URL 支持的服务商模板并把结果传播到绑定它的服务商，但过程是静默的。操作者若不打开模板并读取映射列表，就无法知道上游目录何时实际为某个绑定服务商新增或退役了模型，因此需要通知——并带有刻意收窄的触发条件。它必须只在真实映射新增时触发，绝不为仅字段差异或失败刷新触发。它也不能成为第二个同步实现，且不能扩展后端命令面或消息 schema。[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 后来把消息的产生移入后端；本记录为 additions-only 决策而保留。

## Decision

后端推导 additions-only 差异。`src-tauri/src/ai_gateway/templates.rs::template_sync_addition_notices(before, after, template_id)` 在同步前 `before` 与同步后 `after` 配置之间比较绑定到该模板的服务商，并为每个至少新增一条映射的服务商产出一条通知。只有同一服务商同步前不存在的 `upstream_model` 映射才算合格；未绑定该模板的服务商被忽略，输出遵循同步后配置的服务商与映射顺序。仅字段差异（显示名、有效协议、服务商名、`base_url`）、同步前已禁用的映射、模板跳过或列入 `ignored_models` 的模型，以及没有绑定服务商的模板都不合格，因此没有合格变化的模板不产生任何内容。该差异在已应用同步的串行化 `modify_config` 变更内运行，因此同一次原子写入既持久化结果又捕获新增。

在配置写入成立后，`src-tauri/src/ai_gateway/commands.rs::record_template_sync_addition_message` 经 `crate::messages::record_message_silent` 为每个有新增的模板恰好记录一条消息。负载为 `{ source: "ai_gateway", category: "template_sync", severity: "info", target: { tab: "ai-gateway" } }` 且无 `dedupe_key`；标题为事件时本地化文本（`Provider template <name> models changed` / `服务商模板 <name> 模型已变更`），摘要为以 `"; "` 连接的受影响服务商数与新增数，明细为按换行连接、逐服务商列出每个新增映射 `local_model` 的行，空时回退 `upstream_model`。

失败隔离是显式的。致命源问题（网络、非 JSON、非法结构、空有效模型集）保持配置不变并在任何消息之前返回，因此没有消息逸出。缺少 `AppHandle` 时跳过发出，而测试接缝仍捕获输入。仅退役的同步不产生 `template_sync` info 消息；它改为记录后端按服务商范围的 `template_mappings_retired` warning，因此仅移除映射的模板同步仍会被呈现。best-effort 终端刷新在消息与退役记录之后运行，绝不阻塞它们。

手动 `ai_gateway_sync_provider_template` 命令与自动批次现在都通过共享同模板 guard 运行同一个 `execute_template_sync`，因此新增映射的手动同步会记录同一条 info 消息。没有新增命令、持久化格式或消息 schema 字段；通知复用既有消息存储并只读取既有加密网关配置。

## Alternatives considered

- 在每次成功同步时通知：拒绝原因：未变化的目录会在每个 tick 产生一条消息，操作者无法区分真实目录变化与例行波动。
- 增加每模板 `dedupe_key`：拒绝原因：重复的真实变化是值得各自记录的独立事件；dedupe key 会把后来的新增静默并入更早的消息并隐藏第二次变化。
- 在 Rust 后端产生 `template_sync` info 通知：当时被拒绝，因为批次循环与失败存储位于前端调度器，后端通知只会增加第二条执行路径而没有行为收益。[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 后来采纳后端 owner 并在那里产生消息，因此该替代方案不再被拒绝。
- 对手动「同步模型列表」操作抑制 info 通知：当时被拒绝，因为该操作已同步报告其结果。后端接管所有权后，手动与自动路径都记录 additions-only 消息，因此手动路径现在也产生它。
- 通过比较整个映射对象而非 `upstream_model` 是否存在来检测变化：拒绝原因：仅字段传播（显示名、有效协议）会被误报为模型变化。

## Consequences

- 每个发生变化模板每次同步恰好一条聚合消息：受影响服务商数聚合该模板所有合格的绑定服务商，一次同步为每个发生变化模板创建一条消息，无论同步是手动还是自动。
- 重复新增产生独立消息：未设置 `dedupe_key`，因此两个连续合格周期创建两条独立消息；这是刻意且被接受的。
- 失败与退役：失败同步不产生 `template_sync` info 消息，仅退役的同步改为记录后端按服务商范围的 `template_mappings_retired` warning；该 warning 仅在成功配置写入后出现。
- 消息文本跟随当前语言：标题、摘要与明细在创建时经内联 `crate::messages::localized` 事件时字符串解析，而非经前端 i18n bundle。四个早先的前端 `aiGatewayTemplateSyncNotification*` 键仍保留在两个 bundle 中，但在 [Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 把消息产生移到后端后不再被生产使用。
- 没有后端命令、持久化格式或消息 schema 变化：info 通知复用既有消息存储并只读取既有加密网关配置。
- 验证：交付的行为测试为 `src-tauri/src/ai_gateway/tests/auto_refresh.rs` 与 `src-tauri/src/ai_gateway/tests/templates.rs` 的 additions-only 差异与消息记录路径、`src-tauri/src/ai_gateway/tests.rs` 的共享命令行为，以及 `src/components/AiGateway/AiGateway.test.tsx` / `src/i18n.test.ts` 的既有键与手动路径。
- 关系与取代：部分取代。[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 替换本记录的前端消息产生及其被拒绝的后端替代方案，而 additions-only、无 dedupe 与失败隔离决策继续有效；本记录保留并交叉链接，不归档。本记录继续扩展 [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md)，且 [A Provider Template Sync Deletes Retired Mappings](../architecture/2026-10-08-provider-template-retired-mapping-removal.md) 与 [Gateway Alert Pills Dismiss per Instance and Archive Provider-Scoped Warnings](2026-10-07-gateway-alert-badges-and-message-center.md) 的退役语义继续有效。
- `MEMORY.md` 在同一变更的 `模板自动刷新` bullet 中记录后端 additions-only 消息与仅退役区分；导航 JSON 及其生成的 Markdown 记录后端 owner，消息 helper 为模块私有，无需新增公共路径。
