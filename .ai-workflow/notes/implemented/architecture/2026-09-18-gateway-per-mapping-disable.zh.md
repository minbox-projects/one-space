# Agent Note: Gateway Per-Model Mapping Disable Is an Explicit Exclusion

Status: implemented

[English](2026-09-18-gateway-per-mapping-disable.md) | 中文

## Problem

上游服务商此前只能整体启用或禁用，因此想让网关停止服务某个本地模型的操作者只能删除该映射行或禁用整个服务商。删除映射行会丢弃远端模型名、行协议与显示名，而失去映射的本地模型随后会回退到服务商的 `default_model`，于是被静默改道而不是被拒绝；禁用整个服务商则会让该服务商的其他所有模型下线，重新启用时也无从得知原有的映射状态。两者都无法表达「该服务商明确不提供这一个模型，同时继续提供其余模型」，而从删除映射行推断出的状态也无法在服务商禁用/重新启用周期中保留。

## Decision

`ModelMapping` 新增持久化布尔字段 `enabled`，使用 `#[serde(default = "default_true")]`：字段缺省（不出现）即视为启用，该值始终序列化，因此用户的逐行意图能经受配置往返，旧 `api_fusion.json` 无需迁移。`selection::resolve_model_for_protocol` 在匹配时跳过禁用行，因此禁用行绝不参与服务、绝不产生 `ProtocolMismatch`，其 `upstream_model` 也绝不被转发。当请求的本地模型只命中某服务商的禁用行时，解析在 `default_model` 回退之前返回 `NoMatch`，因此该服务商不是候选、也绝不被联系；按模型禁用是显式排除，而非「未映射」。被启用行命中但协议不同的本地模型沿用既有 `ProtocolMismatch` 不回退规则，而完全未命中任何映射的本地模型在服务商协议一致时沿用既有 `default_model` 回退。`storage::normalize_config` 仍保留非空的禁用行，因此被禁用的映射能在保存与重新加载后存续，而不会被清除。

同一条启用谓词被应用到映射暴露的每一处：`GET /v1/models` 所用的 `runtime_http::local_model_names`、前端的 `aggregateModels` / `AggregatedModelsDialog`，以及终端同步所用的 `build_gateway_provider`。服务商仅在 `enabled` 时贡献，而映射行仅在其 `enabled && !auto_disabled` 时贡献：服务商级 `auto_disabled` 字段为旧版兼容字段、绝不参与过滤，行自身的自动禁用由 [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md) 描述的映射运行时状态结算。对 `opencode`，只有启用且未被自动禁用的映射被写入 `tool_config.models`；对 `codex`，`model` 取首个非空且启用、未被自动禁用的映射 `local_model`，仅当不存在这类映射时才回退首个非空 `default_model`。

映射级 `enabled` 与服务商级 `enabled` 以及行的运行时状态相互独立。`selection::set_user_enabled` 只作用于服务商，`selection::manual_reenable` 只清理运行时状态——服务商级旧字段与所有自动禁用行的运行时状态——因此禁用或重新启用服务商、或清除自动禁用状态，绝不改变任何映射的 `enabled` 意图；被用户禁用的映射只能由对该映射的显式操作恢复。前端方面，`ProviderDetailDialog` 为每个映射行提供反映并提交存储值的启用开关，以 `data-disabled="true"` 与弱化样式标记被用户禁用的行、以 `data-auto-disabled="true"` 标记被自动禁用的行，`resolveMappingPreview` 在请求的本地模型只命中禁用或自动禁用映射时返回 `null`。

## Alternatives considered

- 删除映射行而不新增启用标志：未采纳，因为删除会丢弃远端模型、协议与显示名，而失去映射的本地模型随后回退到 `default_model`，于是被改道而不是被拒绝。
- 禁用整个服务商：未采纳，因为那会让该服务商的其他所有模型下线，且无法表达单个不可用模型。
- 把禁用行当作未映射并允许其回退 `default_model`：未采纳，因为显式排除的意义正是在该服务商上拒绝该本地模型，回退会把它路由到默认远端模型并掩盖用户意图。
- 由上游健康自动推导可用性并自动禁用单个映射：当时未采纳，因为映射的 `enabled` 是用户意图，而自动可用性只由服务商级 `auto_disabled` 表达。[Gateway Template Model Retirement](2026-09-20-gateway-template-model-retirement.md) 之后把其中一种按映射自动禁用纳入模板同步契约——同步会禁用上一版模板已移除模型的派生映射（只写 `false`）——而 [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md) 让上游健康成为另一种按行自动禁用，因此该备选不再成立；模板同步仍绝不启用任何映射。
- 让服务商禁用/重新启用重置或恢复映射状态：未采纳，因为用户的逐行意图必须在服务商开关中存续，混淆两个层级会让服务商重新启用静默改变路由。
- 把逐映射状态存储在映射行之外：未采纳，因为那需要一个并行存储和第二个事实来源；在既有行上增加可选字段保持单一事实来源。

## Consequences

- 禁用映射被排除出候选选择、`GET /v1/models`、聚合模型视图与终端同步模型清单；其 `upstream_model` 不再可能被转发，只命中某服务商禁用映射的请求返回无候选（`all_providers_unavailable`），而不是回退该服务商的 `default_model`。
- 完全未命中任何映射的本地模型在服务商协议一致时仍使用服务商 `default_model`，因此既有回退被收窄到真正不命中任何行的请求。
- 旧 `api_fusion.json` 以每个映射都启用完成反序列化且不做迁移；新构建始终序列化 `enabled`，旧构建忽略未知字段并把每一行都视为启用，因此任一顺序的发布都安全。
- 服务商禁用/重新启用与手动清除 `auto_disabled` 保留每个映射的状态，因此被用户禁用的行保持禁用，直到用户切换该行；服务商与映射两个层级保持独立。
- `MEMORY.md` 与 `navigation.json` 已在同一变更中按映射级 `enabled` 语义与逐行开关更新，`navigation.md` 已由权威 JSON 重新生成。
- Supersession：部分取代（partial supersession）。[API Gateway Aggregated Models Open in a Dialog With a Shared Count](../feature/2026-09-17-api-fusion-aggregated-models-dialog.md) 与 [API Fusion Terminal Sync Writes an Independent Gateway Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md) 被保留并交叉链接，因为本次变更仅以新的启用过滤收窄了它们记录的映射选择谓词，而它们的决策（共享聚合弹框与独立终端同步服务商）保持不变。[Gateway Template Model Retirement](2026-09-20-gateway-template-model-retirement.md) 也部分取代本记录：其「按映射自动禁用不在范围内」的被否决备选对模板同步已不再成立——同步会禁用上一版模板已移除模型的派生映射并只写 `false`——而显式排除规则与用户 `enabled` 意图在服务商/映射之间的独立性继续有效。[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md) 也部分取代本记录：由上游健康自动禁用单个映射的被否决备选如今成为决策，并作为独立于用户 `enabled` 的行运行时状态结算；现行标准由本 Note、新 Note 与 `MEMORY.md` 承载。[API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md)、[Gateway Single-Candidate Fast Fail and Standard Error Responses](2026-09-18-gateway-fast-fail-and-standard-errors.md)、[New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md)、[API Fusion Local Key Creation Uses a Name Dialog](../feature/2026-09-17-api-fusion-local-key-name-dialog.md) 与 [Bilingual Note Triplets](../process/2026-09-17-bilingual-note-triplets.md) 互不相关，因此都未被取代。
