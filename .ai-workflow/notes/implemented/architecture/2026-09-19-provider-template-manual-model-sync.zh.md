# Agent Note: Provider Templates Drop Built-in Model Catalogs and Prices

Status: implemented

[English](2026-09-19-provider-template-manual-model-sync.md) | 中文

## Problem

服务商模板此前是应用内置的整理目录。内置快照携带 OpenCode Zen 的 103 个模型与 CommandCode 的 62 个模型，含四档价格、星期峰谷时段与推理强度；同步管线解析两类厂商专用载荷——`https://models.dev/api.json` 的 `opencode` 条目与 CommandCode 公开模型清单——并重写派生服务商，包括 provider-scoped 价格行。该设计有三个叠加问题：目录会偏离实时端点，且每次刷新模型清单都必须改动随应用发布的快照；整理的价格数据形成第二个价格界面，可能与操作者的映射行价格不一致，且同步可能覆盖操作者手改的价格；同一份官方数据还要经两条路径获取——独立 `api_gateway_fetch_models` 命令用于手工导入模型，模板同步用于模板绑定服务商。已批准的方向是：模板只作为端点定义并携带可选模型清单 URL，模型清单仅经针对该 URL 的显式手动操作刷新，价格完全在服务商映射行上手动维护。

## Decision

内置快照现在只携带元数据与空目录。`provider_templates.json` 保留 `id`、`name`、`description`、`base_url`、`protocol`、`source`、`models_url` 与 `models`；两个内置模板都发布空 `models` 列表并预配置模型清单 URL——`opencode-zen` 为 `https://opencode.ai/zen/v1/models`，`commandcode` 为 `https://api.commandcode.ai/provider/v1/models`——且每个模板的 `source` 即同一 URL。`ProviderTemplate` 删除 `snapshot_version`，`ProviderTemplateModel` 精简为 `upstream_model`、可选 `display_name`、可选 `protocol` 与持久化 `enabled`（`#[serde(default = "default_true")]`，始终序列化；未存该标志的模型读取为启用）；四档价格、`off_peaks` 与 `reasoning_efforts` 不再存在于模板模型上。`parse_template_snapshot` 对非法 JSON、空模板 id 与重复模板 id 保持结构致命，保留模型形状规则（空标识、未知协议字符串与重复 `upstream_model` 只保留首个有效条目），并把空白 `models_url` 归一化为无。

`api_gateway_sync_provider_template` 从模板自身的 `models_url` 同步单个模板；未配置 URL 时返回可操作错误且不写盘。`fetch_template_models` 通过 `reqwest` GET 该 URL，15 秒超时、不带凭据。`parse_model_list_source` 接受 OpenAI 兼容形状——`data` 数组、`models` 数组（`name` 可充当标识）、根数组与纯字符串条目——把对象条目的 `id` 作为上游模型名、其 `name` 经补全为覆盖整个标识的显示名（[Gateway Synced Model Names Cover the Whole Identifier](../bug-fix/2026-09-20-gateway-synced-model-name-completion.md)），并仅在 `supported_endpoints` 为数组时据其推导协议：`/chat/completions` 优先于 `/responses`，声明的数组两者都不含则丢弃该条目，字段缺失或非数组则保留条目且协议为空并继承模板协议。解析结果整体替换模板的模型清单：同名模型由源提供的协议覆盖模板值、显示名取补全后的名称，源未提供则保留本地值，`enabled` 永远保留本地值；新模型默认启用，重复标识保留首个。网络失败、超时、非 2xx、非 JSON 载荷、缺模型数组、条目缺标识与空有效模型集均为致命错误，错误信息包含 URL 与原因。同步在克隆上暂存新模板与全部派生服务商更新，经加密原子 `write_config` 持久化一次后才提交到内存；返回视图记录 `synced_at`、保留模板 `source` 并令 `from_snapshot` 为 false。

同步依旧绝不整体替换派生服务商。`propagate_to_derived` 仅在派生服务商的 `name`、`base_url` 与 `protocol` 仍等于上一版模板值时才更新它们，并为每个启用且未被该服务商 `ignored_models` 阻止、且尚无映射的模板模型新增映射；既有映射的 `display_name` 与 `protocol` 仅在仍等于上一版模板值时才更新。模板中禁用的模型绝不新增，`local_model` 绝不被改动，同步也绝不重新启用既有映射；上一版模板已携带而新清单已移除的模型，其派生映射被同步禁用且行保留，模板不再携带的映射保留在服务商上并继续由 `isMappingDeprecated` 标记 deprecated；同步不创建也不修改任何价格行。[Gateway Template Model Retirement](2026-09-20-gateway-template-model-retirement.md) 记录了该退役规则。手动服务商的字段与映射逐字段不变。

`api_gateway_create_provider_from_template(template_id, name, base_url, protocol, api_key)` 在暂存任何内容之前拒绝空 API Key，名称与 `base_url` 为空时回退模板值，协议始终以参数为准。新服务商携带 `template_id`、`default_model` 为空，获得每个启用模板模型的一条启用映射（`local_model` 等于 `upstream_model`、有显示名时取显示名、模型协议或模板协议）且不写任何价格行；没有启用模型的模板创建出的服务商没有映射。`api_gateway_delete_provider_model` 在模板绑定与手动服务商上都删除该映射与该模型专属价格行，并在绑定服务商上把上游模型恰好一次记入 `ignored_models`，使后续同步无法复活它。`api_gateway_restore_provider_model` 把模型移出忽略集合、仅按模板当前数据重建映射，并在模板已无该模型时返回可操作错误；恢复后的模型在操作者输入价格前仍为未定价。

模板编辑器只维护模型清单：上游名与显示名输入、启用开关（新行默认启用）、新增与删除操作，并通过 `api_gateway_upsert_provider_template` 保存完整列表；没有任何价格输入，也没有获取模型面板。模板卡片仅在 `models_url` 非空时渲染「同步模型列表 / Sync models」操作，保留模型数、数据来源与最近同步时间，列出模型时不显示价格档位、峰谷时段或推理强度，并把禁用模型行以 `data-disabled="true"` 弱化渲染。价格完全在服务商映射行上手动维护：`MappingPriceEditor` 是唯一价格入口。

## Alternatives considered

- 保留内置快照目录并在应用内重新生成官方模型清单：未采纳，因为应用将继续持有两个实时端点的副本、在版本之间漂移，而端点本身就能提供该清单。
- 保留硬编码的 models.dev 与 CommandCode 同步源及其厂商专用解析器：未采纳，因为模板已携带 `models_url`，用户维护的模板可以指向任意端点，逐厂商解析器无法覆盖；被删除的 `TemplateSourceKind` 选择只用于在两个厂商之间切换。
- 保留模板价格行与同步价格传播：未采纳，因为价格因账号而异、整理副本会过期，且同步写入价格可能覆盖操作者的映射行手改；映射行手动定价才是唯一来源。
- 把禁用的模板模型按启用映射传播：未采纳，因为模板模型的 `enabled` 标志是操作者的本地意图；同步不得复活操作者禁用的模型，创建也不得复制它。
- 让本地显示名与协议永久优先于源：未采纳，因为端点才是官方模型名与协议的权威；已批准的源优先规则刷新协议、补全名称规则设定显示名，而「仅当未被改动才更新」的映射规则与本地 `enabled` 标志仍然保护本地意图。

## Consequences

- 模板不再发布目录与价格：在首次同步或手工编辑之前，内置模板的模型清单为空，两个内置模板指向各自的模型清单 URL。被移除字段在读取时忽略；本次变更前写入的持久化模板状态仍可加载，其模型标识、显示名、协议与启用标志继续可用；旧 `api_gateway.json` 仍可读取，因为 `ProviderTemplateState` 与 `ProviderTemplateModel` 保持 serde 默认，并由版本门控迁移一次性升级（[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)）。
- 同步语义：操作手动、按模板隔离；成功同步会整体替换列表，显示名取补全后的名称、协议源优先覆盖、源未提供的本地值与本地 `enabled` 标志保留，并记录同步时间；空白 URL、超时、非 2xx、非 JSON、缺模型数组、条目缺标识与空有效集都会失败，错误包含 URL 与原因且不写任何内容；写入边界仅为模板状态与派生服务商的字段和映射，绝不触碰本地 Key、`terminal_syncs`、转发运行状态、历史用量或价格行。
- 绑定与维护：绑定服务商只为新出现的启用模型补齐启用映射；禁用与忽略的模型绝不新增；既有映射的显示名与协议仅在未被改动时才更新；下架模型保留映射行与 deprecated 标记并被同步禁用；删除会移除映射与该模型专属价格行并把忽略记录恰好写一次；恢复只重建映射、不写价格行；手动服务商不受影响。
- 定价与编辑器：价格完全在服务商映射行上手动维护，`MappingPriceEditor` 是唯一价格入口，因此模板相关流程只能删除价格行（经删除操作）、绝不创建或修改价格行；编辑器与卡片不再渲染价格档位、峰谷时段或推理强度。
- 移除的代码：`api_gateway_fetch_models` 及 `apiGatewayFetchModels` 封装、`fetch_models_from_url`、`fetch_template_source`、`TemplateSourceKind`、`template_source_kind`、`template_source_label`、`parse_source_value`、`ModelsDevSource`、`non_empty_string`、`merge_source_price`、`reasoning_efforts_from_options`、`parse_models_dev_source`、`commandcode_protocol`、`parse_commandcode_source`、`normalize_price` 与 `sanitize_out_of_range_numbers` 均已从产品代码移除，行为测试断言被删命令与封装不再被注册或导出；模板相关命令仍由 `lib.rs` 导出并在 `app_runtime/run_app.rs` 注册。
- 回滚：回退代码无需数据迁移，因为旧构建的 `ProviderTemplateModel` 保留价格字段的 serde 默认并忽略新的 `enabled` 字段，其内置快照仍携带目录，模板创建与同步回到旧的整理形态；本构建写入的持久化模板状态可被旧构建读取，之后旧构建的同步按自身合并规则重新填充模板价格；回退后旧构建通过自身 `cargo test` 与 `npm test`，并可加载本构建写入的配置。
- 部分取代：本记录取代 [API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 的内置 103/62 模型目录、models.dev/CommandCode 专项解析、同步价格传播与 create/restore 价格行，并取代 [Gateway Model Prices Move Into Provider Mappings](2026-09-19-gateway-model-prices-in-provider-mappings.md) 的模板同步价格行规则；第一条记录的称谓、模板绑定、忽略集合生命周期、deprecated 标记、「仅当未被改动才更新」的合并、原子写入与星期价格语义，以及第二条记录的按映射行价格入口、服务商专属精确匹配、读取时归一化与冻结的历史金额均继续有效；其余 active 记录当时互不相关；[Gateway Template Model Retirement](2026-09-20-gateway-template-model-retirement.md) 之后部分取代本记录的退役模型规则，其余决策继续有效；[Gateway Synced Model Names Cover the Whole Identifier](../bug-fix/2026-09-20-gateway-synced-model-name-completion.md) 部分取代本记录「源 `name` 即显示名」的事实，其余决策继续有效；[Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](../feature/2026-09-23-template-terminal-resync.md) 部分取代本记录的同步写入边界：成功同步后共享命令经既有终端管线 best-effort 刷新已同步工具，因此 `terminal_syncs` 与工具记录可由该委派阶段写入，而本记录模板同步阶段自身的边界与其余决策继续有效。
- `MEMORY.md` 与 `navigation.json` 的 `api-gateway` / `api-gateway-backend` 条目在同一变更中描述精简后的模板形状、条件式手动同步、`enabled` 标志及其创建/传播效果、补全后的显示名规则、源优先的协议规则与完全手动的定价，`navigation.md` 已按权威 JSON 重新生成。
