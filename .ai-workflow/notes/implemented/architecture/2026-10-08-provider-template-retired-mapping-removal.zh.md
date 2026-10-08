# Agent Note: A Provider Template Sync Deletes Retired Mappings

Status: implemented

[English](2026-10-08-provider-template-retired-mapping-removal.md) | 中文

## Problem

模板同步会按 `models_url` 整体替换模板的模型清单，但端点已下架的模型此前只是把派生映射禁用并保留。绑定服务商因此堆积既不再服务、也不再匹配其模板的禁用 deprecated 行，操作者必须逐条找到并手工删除，而服务商的 `ignored_models` 未记录任何内容，被移除的模型只因其禁用行从未被重新启用才保持缺席。退役行为必须彻底移除这些过期行，同时避免后续同步悄悄把已移除的模型重新加回。

## Decision

模板同步现在会删除每个绑定到该 `template_id` 的服务商中，上一版模板已携带而抓取列表已不存在的模型对应映射（无论该映射的启用标志），以及 `upstream_model` 不在抓取列表中的既有禁用映射。`propagate_to_derived`（`src-tauri/src/ai_gateway/templates.rs`）按映射行顺序去重收集被移除的 `upstream_model` 并保留其余映射；新列表仍携带的模型对应映射即使被用户禁用也保持原样，同步从未退役的启用手动附加映射保持原样，手动服务商或绑定其他模板的服务商不被改动。

每个被移除的模型至多一次追加到绑定服务商的 `ignored_models`（`src-tauri/src/ai_gateway/types_config.rs` 的 `GatewayUpstreamProvider`，其文档注释现在说明手动删除模型与模板同步都会写入它），因此后续列表再次包含该模型的同步不会为它新增映射；`ai_gateway_restore_provider_model` 仍是模板仍列出该模型时的唯一回归路径，并在模板不再列出时保持其现有可操作错误。

同步会删除受影响服务商中 `upstream_model` 属于被移除模型的 `model_prices` 行，而其他服务商对同一模型的行与其他模型的行保持不变。当服务商 `default_model` 去空白后等于某个被移除的模型名时，同步清除它；指向保留模型的 `default_model` 保持不变。

配置写入成功后，同步会为每个至少删除了一条映射的绑定服务商恰好记录一条 `template_mappings_retired` warning：source `ai_gateway`、category `template_mappings_retired`、severity `warning`、按服务商的去重键 `ai_gateway_template_mappings_retired:<provider_id>`、目标页签 `ai-gateway` 且服务商为 `entity_id`，双语摘要与 `detail` 字段都按映射行顺序去重列出被移除的模型（`commands.rs` 的 `template_mappings_retired_message_input` 把 `detail` 设为与摘要相同的逗号连接列表）。本就禁用的遗留行也会触发同一条 notice。没有删除的同步、没有删除的服务商与写入失败都不记录任何内容。

所有删除、忽略集合、价格行与 `default_model` 变更都在同一份既有配置克隆上暂存，并由唯一一次既有原子 `write_config` 提交；写入失败则每个服务商、价格行与消息都保持不变。未改变的传播规则继续成立：服务商 `name`、`base_url`、`protocol` 与映射 `display_name`、协议仅在仍等于上一版模板值时更新，`local_model` 绝不被修改，忽略与模板禁用的模型绝不传播，既有映射绝不重新启用。行被删除后，既有退役映射药丸（`isMappingDeprecated` 与按实例关闭存储）对已清理的服务商不再渲染任何内容，因为它从当前状态派生且找不到禁用且缺席的行。忽略模型区块在两种语言下及组件回退文案现在都说明模型是由用户或由模板同步从服务商移除的；翻译键名保持不变。

## Alternatives considered

- 保留此前退役记录所记载的「禁用并保留」行为：未采纳，因为禁用的 deprecated 行不再服务、在卡片上始终可见且仍需手工删除；删除该行并把模型记入 `ignored_models` 能一次清除过期状态并阻止回归的模型被重新加回。
- 保留被移除行的 provider-scoped 价格数据与指向它的 `default_model`：未采纳，因为价格行与默认路由会超出它们所描述映射的生命周期，并可能为一个服务商已不再映射的模型计价或默认路由。
- 删除新列表中不存在的所有映射，包括禁用的手动附加映射与其他模板的映射：未采纳，因为上一版模板从未携带的映射、手动服务商与绑定其他模板的服务商都在模板契约之外；只移除「上一版模板减去抓取列表」的差集与已禁用的缺席行。
- 把移除记录为卡片提示的持久化同步事件：未采纳，因为提示只描述当前状态；行被删除后它找不到任何内容，存储事件只会为一个无操作者可见收益的目标新增持久化字段及其生命周期。
- 后续同步再次列出该模型时重新启用或复活被移除的模型：未采纳，因为忽略集合是移除的持久记录；操作者在模板仍列出该模型时经 `ai_gateway_restore_provider_model` 显式恢复它。

## Consequences

- 退役是有意破坏性的：映射行、其 provider-scoped 价格行与匹配的 `default_model` 都被移除，模型被写入 `ignored_models` 一次；配置 schema、命令签名与消息 schema 均不变，也不存在迁移或回填。
- 回滚会还原代码与行为，但无法恢复同步已删除的行；回退后，回退构建绝不重新启用映射，并对每个剩余行按变更前方式转发。
- 行为迁移：本就禁用的遗留 deprecated 行会被下一次同步（人工或自动）删除并在按服务商范围的 warning 中报告；早前构建在模板移除后仍保持启用的映射，其模型早已离开上一版模板，因此它不在退役集合内并保持当前状态，直到它被禁用（再由后续同步移除）或被操作者删除。
- 旧构建会把结果配置按普通行与忽略条目读取，退役映射药丸则直接找不到任何内容。
- 当前状态提示仍会统计任何具有「已禁用且缺席」形态的剩余映射，例如被用户禁用的手动附加映射，因为同步只删除其退役集合内的行；它不是事件日志，重新启用映射始终是操作者的显式动作。
- 本次交付重写了 `src-tauri/src/ai_gateway/tests/templates.rs` 中针对删除范围、忽略集合、价格行与 `default_model`、以及按服务商范围消息摘要与 `detail` 的行为测试，取代此前的「禁用并保留」断言。
- Supersession（取代评估）：完全取代 [Gateway Template Model Retirement Disables Derived Mappings](2026-09-20-gateway-template-model-retirement.md)，该记录随本声明保留，因为其「禁用并保留」决策与「删除退役映射」的被否决备选在此被反转，而其卡片头像、当前状态提示与单向映射规则作为它自身的历史继续存在；部分取代 [Gateway Alert Pills Dismiss per Instance and Archive Provider-Scoped Warnings](../feature/2026-10-07-gateway-alert-badges-and-message-center.md) 的退役消息条款，其药丸与关闭决策继续成立；部分取代 [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) 中「同步绝不写 `ignored_models` 或价格行」的条款与 [Automatic Template Sync Notifies the Message Center on Real Mapping Changes](../feature/2026-09-24-ai-gateway-template-auto-sync-notification.md) 中描述「仅禁用」退役的条款，二者的调度器、仅新增 info 消息与失败隔离决策继续成立；并就地修正 [API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 与 [Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 的退役模型与同步价格行事实，二者的其余决策继续成立。
- `MEMORY.md` 与 `navigation.json` 的 `ai-gateway` 和 `ai-gateway-backend` 条目在同一变更中描述删除规则与修正后的忽略模型文案，`navigation.md` 已按权威 JSON 重新生成。
