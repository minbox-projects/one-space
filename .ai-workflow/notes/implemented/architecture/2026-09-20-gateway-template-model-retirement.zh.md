# Agent Note: Gateway Template Model Retirement Disables Derived Mappings

Status: implemented

[English](2026-09-20-gateway-template-model-retirement.md) | 中文

## Problem

模板同步此前只能补齐模型，无法让模型退役。同步会按 `models_url` 整体替换模板的模型清单，但端点已下架的模型仍在每个派生服务商上保留映射，且该映射保持启用、继续服务，直到操作者在服务商对话框中手工禁用或删除它。于是同步结果与其留下的转发状态相互矛盾，服务商卡片既看不出哪些映射来自模板，也看不出某个映射的模型已不在模板中。

## Decision

模板同步现在会把「上一版模板已携带、抓取列表已不存在」的模型对应派生映射禁用而不删除。`propagate_to_derived`（`src-tauri/src/api_gateway/templates.rs`）把退役集合计算为 `previous.models` 中 `upstream_model` 未出现在抓取列表的条目，按 `upstream_model` 精确匹配，并对每个 `template_id` 匹配的服务商把该上游模型的映射置 `enabled = false`。退役集合仅该差集：上一版模板从未携带的映射、本就禁用的映射、操作者早先删除或忽略的模型、手动服务商与绑定其他模板的服务商都绝不被触碰，同步也不写入任何 `ignored_models` 记录。只有映射的 `enabled` 标志改变：行、`local_model`、`display_name`、`protocol`、`reasoning_efforts`、服务商 `default_model` 与所有价格行保持原样，不新增任何持久化字段；模板只是禁用但仍列出的模型不会退役。新模板与全部派生服务商更新在克隆上暂存，并只经一次加密原子 `write_config` 落盘；写入失败则保持原状态。

退役是单向的。同步绝不重新启用既有映射——对既有映射只会写入 `false`——因此模型回归模板后退役映射仍保持禁用；操作者经服务商对话框显式重新启用的映射，在后续仍列出该模型的同步后也保持启用；既有规则仍会为新出现的启用模板模型补齐一条启用映射。由于禁用映射受既有启用标志语义排除，退役模型无需任何下游改动即退出候选选择、`GET /v1/models`、聚合模型视图与终端同步模型清单。

上游服务商卡片按当前状态解释退役。`UpstreamProviderList` 接收已加载的 `templates` 视图，把服务商 `template_id` 解析到模板视图后，在卡片标题旁渲染来自 `ProviderTemplateIcon.tsx` 的既有 `ProviderTemplateAvatar`，并带 `apiGatewayProviderTemplateAvatarTitle` 无障碍标题（`Created from template {{name}}`）；手动服务商或无法解析的 `template_id` 不渲染头像且不报错。退役映射提示是 `enabled === false` 且 `upstream_model` 不在绑定模板中的映射集合（`isMappingDeprecated`），仅在非空时渲染 `apiGatewayTemplateRetiredMappings` 的数量提示，并在 `apiGatewayTemplateRetiredMappingsTooltip` tooltip 中列出模型名。数量为零时不渲染；由于提示由当前数据派生而非存储事件，重新启用或删除映射会立即使其收缩。`src/i18n.ts` 为三个键提供中英双语，`en_keys.txt` / `zh_keys.txt` 保持配对。

## Alternatives considered

- 禁用模板中不存在的所有映射，包括手动映射：未采纳，因为上一版模板从未携带的映射不在模板契约之内；退役集合仅是「上一版模板减去抓取列表」的差集，因此手动映射、绑定其他模板的服务商与从未匹配模板的模型都保持原样。
- 模型回归模板时自动重新启用退役映射：未采纳，因为同步必须保持单向——`enabled` 表达操作者意图，同步绝不重新启用既有映射；操作者在服务商对话框中显式重新启用映射，在那之前回归的模型保持映射禁用，并因模型已回到模板而不再计入卡片提示。
- 删除退役映射：未采纳，因为删除会丢弃远端模型名、行协议与显示名，移除 provider-scoped 价格行并把模型记入 `ignored_models`；保留的禁用行保住了映射数据、让退役在卡片上仍然可见，操作者还可以有意重新启用它。
- 把卡片提示记录为持久化同步事件：未采纳，因为提示只需描述当前状态；存储的事件会在操作者后续编辑后依然存在，还需要新增持久化字段及其生命周期，而派生提示在映射被重新启用或删除后立即收缩，且无需迁移或已读状态。

## Consequences

- 退役只是对已移除模型的派生映射写入一次 `enabled = false`：配置 schema、命令签名与旧 `api_gateway.json` 的可读性都不变，旧构建按它已能理解的既有取值读取禁用映射。
- 回滚无需数据迁移：回退后的构建绝不重新启用禁用映射，因此回滚后除操作者未触碰过的行外，转发行为与变更前一致。
- 行为迁移：早前构建在模板移除后仍保持启用的映射，其模型早已离开上一版模板，因此不在下一次同步的退役集合内，在操作者显式禁用之前保持当前状态。
- 卡片提示由当前状态派生，因此也会计入任何具有相同「模型不在模板且已禁用」形态的映射；它不是事件日志，重新启用映射仍是操作者的显式动作。
- Supersession（取代评估）：部分取代。[API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 的退役模型事实与其被否决的 auto-disable 备选已就地修正，其称谓、模板绑定、忽略集合生命周期、「仅当未被改动才更新」的合并与星期价格决策继续有效；[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 的同一退役模型事实已就地修正，其目录移除、源优先与手动定价决策继续有效；[Gateway Per-Model Mapping Disable Is an Explicit Exclusion](2026-09-18-gateway-per-mapping-disable.md) 被保留并交叉链接：其显式排除与服务商/映射独立性决策继续有效，而其「按映射自动禁用不在范围内」的被否决备选已不再成立——本记录取代其中的模板同步部分，[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md) 取代其中的上游健康部分。其余 active 记录互不相关。
- `MEMORY.md` 与 `navigation.json` 的 `api-gateway` / `api-gateway-backend` 条目在同一变更中描述退役规则与卡片头像、提示，`navigation.md` 已按权威 JSON 重新生成。
