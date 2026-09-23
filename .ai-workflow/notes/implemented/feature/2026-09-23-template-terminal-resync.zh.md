# Agent Note: Template Sync Best-Effort Refreshes Previously Synced Terminal Tools

Status: implemented

[English](2026-09-23-template-terminal-resync.md) | 中文

## Problem

服务商模板同步与终端同步可能相互漂移。`api_gateway_sync_provider_template` 是人工「同步模型列表」与[自动刷新调度器](2026-09-23-template-auto-refresh.md)共用的命令：它刷新模板模型清单并增量传播到每个派生服务商，但不写入任何工具侧配置。因此，已通过终端面板同步过的工具（`opencode` 与/或 `codex` 已持有带标记的 `API Gateway` 服务商记录）会保留基于上一版映射集合构建的网关记录，直到操作者手工重跑终端同步；工具侧模型清单可能落后于网关刚获得或刚失去的映射。弥合该缺口有两条约束：传播必须同时服务人工操作与无人值守的调度器，且不得复制终端同步规则；次级终端失败必须始终无法使已经落盘的模板同步失败。

## Decision

`api_gateway_sync_provider_template` 现在接收注入的 `app: tauri::AppHandle`（由 Tauri 提供；前端可见的参数字形仍只有 `templateId`），并委托给 `src-tauri/src/api_gateway/commands.rs` 中的 `apply_template_sync_with_terminal_refresh`。该包装先执行 `apply_template_sync_with` 并在失败时提前返回，因此模板同步未成功时绝不启动任何终端工作。只有在模板同步落盘之后，它才检查是否存在至少一个携带该 `template_id` 的上游服务商、收集 `previously_synced_terminal_tools`，并在列表非空时为这些工具恰好 await 一次注入的终端同步调用，且吞掉其结果。

`previously_synced_terminal_tools` 复用 `terminal_targets_from`，因此刷新套用与终端目标列表相同的「已同步」谓词：受支持工具（`opencode`、`codex`，按 `SUPPORTED_TERMINAL_TOOLS` 顺序）仅在仍持有带标记的受管网关记录时才算数，单凭台账绝不认领未标记的用户服务商。命令把注入的闭包接到既有 `apply_terminal_sync`，因此刷新就是携带完整当前映射集合的普通终端同步：同样的标记与台账规则、默认 Key 解析、`local_base_url`、opencode 激活与 `opencode.json` 投影、codex 手动激活，以及绝不改写用户记录、Key 与价格行的保证。从未同步过的工具与没有绑定服务商的模板不触发任何终端写入。

决定哪些工具已同步的服务商载荷读取位于 best-effort 区域之内：读取失败退化为空载荷，因此刷新被跳过而不是让命令失败。每个终端刷新错误——无启用本地 Key、工具服务商 upsert 失败、opencode 激活或投影失败——都被吞掉，因此模板视图与其 `synced_at` 更新始终成立，次级步骤绝不对外暴露错误。

## Alternatives considered

- 前端编排：在同步 promise 完成后由 ApiGateway 视图或 `useTemplateAutoRefresh` 触发终端同步。未采纳，因为两个调用方都要重复同样的绑定服务商与已同步工具前置判断及进行中簿记，调度器将依赖它并不拥有的工具状态，而共享的后端命令是唯一能在同一流程里读取已持久化模板配置与工具服务商列表的位置；把传播留在那里也让两个调用点与封装参数保持不变。
- 硬失败：传播终端刷新错误并让命令失败。未采纳，因为刷新对模型清单同步而言是次级步骤；upsert 或投影问题会把一次已落盘、`synced_at` 已更新的模板同步变成可见失败，而终端问题仍可通过显式终端命令看到。
- 无条件刷新：每次模板同步成功后都不论绑定服务商或已同步工具直接调用终端同步。未采纳，因为未绑定模板与从未同步过的工具没有可接收的内容，自动调度器还会为无人使用的模板反复改动工具配置文件。
- 变更检测：仅当拉取的模型清单与已存列表不同时才刷新。未采纳，因为这是没有行为必要的第二层决策面：绑定模板只要存在已同步工具就总是刷新，与人工终端同步的整体替换语义一致，也无需再做一层必须建模显示名、协议、启用标志与增量传播的比较。

## Consequences

- 两个调用方都在无需前端改动的情况下获得该传播：人工「同步模型列表」与自动刷新调度器运行同一命令，因此绑定模板每次成功同步都会刷新其已同步工具，前端既有的同步后重载会在终端面板显示新的同步时间。
- 写入顺序与前置条件：模板状态与派生映射先经既有的单次加密原子写入落盘；终端刷新随后最多运行一次，且仅在存在绑定服务商且已同步工具列表非空时运行。零绑定服务商或零已同步工具意味着零终端调用，模板同步失败绝不进入刷新阶段。
- Best-effort 契约：服务商载荷读取失败退化为「没有已同步工具」，任何终端刷新错误都被吞掉，因此返回的模板视图与 `synced_at` 更新始终成立，绝不暴露次级错误。刷新不新增任何终端同步规则：它复用 `apply_terminal_sync`，因此 [Gateway Reasoning Efforts Sync to OpenCode Model Variants](../architecture/2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md) 与既有台账、标记语义原样适用。
- 契约：无 schema 变更、无新增持久化字段，前端契约仍为 `templateId`；新增的 `AppHandle` 由 Tauri 注入。回滚恢复仅模板同步；已同步的工具记录保持有效，并可随时手动刷新。
- Supersession（取代评估）：部分取代。本记录部分取代 [API Fusion Terminal Sync Writes an Independent Gateway Provider](2026-09-17-api-fusion-terminal-independent-provider.md) 的显式点击写入触发：该记录声明每次写入都只发生在用户显式点击时，而成功模板同步现在同样会触发终端写入路径；其独立网关服务商、标记与台账决策继续有效并被原样复用。本记录也部分取代 [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) 的同步写入边界（其「绝不触碰 `terminal_syncs`」的说法现在只对模板同步阶段成立）以及 [Provider Templates Refresh Automatically on a Persisted Interval](2026-09-23-template-auto-refresh.md) 的命令未改动前提（其调度器现在同样获得终端传播）；两份记录的其余决策均继续有效。[Gateway Reasoning Efforts Sync to OpenCode Model Variants](../architecture/2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md) 的终端同步行为未被取代；刷新将其原样复用。
- `MEMORY.md` 与 `.ai-workflow/index/navigation.json` 的 `api-gateway-backend` 条目在同一变更中记录该 best-effort 刷新、其触发前置与注入的 `AppHandle`，`navigation.md` 已按权威 JSON 重新生成。
