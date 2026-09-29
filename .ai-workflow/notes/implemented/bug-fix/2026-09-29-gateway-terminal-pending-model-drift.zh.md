# Agent Note: Gateway Terminal Sync Flags Model-Selection Drift as Pending

Status: implemented

[English](2026-09-29-gateway-terminal-pending-model-drift.md) | 中文

## Problem

终端面板的 `pending_sync` 标志此前只比较持久化的同步台账：上次写入的默认本地 Key id 与本地 Api 地址；它从不检查上次同步已经在工具侧写出的网关服务商记录。当上游发生变化（映射协议、`enabled` 翻转、映射自动禁用或不同的 `default_model`），或应用升级改变了生成的模型清单（例如为仅 Responses 的 OpenCode 模型加入的按模型 `provider.npm` 覆盖）之后，已存记录可能已经过期，而徽标仍显示绿色「已同步」。用户看不到任何需要再点 `Sync` 的信号。

## Decision

`terminal_targets_from`（`src-tauri/src/ai_gateway/commands.rs`）现在对每个已同步目标把台账比较与模型选择比较取或。新增私有 helper `terminal_model_selection_drifted(tool, stored, generated)`，把工具侧已存网关记录与针对同一 provider id、工具与本地 Api 地址新构建的 `build_gateway_provider` 载荷比较：

- `opencode`：深比较 `tool_config.models` —— 同步会写出的完整模型清单，含条目形态与数组顺序。
- `codex`：比较记录顶层的 `model` 字段，而非 `tool_config.model`（后者保存 `wire_api`），因为同步把 codex 的模型写在顶层。
- 比较键仅在一侧存在即视为漂移。
- 载荷构建失败视为漂移，因此无法构建的新选择绝不报告为已同步。
- 绝不比较 Key 值：比较载荷以空占位 Key 构建，因为应用侧服务商列表会脱敏 opencode 的 Key。Key id 漂移继续由既有台账比较（`terminal_sync_pending`）覆盖。

因此，带台账记录的已同步目标的 `pending_sync` 为 `terminal_sync_pending(record, default_key_id, base_url) || terminal_model_selection_drifted(...)`；没有台账记录时过去与现在都显示待同步。`synced` 谓词、台账复用规则与响应字段保持不变，因此 `synced` 可以为真而 `pending_sync` 同时为真。

## Alternatives considered

- 比较整个生成的网关记录（含 `base_url` 与 `options.apiKey`）：未采纳，因为应用侧列表拿不到 Key 值（opencode 的 Key 会被脱敏），而 Key id 与 Api 地址漂移已由台账比较覆盖。
- 对 `codex` 比较 `tool_config.model`：未采纳，因为同步把 codex 的模型写在记录顶层，`tool_config` 保存的是 `wire_api`，比较该路径永远发现不了漂移。
- 在同步台账中持久化模型清单指纹：未采纳，因为这会增加一份需要长期保持版本的存储副本，而按当前配置重建新载荷是精确的，且不改动台账 schema。
- 把载荷构建失败视为未漂移：未采纳，因为此时无法验证新的模型选择，而错误的「已同步」恰好会掩盖该检查要暴露的过期状态。

## Consequences

- 任何改变模型选择的上游映射、协议、启用或自动禁用状态或 `default_model` 变更，以及任何改变生成条目的应用升级（例如按模型的 `provider.npm` 覆盖），都会让此前同步过的行在用户再次同步前显示 `待同步`；该行的单个操作按钮对「已同步但待同步」的目标本就显示 `Sync`，因此无需前端改动。
- 该检查只读：`ai_gateway_terminal_targets` 只在内存中构建新载荷，绝不写配置、台账或工具记录。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml --lib terminal_targets_from`（exit 0，9 通过）与 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::`（exit 0，588 通过、0 失败）；测试覆盖 opencode 模型漂移与匹配记录、codex 顶层 `model` 漂移以及已同步夹具。
- Supersession（取代评估）：部分取代。[API Fusion Terminal Sync Writes an Independent Gateway Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md) 记录终端面板读取后端提供的 `pending_sync` 而不自行推导待同步状态；该前端契约与独立服务商决策继续有效，本记录扩展了该值背后的后端 `pending_sync` 计算。[API Gateway Resolves the Listening Port per Build Profile](../architecture/2026-09-22-gateway-build-profile-port.md) 记录跨 profile 的 Api 地址漂移属于已接受的待同步情形，它仍是两条漂移分支之一；[Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](../feature/2026-09-23-template-terminal-resync.md) 复用的 `synced` 谓词保持不变。
- `MEMORY.md`、`docs/USAGE.md`、`.ai-workflow/index/navigation.json` 的 `ai-gateway-backend` 条目与重新生成的 `navigation.md` 在同一变更中记录该模型选择漂移规则。
