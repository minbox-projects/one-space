# Agent Note: Provider Templates Own Closed Strategies and Bind Providers by Template Id

Status: implemented

[English](2026-10-10-provider-template-bound-strategies.md) | 中文

## Problem

AI 网关此前从可变的端点字符串选择服务商专属的集成行为：看起来像 CommandCode 的 `base_url` 会获得 CommandCode 额度卡片，看起来像 Go 的 URL 会获得 Go 用量区块，与服务商实际如何配置无关。这种耦合无法表达按模板定义的行为，会把特殊集成为任何 URL 恰好匹配的服务商，也无法区分未绑定与已绑定的服务商。内置目录还以一个 OpenCode Zen 模板为中心，其模型清单不带逐模型协议，因此同步能路由的值没有定义。现存的 Zen 服务商还需要一次性、保留数据地迁移到 OpenCode Go，且绝不因为 URL 像 Zen 就重写服务商。

## Decision

- 模板策略字段：`ProviderTemplate` 携带封闭策略字段 `sync_strategy`（`generic` | `opencode_go`）、`quota_strategy`（`none` | `commandcode` | `opencode_go`）与 `error_strategy`（`generic`），由 `types_config.rs` 的 `SyncStrategy`、`QuotaStrategy` 与 `ErrorStrategy` 定型，且均为 `#[serde(default)]`。`parse_template_snapshot` 宽容读取：缺失或未知的存储值解析为 `generic`、`none` 或 `generic`，而新保存的非法值被拒绝且不改变配置。
- 内置目录：`provider_templates.json` 恰好内置 `commandcode`（`models` 为空，模型清单 URL `https://api.commandcode.ai/provider/v1/models`，`sync_strategy = generic`，`quota_strategy = commandcode`）与 `opencode-go`（base URL `https://opencode.ai/zen/go/v1`，模型清单 URL `https://opencode.ai/zen/go/v1/models`，`sync_strategy = opencode_go`，`quota_strategy = opencode_go`）。不再有 `opencode-zen` 内置模板。
- Go 声明基线：OpenCode Go 模板恰好声明 20 个 Chat Completions 模型 id 与 6 个 Responses 模型 id。`parse_model_list_source` 对 `SyncStrategy::OpencodeGo` 模板是声明感知的：解析源之后只保留有效模板显式声明为 Chat Completions 或 Responses 的 id，或源自身以受支持端点声明的 id，并把已声明 id 的协议改写为声明值；源以 `/messages` 或 `/v1/messages` 声明的 id 现在被识别为 Anthropic Messages，而未声明或元数据缺失的 id 绝不添加，也绝不按 Chat 猜测。`generic` 模板保持既有的协议继承解析。
- 绑定与有效策略解析：`ai_gateway_upsert_provider` 已携带 `template_id: Option<String>`，因此没有新的绑定命令。提交的未知或已删除 id 在暂存任何内容前被拒绝，`None` 解除绑定，该值经 `effective_template`（已持久化模板状态优先，其次内置快照，未知或已删除 id 返回错误）、`effective_provider_template`（未绑定或损坏绑定时返回 `None`）与 `effective_quota_strategy`（默认 `none`）解析。解析只读取 `template_id`；绝不查询 URL、名称或图标。
- 基于策略的监控派发：`resolve_quota_request` 只接受有效配额策略为 `commandcode` 的服务商，`resolve_go_usage_request` 只接受 `opencode_go`；两者在读取任何 key 或 base URL 之前就拒绝不匹配，因此不可能发起上游抓取。固定端点、固定源 key、每服务商五分钟缓存、强制刷新、源 key 与 base URL 失效以及只读边界均不变，且策略或绑定变化不能复用不兼容的快照，因为策略守卫在读取缓存之前就拒绝请求。
- Schema 3 迁移：`GATEWAY_CONFIG_SCHEMA_VERSION` 提升到 3。版本门控的 `migrate_legacy_config` 先执行幂等的原始 JSON `opencode-zen` → `opencode-go` 变换，仅由已持久化的 `template_id` 驱动：模板 id 与绑定服务商改为 `opencode-go` 与 Go base URL，已持久化模板快照与 `source` 变为 Go 基线，`synced_at` 清空，服务商账户与用户维护数据（name、keys、enabled 状态、weight、tags、icon、价格行、显式协议）保留，未声明为 Go Chat/Responses 的遗留映射被置为禁用并恰好一次写入 `ignored_models`，不受支持的 `default_model` 被清除，原先继承旧 provider 默认的受支持 Responses 映射获得显式 Responses 协议。URL 仅仅像 Zen 的服务商绝不迁移，已存在的 Go 模板状态确定性地丢弃过时的 Zen 重复项，失败的改写保留旧的完整字节供下一次读取重试，更高版本不降级地按原样读取。

## Alternatives considered

- 对额度与 Go 派发保留 URL/名称启发式：未采纳，因为它会给任何端点字符串恰好匹配的服务商特殊集成，无法区分未绑定与已绑定服务商，并在端点迁移时失效；显式模板策略稳定且可见。
- 增加服务商级策略覆盖或新的绑定命令：未采纳，因为模板（或 generic 回退）是唯一策略来源，且现有 provider upsert 已携带绑定字段。
- 内置一份带价格与推理强度的 OpenCode Go 目录：未采纳，理由与早先移除模板目录相同——价格与推理强度因账户而异并维护在映射行上；Go 模板只随附决定哪些可同步 id 可路由的协议声明。
- 为未声明 id 推断协议，或复用 Zen `/zen/v1/models` 端点：未采纳，因为 Go 模型清单不带逐模型协议，且 Zen 端点是另一个产品面；只有显式 Chat/Responses 声明具有权威性。
- 迁移每个 URL 像 Zen 的服务商：未采纳，因为 URL 相似不是绑定；只有已持久化的 `opencode-zen` `template_id` 才触发迁移，因此用户配置的服务商绝不会被静默重写。
- 放弃失败的迁移改写而不保留旧字节：未采纳，因为失败的改写不得暴露部分状态；迁移保留先前的完整字节，由下一次读取重试。

## Consequences

- 服务商集成只从 `template_id` 解析：已绑定服务商遵循其模板策略，未绑定或损坏绑定使用 generic 回退（`generic`/`none`/`generic`），没有任何服务商从 URL 获得 CommandCode 额度或 OpenCode Go 用量。损坏绑定按 generic 路由，UI 显示缺失目标，直接监控命令在抓取前拒绝不匹配。
- 内置目录恰好是 CommandCode 与 OpenCode Go；Go 模板的 20 个 Chat 与 6 个 Responses 声明决定可路由的 Go 模型集合，而 generic 模板保持既有的协议继承同步，源以 `/messages` 声明的 id 现在作为 Anthropic Messages 映射被接纳，而不再不可路由。
- 额度与 Go 用量保持只读、按服务商五分钟缓存、可强制刷新，并在源 key 或 base URL 变化时失效；策略或绑定变化不能复用不兼容的快照，监控查询绝不改变服务商健康、配置、用量日志或终端同步。
- Schema 3 迁移保留服务商账户与用户维护数据，同时把不受支持的遗留映射隔离为禁用的 `ignored_models`，清除不受支持的默认值，且是幂等的；第二次读取字节稳定，失败的改写保留旧字节供重试，更高版本不降级地读取。
- 回滚：回滚产品代码不会逆转已持久化的 schema 3 身份迁移。要运行仍需要 OpenCode Zen 身份的旧构建，请恢复同一 profile 迁移前的加密 `ai_gateway.json` 备份并核对旧模板 id、base URL、映射与价格；仅二进制回滚会保留已持久化的 `opencode-go` 身份。
- Supersession：部分取代六条记录。本记录取代 [CommandCode Provider Cards Show Account Quota](../feature/2026-09-23-commandcode-provider-quota.md) 与 [OpenCode Go Provider Cards Show Usage](../feature/2026-09-24-opencode-go-provider-usage.md) 的基于 URL 的额度派发，以及 [Upstream Provider List Bulk-Refreshes Quota for the Current Filter Results](../feature/2026-09-28-provider-quota-bulk-refresh.md) 的基于 URL 的批量刷新合格性；它为 [API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 与 [Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 取代内置 `opencode-zen` 目录，并加入封闭策略字段、绑定校验与有效策略解析；它把 [Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 从 schema 2 推进到 schema 3 并加入 Zen 到 Go 变换。各来源记录保留其端点、缓存、只读边界、命名与结构决策，声明感知的 Go 同步仅对 Go 模板取代空目录假设。
- 取代更新：[Native Anthropic Messages Entry, Protocol Recognition and OpenAI Entry Conversion](2026-10-11-ai-gateway-anthropic-messages.md) 部分取代本记录的 Go 声明基线与端点识别边界：源声明的 Messages 端点现在作为 Anthropic Messages 进入兼容集合，且其 Anthropic Messages 映射可由转换经既有 OpenAI 入口服务，而封闭策略字段、绑定校验、有效策略解析与 schema 3 迁移决策继续有效。
