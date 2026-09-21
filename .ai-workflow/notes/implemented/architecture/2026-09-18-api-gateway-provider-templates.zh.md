# Agent Note: API Gateway Provider Templates and Incremental Model Sync

Status: implemented

[English](2026-09-18-api-gateway-provider-templates.md) | 中文

## Problem

为 OpenCode Zen 或 CommandCode 手工搭建上游服务商既繁琐又易错：操作者要把数十个上游模型名、四档价格、峰谷时段与推理强度清单从官方来源抄进服务商表单与价格表。官方数据会持续变化，本地副本没有任何机制保持同步；而在刷新时整体替换又会重新启用用户禁用的模型、复活用户删除的模型并覆盖用户手改的价格。价格模型无法表达按星期限定的峰谷计划（例如 CommandCode DeepSeek 的谷时在工作日与周末不同），也没有任何服务商记录与其来源目录之间的机器可读关联。

## Decision

服务商模板以内置快照形式随应用发布。`src-tauri/src/api_gateway/templates.rs` 解析以 `include_str!` 嵌入的 `src-tauri/src/api_gateway/provider_templates.json`，生成 `ProviderTemplate` 与 `ProviderTemplateModel`；`parse_template_snapshot` 把非法 JSON、空模板 id 与重复模板 id 视为致命，而模型级问题为非致命：标识为空的模型或协议未知的模型被丢弃，重复 `upstream_model` 保留第一条，空白 `models_url` 归一化为无。内置两个模板：`opencode-zen`（`base_url` `https://opencode.ai/zen/v1`、模型清单 URL `https://opencode.ai/zen/v1/models`）与 `commandcode`（`base_url` `https://api.commandcode.ai/provider/v1`、模型清单 URL `https://api.commandcode.ai/provider/v1/models`）。[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 移除了内置的 103/62 模型目录及其价格、峰谷时段、推理强度与 `snapshot_version`，因此快照现在为每个模板声明模型清单 URL 并携带空 `models` 列表；只有网关可服务的模型才能进入同步后的模板：声明的 `supported_endpoints` 既不含 `/chat/completions` 也不含 `/responses` 的条目被丢弃，而没有端点信息的条目保留并继承模板协议。`api_gateway_provider_templates` 按快照顺序返回内置模板、追加已持久化的自定义模板并给出 `synced_at`、`source`、`from_snapshot`，已持久化的同步结果优先于快照。`api_gateway_sync_provider_template` 现在即取代记录所述的手动模型清单抓取：要求模板 `models_url` 非空（否则返回可操作错误且不写入），以 `reqwest` GET 该 URL（15 秒超时、不带凭据），并按「源优先的显示名与协议 + 本地拥有的 `enabled` 标志」整体替换模板模型清单。网络失败、非 2xx 响应、非 JSON 载荷、缺模型数组、条目缺标识与空有效模型集为致命错误，返回含 URL 与原因的可操作错误；新模型默认启用，源未提供显示名或协议时保留本地值。解析后的模板状态持久化为 `GatewayConfig.provider_templates: Vec<ProviderTemplateState>` 并带 `#[serde(default)]`，因此旧 `api_gateway.json` 无需迁移仍可反序列化。

同步是增量下发，绝不整体替换派生服务商。`apply_template_sync_with` 在配置克隆上暂存、更新每个 `template_id` 匹配的服务商、通过既有加密 `write_config` 一次性持久化克隆，只有持久化成功后才提交到内存；写入失败则配置保持原状，且同一模板派生的多个服务商各自独立接收更新。只有当字段仍等于上一版模板值时才会更新：服务商 `name`、`base_url` 与 `protocol`，以及映射 `display_name` 与 `protocol`。映射 `local_model` 绝不被改动，同步只禁用、绝不启用映射：上一版模板已携带而新清单已移除的模型对应派生映射被禁用并保留行，因此用户禁用的模型不会被重新启用，退役模型也不再继续服务。手动服务商没有 `template_id`，因此任何同步后其全部字段与映射逐字段不变。启用的新模板模型会以启用映射补齐，除非其名称在该服务商的 `ignored_models` 中；模板中禁用的模型绝不补齐；由于模板不携带价格数据，同步绝不创建或修改任何价格行。在模板绑定服务商上删除模型会移除其映射、把上游模型恰好一次记入 `ignored_models` 并删除其 provider-scoped 价格行，因此后续同步无法复活它；在手动服务商上删除只移除映射与对应价格行、不写忽略记录。恢复被忽略模型只按模板当前数据重建映射、将其移出忽略集合，模板已无该模型时返回可操作错误。官方来源下架的模型在每个派生服务商上保留映射，并由界面中的 `isMappingDeprecated` 标记 deprecated；同步会禁用该映射，因此在操作者显式重新启用之前，它按既有启用标志规则退出转发候选、`GET /v1/models` 与聚合视图。[Gateway Template Model Retirement](2026-09-20-gateway-template-model-retirement.md) 记录了该规则。

从模板创建的服务商与该模板绑定。`api_gateway_create_provider_from_template(template_id, name, base_url, protocol, api_key)` 在暂存任何内容之前拒绝空 API Key，名称与 `base_url` 为空时回退模板值，协议始终以参数为准。新服务商携带 `template_id`、`default_model` 为空，并获得每个启用模板模型的一条启用映射（`local_model` 等于 `upstream_model`、有官方显示名时取其值、模型协议或模板协议），且不写任何价格行。`api_gateway_delete_provider_model` 与 `api_gateway_restore_provider_model` 实现上述删除与忽略集合生命周期；在手动服务商上，删除只移除映射与其 provider-scoped 价格行，不写忽略记录。共落地五个命令：`api_gateway_provider_templates`、`api_gateway_sync_provider_template`、`api_gateway_create_provider_from_template`、`api_gateway_delete_provider_model` 与 `api_gateway_restore_provider_model`。

峰谷时段可以按星期限定。`OffPeakPrice` 新增 `days: Option<Vec<u8>>`，即可选的 UTC+8 星期集合，`0` 为周日、`6` 为周六；缺省或空表示每天，该集合在读取与写入时归一化（丢弃越界值、去重、升序）并按归一化结果序列化。`is_off_peak_with_days` 保留既有 `[start, end)`、跨午夜（`start_time > end_time`）与零时长语义，并额外要求请求的 UTC+8 星期在集合内；`compute_cost_at_time` 按首个命中时段计价，旧单值 `off_peak` 保持原路径。没有该字段的旧配置无需迁移且结果完全相同。[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 移除了快照中整理的 CommandCode 峰谷数据，因此没有任何模板携带时段，该字段改由操作者在 provider-scoped 价格行上手动维护。前端通过 `MappingPriceEditor` 以及 `src/lib/apiGateway.ts` 的 `formatOffPeakDays` 与 `normalizeReasoningEfforts` 辅助函数编辑与展示该集合。

本功能保留既有称谓。中文名为「服务商模板」，英文概念为 "Provider Template"、区块标题为 "Provider Templates"，而「上游服务商 / Upstream providers」仍是已配置转发目标的名称；没有重命名任何后端类型、导航 id 或配置字段。模板区通过 `ProviderTemplateSection` 与 `TemplateCreateDialog` 渲染，模板绑定服务商的映射推理强度在 `ProviderDetailDialog` 中展示与编辑，并会作为 `variants` 写入 opencode 终端同步的模型条目（见 [Gateway Reasoning Efforts Sync to OpenCode Model Variants](2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md)）。

## Alternatives considered

- 把功能命名为预设服务商 / Provider Presets 或模型目录 / Model Catalog：未采纳，因为应用已把 presets 用于 AI Environments 的预填服务商，而 catalog 会歪曲模板现在的定位——携带可选模型清单 URL 的端点定义，绑定派生服务商并直接创建服务商，而价格、峰谷时段与推理强度位于服务商映射上。
- 把「上游服务商 / Upstream providers」改名为通用的「服务商」，并用「服务商模板」独占模板概念：未采纳，因为既有导航、后端类型与文档都使用上游服务商这一词汇，改名只会破坏它且没有收益。
- 同步时整体替换派生服务商（快照全胜，无增量规则）：未采纳，因为那会重新启用被禁用的模型、复活被删除的模型并覆盖手改价格，与用户意图优先的要求相矛盾。
- 删除模型时不写忽略集合：未采纳，因为下次同步会静默复活它，用户必须在每次同步后重复删除。
- 自动禁用或自动删除官方来源下架的模型：未采纳，因为官方缺失不是用户决策；映射保留并标记 deprecated。[Gateway Template Model Retirement](2026-09-20-gateway-template-model-retirement.md) 之后采纳了其中的禁用一半——模板同步现在会禁用已移除模型的派生映射并保留行——因此只剩自动删除仍被否决，被显式重新启用的映射也不会再次被禁用。
- 后台自动或按计划同步：未采纳，因为本功能的同步是显式用户操作，自动写入会在未经同意的情况下改动本地配置。
- 在 v1 支持第三方或用户自定义模板以及模板解绑/重绑：未采纳，属 v1 范围之外；只内置两个模板，数据结构保持可扩展。
- 用非官方抓取路径获取 CommandCode 价格与推理强度，或在保留整理快照的同时不提供它们：未采纳，因为没有公开的机器可读来源；[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 之后移除了模板中的整理价格数据，因此价格、峰谷时段与推理强度现在改在服务商映射上手动维护。
- 把 `days` 设为必填，或把星期限定建模为独立时段类型：未采纳，因为可选集合是增量的，保留旧配置的序列化与行为，且零迁移。

## Consequences

- 模板数据在首次同步前仍离线可用：内置快照携带两个内置模板的元数据与模型清单 URL、目录为空，模板卡片展示模型数、数据来源与最近同步时间；同步失败保留模板当前状态而不是清空；[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 移除了全部整理的模型、价格、峰谷与推理强度条目，因此目录跟随其端点或操作者的手工编辑。
- 同步为手动、按模板隔离，且仅在模板声明模型清单 URL 时提供：仅正在同步的模板操作禁用并显示进行中，致命的来源问题返回含 URL 与原因的可操作错误且不写入任何内容，整次更新只经一次加密原子 `write_config` 落盘；写入边界仅为模板状态与派生服务商的字段/映射，绝不触碰本地 Key、`terminal_syncs`、转发运行状态、价格行或历史用量记录。
- 用户意图在每次同步后保持：模板中禁用的模型绝不补齐，映射的禁用标志不会被重新启用，被删除模型留在 `ignored_models` 中且不被恢复，手改的价格、推理强度、显示名、服务商名称、`base_url` 与协议保留用户值（同步不写入任何价格或推理数据），启用的新模型被补齐，官方下架模型保留、标记 deprecated 且其派生映射被同步禁用，手动服务商逐字段不变，同一模板的多个派生服务商各自独立接收更新。
- 从模板创建的服务商携带绑定、`default_model` 为空、每个启用模板模型一条启用映射且不写价格行；删除会移除映射与该 provider-scoped 价格行并在绑定服务商上记录被忽略模型，恢复重建映射且不写价格行，模板已无该模型时返回可操作错误；五个命令 `api_gateway_provider_templates`、`api_gateway_sync_provider_template`、`api_gateway_create_provider_from_template`、`api_gateway_delete_provider_model` 与 `api_gateway_restore_provider_model` 由 `lib.rs` 导出并在 `app_runtime/run_app.rs` 注册。
- 星期限定计价是增量的：没有 `days` 的时段表示每天，旧配置无需迁移且结果不变，旧单值 `off_peak` 不受影响，`MappingPriceEditor` 展示并编辑星期（空表示每天），推理强度在映射上维护并作为 `variants` 写入 opencode 终端同步的模型条目，而模板不携带任何档位（[Gateway Reasoning Efforts Sync to OpenCode Model Variants](2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md)）；内置的 CommandCode 峰谷窗口属于整理的快照数据，已被 [Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 移除。
- 兼容与回滚：`provider_templates`、`template_id`、`ignored_models`、`reasoning_efforts` 与 `days` 全部 serde 默认，旧 `api_gateway.json` 免迁移可读，旧构建忽略新字段并继续用既有映射与价格转发；回滚后旧版本下一次保存会丢弃模板状态与忽略记录，而映射、价格行与用量历史保留，历史金额永不重算；携带 `days` 的新价格行会被旧版本按每天解释，这只可能影响回滚后新请求的峰谷判定，必须记录在回滚说明中；移除模板区入口与新命令的界面调用即可停止使用该功能，且因为没有重命名任何类型、导航 id 或配置字段，回收命名只需改文案、无数据迁移。
- `MEMORY.md`、`navigation.json` 与重新生成的 `navigation.md` 在同一变更中描述模板形状与模型清单 URL、绑定与忽略集合、增量合并、星期价格语义与五个命令。
- Supersession（取代评估）：`ai-workflow notes list` 显示本记录对 [API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md) 与 [Gateway Per-Model Mapping Disable Is an Explicit Exclusion](2026-09-18-gateway-per-mapping-disable.md) 均无完全或部分取代，因此两条记录都保留；用量记录的日志、记录时价格固化与保留决策仍然有效，其价格入口与全局匹配决策已被 [Gateway Model Prices Move Into Provider Mappings](2026-09-19-gateway-model-prices-in-provider-mappings.md) 部分取代；星期字段是增量（缺省即每天、旧结果不变），映射记录的映射级 `enabled` 语义、服务商/映射独立性与显式排除规则仍然有效——退役同步只会禁用已移除模型的派生映射、绝不启用任何映射——而删除/忽略/deprecated 是另一套生命周期；[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 部分取代本记录：它替换了内置 103/62 模型目录、models.dev/CommandCode 专项解析、同步价格传播与 create/restore 价格行，而上述称谓、模板绑定、忽略集合生命周期、deprecated 标记、「仅当未被改动才更新」的增量合并与星期价格语义继续有效；其余 active 记录互不相关，因为本次变更只新增可选字段、一个快照模块与新命令，不改变转发、终端同步、日志、兼容清理或流程决策。
