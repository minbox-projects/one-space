# Agent Note: Gateway Alert Pills Dismiss per Instance and Archive Provider-Scoped Warnings

Status: implemented

[English](2026-10-07-gateway-alert-badges-and-message-center.md) | 中文

## Problem

AI 网关服务商卡片上的两个警告药丸——「N 个映射已从模板移除」与「N 个映射已被自动禁用」——此前是永久只读提示。只要底层运行状态存在，它们就一直可见，操作者无法确认一个已知问题，于是卡片上会堆积过期的琥珀色小标签。与此同时，真正进入 `auto_disabled` 的映射行，或被模板同步退役的派生映射，只在 AI 网关页面打开时可见：关闭页面事件即丢失，从不打开页面的操作者根本不会得知。模板自动刷新的 info 消息也补不上这个缺口——它把已退役映射和新增一起计数，于是退役会在操作者真正需要的 warning 旁边多产生一条 `info` 记录，甚至仅发生退役的同步仍会产生一条 info 消息。因此本变更必须让每个药丸成为瞬态、可按问题实例关闭的提示，跨重启记住关闭状态，同时让已解决后又复发的问题再次出现，并让后端在不改变网关配置 schema、消息 schema 或命令面的前提下，为每个服务商与类别恰好记录一条 warning。

## Decision

前端在新的模块 `src/lib/aiGatewayAlertBadges.ts` 中拥有关闭状态，以 JSON 字符串数组持久化在 `AI_GATEWAY_ALERT_BADGES_STORAGE_KEY = "ai-gateway-alert-badge-dismissals"` 下。每个问题实例由一个不透明键标识：`autoDisabledAlertInstanceKey(providerId, localModel, upstreamModel)` 返回 `auto:<provider>:<localModel.trim() || upstreamModel>:<upstream>`，`retiredMappingAlertInstanceKey(providerId, templateId, upstreamModel)` 返回 `retired:<provider>:<template>:<upstream>`。`readDismissedAlertInstanceKeys` 解析并校验存储的数组，`dismissAlertInstanceKeys` 合并键并持久化，`reconcileDismissedAlertInstanceKeys` 只保留仍是当前问题的键，且仅在确有裁剪时才重写存储集合。每次读取、解析与写入都受包裹，因此缺失、损坏或不可用的存储降级为仅会话内存，绝不阻塞渲染。

`UpstreamProviderList` 从已加载的服务商与模板视图派生 `currentInstanceKeys`——自动禁用行，以及 `enabled === false` 且 `isMappingDeprecated(mapping, template)` 成立的映射——用 `reconcileDismissedAlertInstanceKeys` 初始化 `dismissedKeys`，并在该集合变化时重新协调。因此不再属于问题的实例会被遗忘、之后复发时再次出现，而仍处于禁用行的探测重设保留同一实例键，从而保留关闭状态。两个药丸都渲染一个可键盘聚焦的关闭控件（`type="button"`、本地化 `aiGatewayAlertBadgeDismissAria`、`stopPropagation`，因此卡片既不会被选中也不会被切换）。每个仍待处理的药丸在出现 `AI_GATEWAY_ALERT_AUTO_DISMISS_MS = 8000` 毫秒后，经一个在卸载时清除的 `setTimeout` 自动关闭；该 effect 以待处理实例键的连接串为键，因此无关重渲染不会重启倒计时，而新出现的待处理实例会重启。药丸计数与 tooltip（自动禁用药丸为 `aiGatewayProviderAutoDisabledModelsTooltip`，退役药丸为既有的 `aiGatewayTemplateRetiredMappingsTooltip`）只列出未关闭的实例。

在配置写入成功后，当映射行真正转入 `auto_disabled` 时，后端恰好记录一条按服务商范围的 warning。`src-tauri/src/ai_gateway/runtime_http.rs` 的两条结算路径共用该构建器：`apply_failure` 覆盖立即的非凭据 403 禁用，请求结束的阈值结算为第二条路径。只有 `false`/缺失到 `true` 的翻转才被判定为转换；当全部变更结算完毕后，代码经 `auto_disabled_models` 读取受影响服务商当前完整的自动禁用集合（trim 后的 `local_model`，为空回退 `upstream_model`），并发出 `mapping_auto_disabled_message_input`：source `ai_gateway`、category `mapping_auto_disabled`、severity `warning`、按服务商的去重键 `ai_gateway_mapping_auto_disabled:<provider_id>`、目标页签 `ai-gateway` 且服务商为 `entity_id`、经 `messages::localized` 的双语标题与摘要，并以连接后的集合作为 `detail`。`emit_mapping_auto_disabled` 针对已捕获的应用句柄，经 `messages::record_message_silent` 恰好持久化一条输入；已禁用行的探测重设、无变化结算与写入失败都不发出任何内容。仅测试接缝 `MAPPING_AUTO_DISABLED_MESSAGE_INPUTS` 记录字面输入。

一次成功的模板同步会为本次同步新退役了至少一条派生映射（同步前启用、因上游模型离开模板而被禁用）的每个绑定服务商返回一条 `ProviderRetirementNotice`，来源为 `src-tauri/src/ai_gateway/templates.rs` 的 `propagate_to_derived`，经 `apply_template_sync_with_notices` 与 `apply_template_sync_from_body` 传出；`apply_template_sync_with` 保留旧签名作为丢弃通知的包装。`commands.rs` 中的生产命令 `ai_gateway_sync_provider_template` 在 `modify_config` 内暂存同步，并仅在配置写入成功后记录 `record_template_mappings_retired_messages`，用 `template_mappings_retired_message_input` 构建每条记录：source `ai_gateway`、category `template_mappings_retired`、severity `warning`、按服务商的去重键 `ai_gateway_template_mappings_retired:<provider_id>`、服务商为目标的 `entity_id`，以及经 `localized` 的双语标题/摘要，点名服务商并列出其事件时当前完整退役集合——即所有 `enabled == false` 且 `upstream_model` 不在当前模板中的映射，按配置行顺序。失败的同步、没有绑定服务商的模板以及没有新退役映射的服务商都不记录任何内容；手动与自动路径都运行该命令，因此记录完全一致。仅测试接缝 `TEMPLATE_MAPPINGS_RETIRED_MESSAGE_INPUTS` 记录这些输入。

模板自动刷新的 info 消息现在只报告新增。`src/components/AiGateway/useTemplateAutoRefresh.ts` 的 `computeTemplateSyncChange` 只将真实映射新增（上一版配置中不存在的 `upstream_model`）计为合格；仅发生退役的同步不合格。info 摘要与明细不再携带禁用计数或禁用明细行，两个 i18n 语言包中不再使用的 `aiGatewayTemplateSyncNotificationDisabledCount` 键已删除。info 的严重级别、模板标题、抓取时语言解析与失败隔离规则均未改变。

## Alternatives considered

- 仅在会话内记住关闭状态：未采纳，因为对操作者已经确认过的状况，药丸会在每次重启后重新出现，本功能便无法减少反复出现的噪音。
- 一旦关闭就永久压制某服务商或类别：未采纳，因为已解决后又复发的问题是需要操作者看到的新事件；只有带协调的按实例关闭才能保留这一区分。
- 为每条退役或自动禁用映射各记录一条消息，而非每个服务商一条：未采纳，因为一次模板退役或一次失败突发通常同时涉及同一服务商的若干行，按映射记录会让一个事件产生多条记录；按服务商聚合并携带当前完整集合是选定的粒度。
- 在前端构建退役通知：未采纳，因为退役必须在 AI 网关页面未打开时仍然留下记录，而只有后端看得到执行同步的命令；退役记录由后端拥有。
- 对本次变更之前就已存在的问题做追溯回填：未采纳，这是明确的非目标；它会改写消息中心从未观察到的历史，对新转换也没有必要。

## Consequences

- 已关闭或已自动关闭的药丸会跨重载保持隐藏，直到其实例解决并再次复发：关闭状态在网关配置与模板视图加载时协调，因此完全在两次加载之间解决又复发的问题会在卡片上保留其关闭状态，而后端仍记录新消息；在加载时观察到健康的问题会被裁剪，并在再次出现时重新显示。
- 新增两个按服务商范围的 warning 类别：`mapping_auto_disabled` 与 `template_mappings_retired`，各自仅在配置写入成功后记录。消息存储的一小时去重窗口会把同一服务商的重复转换按 `dedupe_key` 合并，以 `occurrences` 计数，并把摘要/明细刷新为最新完整集合。不做回填，也不产生解决或关闭消息。
- 自动刷新 info 消息只报告新增，因此退役由后端 warning 记录一次，而不再作为 info 记录重复出现；仅发生退役的同步不创建 `template_sync` 记录。
- 无接口、schema 或迁移变更：前端只在本地存储中新增自己的 JSON 关闭集合，旧构建会忽略它；后端复用 `messages::MessageCreateInput`、`record_message_silent`、去重窗口与 `localized`，与既有 key-auth 通知完全一致；没有 Tauri 命令签名、`ai_gateway.json` schema、用量日志 schema 或消息中心 schema 变化。
- 验证：`src-tauri/src/ai_gateway/tests.rs` 中的七个 `ac005_*` 测试覆盖自动禁用转换（每个服务商一条记录、两条结算路径、出现次数合并，以及探测重设、无变化与写入失败的反例）；`src-tauri/src/ai_gateway/tests/templates.rs` 中的六个 `ac006_*` 测试覆盖退役转换；`src/lib/aiGatewayAlertBadges.test.ts`、`src/components/AiGateway/UpstreamProviderList.test.tsx` 的 Step 3 describe 与追加的 `src/i18n.test.ts` 断言覆盖关闭持久化、7999/8000 毫秒边界、部分关闭、复发与损坏存储安全；`src/components/AiGateway/useTemplateAutoRefresh.test.ts` 与 `src/i18n.test.ts` 覆盖仅新增消息与已删除键。本分支上后端 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway` 套件通过（611 后 617），Step 3 三件套通过 130 个测试、Step 4 三件套 165 个，`npm run lint` 退出码 0。
- 关系：对 [Automatic Template Sync Notifies the Message Center on Real Mapping Changes](2026-09-24-ai-gateway-template-auto-sync-notification.md) 的部分取代，该记录保留并互相链接：其禁用通知条款被后端按服务商范围的退役 warning 取代，其「手动『同步模型列表』路径不创建消息」以及「在 Rust 后端生成通知已被拒绝」的陈述被限定为 `template_sync` info 类别，因为退役记录现在在共享命令中为两条路径执行；其新增记录继续有效。[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md) 与 [Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](../architecture/2026-09-23-gateway-auto-disable-recovery.md) 保留并互相链接，因为本变更只记录它们已经持久化的转换，不改动状态机、阈值、探测或恢复命令。[Provider Templates Refresh Automatically on a Persisted Interval](2026-09-23-template-auto-refresh.md) 保留并互相链接，因为其调度器、间隔与失败展示决策继续有效；[Gateway Template Model Retirement Disables Derived Mappings](../architecture/2026-09-20-gateway-template-model-retirement.md) 保留并互相链接，因为本变更只观察它所定义的映射退役。
- `MEMORY.md` 在同一变更中记录按实例关闭、8000 毫秒自动关闭、两个按服务商范围的类别与仅新增的刷新消息，`ai-gateway` 与 `ai-gateway-backend` 导航条目携带新的前端 store 与常量以及新的后端符号，且 `navigation.md` 由权威 JSON 重新生成。
