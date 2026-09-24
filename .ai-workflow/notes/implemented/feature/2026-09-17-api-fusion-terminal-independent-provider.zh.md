# Agent Note: API Fusion Terminal Sync Writes an Independent Gateway Provider

Status: implemented

[English](2026-09-17-api-fusion-terminal-independent-provider.md) | 中文

## Problem

API 网关的终端同步此前会就地改写用户既有的 `opencode` / `codex` 服务商记录，只替换其 `base_url` 与 `api_key`。这种做法仅在记录已存在时可用，会静默改动用户自己的服务商配置，也无法表达用户从未启用的网关；它还没有位置携带网关的模型映射列表，因此工具无法显示网关对外提供的模型。

## Decision

`api_fusion_configure_terminal` / `api_fusion_sync_terminal` 不再改写既有记录。对每个受支持工具（`opencode`、`codex`），后端创建或更新恰好一条名为 `API Gateway` 的独立网关服务商记录，携带网关的本地 Api 地址、生效的默认本地 Key 与网关的模型映射列表，并始终写入 `api_fusion_gateway` 标记（顶层或 `tool_config` 内均可识别）。该记录本身从不携带激活标记；台账写入后，后端会激活 `opencode` 下的网关服务商并投影写入 `~/.config/opencode/opencode.json`（含 `options.apiKey`），`codex` 则保持手动激活与手动投影。只有被用户启用的服务商贡献模型，且只有其被用户启用、未被自动禁用的映射行贡献：`opencode` 写入 `tool_config.models`，以每条此类映射的 `local_model` 为键，值中的名称取 `display_name`，缺省回退远端模型名；`codex` 写入 `tool_config.wire_api = chat` 并把 `model` 设为所有启用服务商中首个非空且被用户启用、未被自动禁用的映射 `local_model`，只有在不存在这类映射时才回退首个非空 `default_model`，两者都不可用时省略该键。类型中已不存在服务商级运行字段，且版本门控迁移会清除遗留的服务商级键（[Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)）。再次同步仅在 `terminal_syncs` 台账的 provider id 仍指向同一工具且带网关标记的服务商时才复用它；台账 id 已指向未标记的用户记录即为过期，此时新建带全新 UUID 的独立网关服务商，而不是改写用户记录。只有没有任何台账 id 匹配时，后端才回退到该工具已带标记的网关服务商；两者都不存在时才生成新 id。`api_fusion_terminal_targets` 对每个受支持工具各返回一个目标并套用同一标记规则，因此未标记的服务商永远不会被报告为已同步。两个命令都通过 `target_tools`（前端封装键为 `targetTools`）接收工具名；工具名大小写不敏感校验并统一以小写（`opencode` / `codex`）存储。映射 schema 新增可选 `display_name`（`Option<String>`，serde 默认且缺省时不序列化）。前端不再自行推导待同步状态，而是读取后端提供的 `target.pending_sync`；导出的 `isTerminalSyncPending` 辅助函数已从 `src/lib/apiFusion.ts` 删除。`claude` / `antigravity` 记录与 Protocol Router 的 route 数据仍不被触碰。

终端同步面板不再提供顶部全局添加/同步按钮与按目标的多选框：每个工具行只在最右侧带一个操作按钮。从未添加过的工具（`hasBeenAdded = synced || synced_key_id != null || synced_at != null` 为假）显示 `添加服务商` 并调用 `api_fusion_configure_terminal`；已添加过的工具显示 `同步` 并调用 `api_fusion_sync_terminal`。后端提供的 `pending_sync` 只驱动该行的已同步/待同步徽标；行不会因为缺少默认 Key 而被禁用，没有启用本地 Key 时命令会被拒绝并提示先新增并启用一个本地 Key。每次写入仍只在用户主动点击时发生；上述 `opencode` 的激活与投影在该次同步内完成，`codex` 保持手动。如果用户已在 `AI Environments` 中删除网关记录，再点 `同步` 会自动新建一条全新的 `API Gateway` 记录。

## Alternatives considered

- 保持改写既有 `opencode` / `codex` 记录：未采纳，因为它会静默改动用户自己的服务商配置、要求记录已存在、无法表达未启用的网关，也没有位置携带模型映射列表。
- 再次同步时无条件信任 `terminal_syncs` 台账 provider id：未采纳，因为过期或被复用的 id 可能指向用户自己的服务商，因此仅当目标记录仍带网关标记时才复用台账 id；未标记的记录视为过期并新建独立服务商。
- 连 `codex` 网关也一并自动激活并投影：未采纳，因为 `codex` 保持手动激活与手动投影，只有 `opencode` 网关被自动激活并投影，使其记录与 `options.apiKey` 真正落盘。
- 不论状态纳入所有网关的模型：未采纳，因为网关自身的 `/v1/models` 列表与候选选择只服务被用户启用的服务商及其被用户启用、未被自动禁用的映射行，列出其余内容会宣告本地服务拒绝提供的模型。
- 每次同步都新建服务商：未采纳，因为反复同步会累积重复的网关服务商；按工具的台账条目让再次同步幂等地落在同一条带标记的记录上。

## Consequences

- 同步 `opencode` 会激活其 `API Gateway` 服务商并投影写入 `~/.config/opencode/opencode.json`，网关记录与其密钥随该次同步真正落盘；`codex` 仍需在 `AI Environments` 中手动启用并手动投影。
- 台账按工具键控并保存 provider id。再次同步仅在该记录仍是带标记的同工具网关服务商时更新同一条记录；记录变为未标记、被改名或被删除时，后端回退到已带标记的网关服务商，否则新建独立记录，绝不改写用户自己的服务商。
- `codex` 的单一 `model` 取启用服务商中首个非空且被用户启用、未被自动禁用的映射 `local_model`；只有在不存在这类映射时才回退到首个非空 `default_model`，否则省略该键。
- `ModelMapping` 新增可选 `display_name` 字段，用于网关的模型列表，并可通过本地模型名称输入框（`apiFusionLocalModelNamePlaceholder` / `apiFusionLocalModelNameAria`）编辑；未使用的 `apiFusionLocalModelName` 键已从 `src/i18n.ts` 删除。
- `src/lib/apiFusion.ts` 不再导出待同步状态辅助函数；终端同步面板与页签角标直接读取后端提供的 `target.pending_sync`。
- 终端同步面板为每行单操作，没有全局按钮、多选框与批量同步：该行按钮在首次写入前显示 `添加服务商`，之后显示 `同步`（`apiFusionSyncOne`）；孤儿键 `apiFusionSyncSelected` 已删除，`apiFusionTerminalSyncDesc` 已按单行操作面板重写。
- `MEMORY.md`、`docs/USAGE.md` 与 `navigation.json` 描述这种独立服务商行为；终端同步测试已据此重写。
- 部分取代：本记录由 [Gateway Per-Model Mapping Disable Is an Explicit Exclusion](../architecture/2026-09-18-gateway-per-mapping-disable.md) 保留并交叉链接，后者收窄了上文的终端同步选择，使 `opencode` `models` 与 `codex` `model` 只取被用户启用的映射；[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md) 进一步跳过自动禁用行并让服务商谓词仅为 `enabled`，并由 [Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](2026-09-23-template-terminal-resync.md) 保留并交叉链接，后者在成功模板同步后追加 best-effort 终端刷新，因此扩展了本记录的显式点击写入触发；此处的独立网关服务商决策仍然成立。
