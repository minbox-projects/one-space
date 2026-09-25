# Agent Note: Gateway Migration Is Permanent and Version-Gated

Status: implemented

[English](2026-09-24-version-gated-gateway-migration.md) | 中文

## Problem

网关此前在每次读取配置时都执行兼容逻辑：每次读取都会归一化价格表、转换单数峰谷时段并清空遗留的服务商级运行字段，每条用量日志查询也都排除 `cancelled` 行而没有任何门控真正删除它们；前端保留着配套回退，而 `migration.rs` 被声明为一版生命的临时模块，其删除取决于构建无法观测的升级窗口。这些兼容逻辑只服务于降级：一旦某个版本写入了当前形状的文件，它对该用户就再也不会触发，但之后的每个版本都仍在为其付出成本，而"下个版本删除"的契约还可能静默破坏任何跳过该版本的安装。直接删除兼容同样不可行：从未被归一化的配置会丢失价格与运行状态，而所有受支持查询早已忽略的 `cancelled` 行仍然物理存在。

## Decision

持久化的网关配置携带 `GATEWAY_CONFIG_SCHEMA_VERSION = 2`（`src-tauri/src/ai_gateway/types_config.rs` 的 `GatewayConfig.schema_version`，`#[serde(default)]`，缺失 = 0 = 遗留），用量数据库带有自己的 `PRAGMA user_version` 门控。`src-tauri/src/ai_gateway/migration.rs` 是永久且版本门控的模块：其原先的一版删除契约被版本门控取代，并继续作为非测试代码中遗留文件名、遗留网关标记与遗留字段名（含单数凭据字段 `api_key`）的唯一所在。

- `storage::read_config_file` 始终运行纯迁移，并在存储版本更旧时盖上当前版本并经 `write_config` 以 best-effort 原子改写加密文件。改写失败时保留此前完整的字节，下一次读取重试迁移；已是当前版本的配置读取时不迁移、不写入，未来或未知版本按原样读取且不改写。
- `write_config` 在每次写入时应用 `scope_model_prices` 并盖上当前版本。
- `migrate_legacy_config` 执行原 JSON 迁移：按与原先逐次读取归一化完全相同的规则把全局价格行落到各服务商、把单数 `off_peak` 折叠进 `off_peaks`、清除服务商级运行键，并把遗留的单数 `api_key` 恰好转换为一条启用的 `Default` 命名密钥池条目（空白或缺失得到空池），同时移除旧字段。
- `migrate_legacy_files` 保留一次性的原地文件改名，`has_legacy_gateway_marker` 保留遗留终端标记谓词。
- `migrate_usage_database` 在旧数据库首次打开时把 `PRAGMA user_version` 推进到 1，并以事务一次性删除所有 `result = 'cancelled'` 行；之后的打开不再删除任何行。

每项删除都行为中立：价格归一化、峰谷折叠与运行键清除恰好发生一次，且发生在任何请求能观察到配置之前，产出的内存值与原先的逐次读取归一化一致；计价直接读取 `off_peaks`，因为单数字段在被读取之前就已折叠；服务商级运行字段本来就是不参与过滤的遗留字段，自按模型自动禁用变更起只有映射行结算运行状态，迁移会把旧键从改写后的文件中清除；遗留单数凭据在任何请求能观察到配置之前恰好一次转换为 `Default` 命名条目，空白或缺失得到空池，因此转换不改变服务商实际使用的凭据；`UsageResult::Cancelled` 及其解析与 SQL 排除随行一起删除，而所有用户可见查询此前即排除这些行，因此任何页面、总数、面或分组都不变；`ai_gateway_upsert_provider` / `ai_gateway_upsert_key` 中的 `"********"` 哨兵比较随产生该占位符的前端 `AI_GATEWAY_KEY_MASK` 常量一起删除，因此全空白值仍照旧保留或生成，且任何受支持调用方都无法发送该占位符；整配置命令 `ai_gateway_save_config`（含 `save_config_inner` 及其 `run_app.rs` 注册）在其前端封装移除后已无调用方，所有写入都走保留的服务商、Key、保留天数与模板命令；被删除的 re-export、不可达的 `effective_default_key` 回退、`AttemptLog::default`、`GatewayKey::default` 以及移入 `#[cfg(test)]` 的仅测试 helper（含 `usage_log::is_off_peak`）都没有生产调用方或本就不可达；前端被删除的 `UpstreamProviderDetail` 组件、`resolveMappingPreview` / `GatewayMappingPreview`、没有生产调用方的 props（`UpstreamProviderList` 的 `onDelete` 与 `templateSection`、`ProviderTemplatePickerDialog` 的 `providers` 与 `onEditTemplate`、`ProviderTemplateSection` 的 `hideTitle`）、合并进 `src/components/AiGateway/gatewayShared.ts` 的重复 helper 与 51 个无引用双语键都没有可达调用点，因此构建、lint 与双语门禁保持通过。记录一项与计划的偏差：`templateSection` 实际由 `UpstreamProviderList` 渲染并有专门的 slot 测试覆盖，但没有任何生产调用方传入它，因此移除该 prop 时一并移除了该测试，前端测试数从 1179 变为 1178。

## Alternatives considered

- 按原计划在下个版本删除 `migration.rs`：不采用，因为删除条件无法被构建观测；跳过该版本的安装会丢失价格与运行状态，而版本门控让任何后续版本都能升级任意旧文件，并对当前文件退化为空操作。
- 保留逐次读取归一化与兼容分支、只删除明显死代码：不采用，因为每次读取都仍会为降级专用路径改写内存值，兼容面也会随每次 schema 变更增长，而不是被限制在同一个版本化迁移里。
- 不带数据库版本标记直接删除 `cancelled` 行：不采用，因为删除必须可证明只发生一次；标记记录清理已完成，之后的打开不再删除，重复启动保持幂等。
- 移除前端常量后保留 `"********"` 哨兵契约：不采用，因为空白值在新建与编辑上都已经表示"未提供值"，哨兵只是前端回显掩码的产物；保留比较等于保留一条没有生产者的兼容路径。
- 每次读取都无条件改写迁移后的配置：不采用，因为当前版本的读取必须无副作用，无法完成改写的读取也必须保留此前完整的字节；只有更旧的存储版本才触发 best-effort 写入，失败由下一次读取重试。

## Consequences

- 遗留配置在 schema 版本 2 上被迁移并改写一次（含单数凭据到密钥池的转换），遗留用量数据库在 `user_version = 1` 上被清理一次；当前版本的安装读取与打开时不发生迁移写入与删除。
- 改写或删除失败绝不暴露部分状态：读取仍返回迁移后的内存配置，磁盘文件保留此前完整的字节，下一次读取重试；已删除的 `cancelled` 行无法恢复。
- 迁移幂等且可在任意未来版本安全携带：对当前版本的文件是空操作，未来或未知版本按原样读取、绝不降级。
- 降级保持 best-effort：`off_peaks` 仍是主字段且未知字段被忽略，但回退的构建再也看不到服务商级运行字段或 `cancelled` 行。
- 迁移在任何请求被服务之前读写加密的 `ai_gateway.json` 与用量 SQLite 文件；转发、重试、结算、用量统计、模板同步与终端同步行为不变。
- 取代关系：对五条记录构成部分取代。schema 2 的密钥池转换扩展本迁移而不改变其决定，由 [Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md) 记录。[Rename API Gateway to AI Gateway](2026-09-23-ai-gateway-rename.md) 保留并交叉链接；本记录只取代其 `migration.rs` 的一版删除契约，所有命名与文件改名决定仍然成立。[Gateway Cancelled Requests Are Not Logs](../simplification/2026-09-21-gateway-cancelled-requests-are-not-logs.md) 保留并交叉链接；本记录只取代其让历史 `cancelled` 行物理保留、接受兼容 `status="cancelled"` 过滤并保留 Rust 与 TypeScript 兼容表示的决定，而下行取消或响应不可交付的入站请求写零行的规则与正常完成日志语义仍然成立。[New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md) 保留并交叉链接；本记录只取代其哨兵比较，而"仅名称的新建获得后端生成的密钥"与"编辑留空保留已存密钥"的保证仍然成立。[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md) 保留并交叉链接；本记录只取代其把服务商级运行字段作为读取时清空的反序列化兼容保留的决定，而行级运行状态、结算、恢复与谓词决定仍然成立。[Gateway Model Prices Move Into Provider Mappings](2026-09-19-gateway-model-prices-in-provider-mappings.md) 保留并交叉链接；本记录只取代其读取时归一化并由下一次写入持久化的设计，而按服务商精确匹配、映射对话框入口与金额冻结规则仍然成立。同一次变更中只做事实层更正、不改变其决定的记录包括：[API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md)、[Gateway Per-Attempt Request Logging and Stored Error Text](2026-09-20-gateway-per-attempt-logging-and-error-text.md)、[Gateway Unpriced Hint Counts Only Billable Usage](../bug-fix/2026-09-21-gateway-unpriced-billable-usage.md) 与 [API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md) 中的 `cancelled` 兼容表述；[Gateway Per-Model Mapping Disable Is an Explicit Exclusion](2026-09-18-gateway-per-mapping-disable.md)、[API Fusion Terminal Sync Writes an Independent Gateway Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md) 与 [API Gateway Aggregated Models Open in a Dialog With a Shared Count](../feature/2026-09-17-api-fusion-aggregated-models-dialog.md) 中的服务商级遗留字段与 `resolveMappingPreview` 引用；以及 [API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md)、[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md)、[Gateway Session Affinity Pins a Session and Model to One Upstream](2026-09-20-gateway-session-affinity-routing.md)、[Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) 与 [Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md) 中的"无需迁移"或"schema 不变"表述。
