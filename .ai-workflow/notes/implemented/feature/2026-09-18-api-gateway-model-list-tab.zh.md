# Agent Note: API Gateway Model List Tab Reuses the Shared Aggregation

Status: implemented

[English](2026-09-18-api-gateway-model-list-tab.md) | 中文

## Problem

API Gateway 页面此前只能通过 `Aggregated models` 指标卡查看聚合后的本地模型：点击指标卡会打开 `AggregatedModelsDialog`，该弹框一次性列出全部已服务模型、没有搜索，并在操作者下一次点击时关闭。随着映射增多，操作者无法让已服务的模型保持在视野内，也无法快速找到某一个模型。聚合本身已作为 `aggregateModels` 存在，而新界面绝不能成为第二份实现，导致其数量或模型名称与指标卡和弹框不一致。

## Decision

页面新增 `Model list` 页签，由 `src/components/ApiGateway/ModelListPanel.tsx` 中的具名导出 `ModelListPanel({ providers })` 承载。面板用 `aggregateModels(providers)` 为每个聚合后的本地模型派生一行，并用新增的导出函数 `resolveAggregatedModelName(entry)` 解析每行的显示名称。`src/components/ApiGateway/index.tsx` 在 `ApiGatewayTab` 中紧随 `"providers"` 加入 `"models"`，使用 `Boxes` 图标，徽标数取 `aggregateModels(config.providers).length`——与 `Aggregated models` 指标卡同一次调用——并以既有 `hidden` 类模式让面板常驻挂载，使已输入的查询在页签切换后保留。

为让映射显示名可用于该清单，`AggregatedModelProvider` 新增可选 `displayName?: string`。`aggregateModels` 仅在映射来源的 `mapping.display_name` trim 后非空时设置它，对未声明或全空白的名称省略该属性，且绝不设置在服务商的默认模型来源上。`resolveAggregatedModelName` 按聚合顺序返回首个非空 `displayName`；当没有任何来源声明时，返回首个 `isDefault === false` 来源的 `upstreamModel`，没有映射来源时回退到 `providers[0]`，条目没有任何来源时回退 `entry.model`。每行展示三列：模型 ID、解析出的模型名称与上游模型；上游列的每个来源按既有聚合顺序显示为上游服务商名加上游模型 ID，默认模型来源复用既有的 `apiGatewayAggregatedModelDefaultBadge` 徽标。

单个搜索框以 `query.trim().toLowerCase()` 对模型 ID 与解析出的模型名称做子串匹配；空或全空白查询显示全部行，上游服务商名与上游模型 ID 不可搜索。聚合没有任何行时面板渲染空态 `apiGatewayModelListEmpty`，非空查询过滤掉全部行时渲染无匹配态 `apiGatewayModelListNoMatch`。`src/i18n.ts` 为中英文新增八个键：`apiGatewayModelListTab`、`apiGatewayModelListSearch`、`apiGatewayModelListSearchPlaceholder`、`apiGatewayModelListIdColumn`、`apiGatewayModelListNameColumn`、`apiGatewayModelListUpstreamColumn`、`apiGatewayModelListEmpty` 与 `apiGatewayModelListNoMatch`。

## Alternatives considered

- 让面板自行聚合与解析名称，而不复用 `aggregateModels` 与同一个导出的名称辅助函数：未采纳，因为页签徽标、指标卡与弹框必须描述同一份聚合，第二份实现可能与它所概括的界面不一致。
- 同时搜索上游服务商名与上游模型 ID：未采纳，因为该页签索引的是本地模型，只有模型 ID 或名称包含查询时行才应保留；匹配仅存在于上游文本的内容会保留模型本身并不匹配的行，而搜索范围明确排除了这种情况。
- 用页签替换 `AggregatedModelsDialog` 或删除该弹框：未采纳，因为指标卡保留其不切换页签的点击概览；页签与弹框共存且同源，任何一方都不必替换另一方。

## Consequences

- `Model list` 页签、`Aggregated models` 指标卡与 `AggregatedModelsDialog` 都来自 `aggregateModels` 及同一谓词（服务商 `enabled`、映射 `enabled && !auto_disabled`），因此它们的模型集合不可能分叉，并与后端按同一谓词实现的 `GET /v1/models` 与终端同步保持一致；禁用服务商、禁用映射或映射行被自动禁用会无需刷新页面地移除面板中的相应行。
- `src/lib/apiGateway.ts` 公开导出 `resolveAggregatedModelName`，`AggregatedModelProvider` 携带可选 `displayName`；既有聚合条目保持原有形状，因为该属性仅在映射声明非空 `display_name` 时出现，所以弹框与指标卡的预期不变。
- 搜索状态存于常驻挂载的面板中：输入查询、切到其他页签再切回会保留查询与过滤后的行；没有启用模型时面板显示空态，非空查询无任何匹配时显示无匹配态且不渲染数据行。
- 验证：`npx vitest run` 在 `src/components/ApiGateway/ModelListPanel.test.tsx`（9）、`src/components/ApiGateway/ApiGateway.test.tsx`（37）、`src/i18n.test.ts`（16）与 `src/lib/apiGateway.test.ts`（63）共 125/125 通过；`npm run lint` exit 0，0 个 error、422 个既有 warning；`npm run build` exit 0。本次变更为纯前端：没有后端命令、配置 schema、`GET /v1/models` 响应或用量 SQLite 变更。
- `.ai-workflow/index/navigation.json` 在 `api-gateway` feature 中登记 `src/components/ApiGateway/ModelListPanel.tsx` 与 `src/components/ApiGateway/ModelListPanel.test.tsx` 以及 `ModelListPanel`、`resolveAggregatedModelName` 两个公共符号，`navigation.md` 由该 JSON 重新生成；`MEMORY.md` 在同一变更中记录该页签到 API Gateway 标准。
- 替代关系：仅对所记录的谓词构成部分取代；页签与搜索决策继续有效。本记录扩展 [API Gateway Aggregated Models Open in a Dialog With a Shared Count](2026-09-17-api-fusion-aggregated-models-dialog.md)，其弹框与指标卡决策仍然成立，而 [Gateway Per-Model Mapping Disable Is an Explicit Exclusion](../architecture/2026-09-18-gateway-per-mapping-disable.md) 仍是被用户启用映射谓词的现行标准。[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md) 部分取代上文的谓词——服务商谓词仅为 `enabled`，行在被用户禁用或自动禁用时跳过——而本页签、其搜索与共享聚合决策继续有效；三个记录都保留并交叉链接，而非被取代。
