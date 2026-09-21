# Agent Note: API Gateway Aggregated Models Open in a Dialog With a Shared Count

Status: implemented

[English](2026-09-17-api-fusion-aggregated-models-dialog.md) | 中文

## Problem

`api-gateway` 页面上的 `Aggregated models` 指标卡显示的数字无法被查看：点击它会把页面切到 `providers` 页签，而该页签列出的是上游服务商而不是聚合后的本地模型。指标卡还自行统计模型数量，在 `RuntimeStatusCard.tsx` 内对每个 `enabled && !auto_disabled` 服务商的 `default_model` 与映射 `local_model` 去重，因此该指标对「本地网关聚合了什么」有自己的定义，任何模型清单都需要第二份实现，而两者可能彼此不一致。

## Decision

聚合后的本地模型从指标卡中移出，改由一份共享聚合承载：指标卡以 `aggregateModels(config.providers).length` 得出数字，而不再自行计数，且每个聚合模型界面都渲染同一份 `aggregateModels` 结果，因此数字不可能与它所描述的清单不一致。本记录最初以 `src/components/ApiGateway/AggregatedModelsDialog.tsx` 交付该清单，并通过指标卡的可选 `onShowModels` prop 与页面的 `isModelsDialogOpen` 状态打开；[API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md) 于 2026-09-21 移除了该弹框界面，`Model list` 页签成为唯一的聚合模型清单界面，指标卡现在通过 `onSelectTab("models")` 切换到它。上文的共享聚合与单一计数决策继续有效。

`src/lib/apiGateway.ts` 以公共函数 `aggregateModels(providers: GatewayUpstreamProvider[]): AggregatedModel[]` 导出该聚合，并导出 `AggregatedModel`（`model`、`providers`）与 `AggregatedModelProvider`（`providerId`、`providerName`、`upstreamModel`、`endpoint`、`isDefault`）接口。服务商在 `enabled` 时参与，且只有其启用、未被自动禁用的映射贡献行：`local_model` 与 `upstream_model` 去空格后都非空的每条此类映射以去空格后的 `upstream_model` 与 `mapping.protocol ?? provider.protocol ?? "chat_completions"` 计入。服务商 `default_model` 是未映射请求的回退目标、不作为聚合行，因此每个来源都带 `isDefault: false`，而该标志与其 `Default` 徽标仍属于接口的一部分。去空格后为空的 `upstream_model` 映射被跳过，因此每个条目的 `upstreamModel` 都非空，这与后端 `src-tauri/src/api_gateway/selection.rs` 把空 `upstream_model` 视为不命中的规则以及既有 `resolveMappingPreview` 一致。条目按本地模型分组，同一模型内的来源先按服务商名、再按上游模型排序，模型按名称排序，因此数量与清单顺序都是确定的。

清单界面为每个聚合后的本地模型列出其每个上游来源（`providerName → upstreamModel`、以路径形式 `/responses` 或 `/chat/completions` 展示的端点标签，以及默认条目的 `Default` 徽标）；取代弹框的 `Model list` 页签保持这一呈现。端点采用 `src/components/ApiGateway/UpstreamProviderDetail.tsx` 既有约定的路径形式，不展示 `chat_completions` / `responses` 原始枚举。指标卡标题继续复用 `apiGatewayAggregatedModels` 键，`src/i18n.ts` 为两种语言保留 `apiGatewayAggregatedModelsDialogDesc` 与 `apiGatewayAggregatedModelDefaultBadge`；弹框移除后，`apiGatewayAggregatedModelsEmpty` 已无代码引用（[API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md)）。

`AggregatedModelProvider` 还携带可选 `displayName`，供后来新增的模型列表页签使用：`aggregateModels` 仅在映射来源的 `mapping.display_name` trim 后非空时设置它，且绝不设置在服务商默认来源上，`src/lib/apiGateway.ts` 导出 `resolveAggregatedModelName` 以解析模型显示名称。[API Gateway Model List Tab Reuses the Shared Aggregation](2026-09-18-api-gateway-model-list-tab.md) 记录了该决策；上文的共享聚合与单一计数决策仍然成立，而本记录引入的弹框界面由 [API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md) 取代。

## Alternatives considered

- 点击后继续切换到 `providers` 页签：未采纳，因为该页签列出的是上游服务商而不是聚合后的本地模型，数字仍然无法查看，也没有把任何本地模型与它背后的上游来源关联起来。
- 让弹框自行分组模型、指标卡保留自己的去重计数：未采纳，因为指标卡数字与弹框会成为同一聚合的两份实现，而「聚合模型数量」绝不能与它所描述的清单不一致。
- 把网关拒绝提供的服务商或映射行也纳入聚合：未采纳，因为本地网关只服务被用户启用的服务商及其被用户启用且未被自动禁用的映射行，列出其余内容等于宣传网关拒绝提供的模型。

## Consequences

- 共享计数是现行事实：指标卡渲染 `aggregateModels(config.providers).length`，不再自行计数。此处记录的点击行为已被替换：[API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md) 让指标卡在点击与 Enter/Space 时选择 `Model list` 页签，而不再打开弹框，且 `onSelectTab` 保持可选，因此未传入该 prop 时指标卡仍按普通卡片渲染。
- 指标卡上的数字与聚合清单中的行来自同一次调用：`aggregateModels` 按本地模型对被用户启用的服务商分组，并把 `local_model` 与 `upstream_model` 去空格后都非空的每条启用、未被自动禁用的映射以端点值 `mapping.protocol ?? provider.protocol ?? "chat_completions"` 计入；去空格后 `upstream_model` 为空的映射不贡献任何条目，因此 `upstreamModel` 始终非空。服务商 `default_model` 不被聚合，因此未映射回退绝不作为行出现，也没有任何条目携带 `isDefault: true`。
- `src/lib/apiGateway.ts` 公开导出 `aggregateModels`、`AggregatedModel` 与 `AggregatedModelProvider`；`src/components/ApiGateway/AggregatedModelsDialog.tsx` 与其 `AggregatedModelsDialog` 导出已由 [API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md) 移除。
- `src/i18n.ts` 为中英文保留 `apiGatewayAggregatedModelsDialogDesc`、`apiGatewayAggregatedModelsEmpty` 与 `apiGatewayAggregatedModelDefaultBadge`，指标卡标题仍复用 `apiGatewayAggregatedModels`；模型列表页签使用 `apiGatewayAggregatedModelsDialogDesc` 与 `apiGatewayAggregatedModelDefaultBadge`，而 `apiGatewayAggregatedModelsEmpty` 在弹框移除后已无引用（[API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md)）。
- `src/components/ApiGateway/ApiGateway.test.tsx` 中的行为测试随弹框移除更新，现断言点击指标卡，或在其上按 Enter / Space，会选择 `Model list` 页签；渲染出的模型行数与指标卡数字相等；端点渲染 `/chat/completions` 而不是原始 `chat_completions` 枚举；以及没有启用服务商的配置展示面板空态（[API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md)）。`src/lib/apiGateway.test.ts` 覆盖 `aggregateModels` 的分组、排序、`local_model` 与 `upstream_model` 的去空格、去空格后 `upstream_model` 为空的映射不贡献条目、自动禁用行被排除而共享模型的健康行与服务商级旧 `auto_disabled` 已置位的服务商仍贡献、仅由自动禁用行提供的模型不产生条目、`default_model` 绝不贡献条目，以及空输入。
- `.ai-workflow/index/navigation.json` 的 `api-gateway` feature 不再登记 `src/components/ApiGateway/AggregatedModelsDialog.tsx` 或 `AggregatedModelsDialog` 符号；本记录登记的共享聚合符号——`aggregateModels`、`AggregatedModel` 与 `AggregatedModelProvider`——仍然登记，`navigation.md` 由该 JSON 重新生成。
- 部分取代：本记录由 [Gateway Per-Model Mapping Disable Is an Explicit Exclusion](../architecture/2026-09-18-gateway-per-mapping-disable.md) 保留并交叉链接，后者收窄了上文的映射选择谓词，使只有被用户启用的映射行参与；[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md) 进一步收窄为「同时未被自动禁用」的行，并让服务商谓词仅为 `enabled`；此处的共享聚合与单一计数决策仍然成立，而本记录引入的弹框界面由 [API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md) 取代。后来移除 `default_model` 条目是记录在 `MEMORY.md` 的现行标准。用量统计与请求日志页面、本地 Key 创建流程与终端同步契约不受影响。
- 模型列表页签的替代关系复核：[API Gateway Model List Tab Reuses the Shared Aggregation](2026-09-18-api-gateway-model-list-tab.md) 以可选 `displayName` 与本页签扩展本记录，而上文已记录的部分取代（[Gateway Per-Model Mapping Disable Is an Explicit Exclusion](../architecture/2026-09-18-gateway-per-mapping-disable.md) 与 [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md)）继续有效；[API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md) 后来移除了弹框并使该页签成为唯一的聚合模型清单界面。用量统计、本地 Key、错误处理与双语 Notes 记录互不相关。
- `MEMORY.md` 承载现行聚合标准——被用户启用的服务商贡献、映射行仅在被用户启用且未被自动禁用时贡献、`default_model` 不被聚合——并记录指标卡切换到「模型列表」页签，因此后来的收窄记录在那里。
