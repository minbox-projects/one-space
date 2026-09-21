# Agent Note: API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog

Status: implemented

[English](2026-09-21-gateway-aggregated-models-open-model-list-tab.md) | 中文

## Problem

`api-gateway` 页面上的 `Aggregated models` 指标卡会打开 `src/components/ApiGateway/AggregatedModelsDialog.tsx`，而它与 `Model list` 页签基于同一份 `aggregateModels` 来源以及同一套服务商与映射谓词，构成第二个界面。该弹框一次性列出全部已服务模型、没有搜索，并在操作者下一次点击时关闭，因此重复了只差一次页签点击即可到达的界面。

## Decision

`src/components/ApiGateway/AggregatedModelsDialog.tsx` 已连同页面的 `isModelsDialogOpen` 状态与指标卡的可选 `onShowModels` prop 一起移除，`Model list` 页签现在是唯一的聚合模型清单界面。`src/components/ApiGateway/RuntimeStatusCard.tsx` 在指标卡的点击处理与 Enter/Space 键处理中都调用既有的可选 `onSelectTab?.("models")`，因此点击指标卡会切换到 `Model list` 页签而不再打开弹框。

共享计数保证不变：指标卡仍以 `aggregateModels(config.providers).length` 得出数字，页签徽标使用同一次调用，`src/components/ApiGateway/ModelListPanel.tsx` 渲染同一份 `aggregateModels` 结果，因此数字与清单不可能彼此不一致。

弹框时期的文案键保留在其仍被使用之处：`ModelListPanel.tsx` 为空态描述渲染 `apiGatewayAggregatedModelsDialogDesc`、为默认模型来源徽标渲染 `apiGatewayAggregatedModelDefaultBadge`，因此旧键名继续保留。为已移除弹框新增的 `apiGatewayAggregatedModelsEmpty` 在移除后已无代码引用，并以中英双语保留在 `src/i18n.ts`。

## Alternatives considered

- 保留弹框与页签共存：未采纳，因为两者渲染同一份 `aggregateModels` 结果，而页签已提供搜索与持续可见性，弹框只是为同一份清单增加了第二种呈现。
- 保留弹框、只改变指标卡的跳转目标：未采纳，原因同样是重复，且要避免为同一份聚合维护两条打开/关闭路径。
- [模型列表页签记录](../feature/2026-09-18-api-gateway-model-list-tab.md)当时未采纳「替换或移除弹框」，因为指标卡保留其不切换页签的点击概览；后来关于搜索与持续可见性的决策使这种共存变得冗余，因此本记录采纳了当时搁置的移除。

## Consequences

- 点击 `Aggregated models` 指标卡，或在其获得焦点时按 Enter / Space，会选择 `Model list` 页签且不打开任何弹框；`onSelectTab` 保持可选，因此未传入该 prop 时指标卡仍按普通卡片渲染。
- 共享计数与谓词不变：指标卡数字与页签徽标都来自 `aggregateModels(config.providers).length`，且 `aggregateModels` 仍只计入 `enabled` 为真的服务商以及 `enabled && !auto_disabled` 的映射。
- 后端行为、`GET /v1/models`、终端同步、session affinity、用量统计与请求日志不受影响，且没有任何导航 id、启动器入口或页签顺序变化。
- `apiGatewayAggregatedModelsDialogDesc` 与 `apiGatewayAggregatedModelDefaultBadge` 仍以弹框时期的名称由模型列表面板使用，`apiGatewayAggregatedModelsEmpty` 现在已无引用但仍保留在 `src/i18n.ts`。
- `src/components/ApiGateway/ApiGateway.test.tsx` 中的行为测试断言点击指标卡，或在其上按 Enter / Space，会选择 `Model list` 页签并渲染面板；渲染出的模型行数与指标卡数字相等；端点渲染路径形式 `/chat/completions`；以及没有启用服务商的配置展示面板空态。
- 部分取代：[API Gateway Aggregated Models Open in a Dialog With a Shared Count](../feature/2026-09-17-api-fusion-aggregated-models-dialog.md) 被保留并交叉链接；其共享聚合与单一计数决策仍然成立，而其弹框界面决策由本记录取代。[API Gateway Model List Tab Reuses the Shared Aggregation](../feature/2026-09-18-api-gateway-model-list-tab.md) 被保留并交叉链接，作为「页签成为唯一聚合模型清单界面」的记录。[Gateway Per-Model Mapping Disable Is an Explicit Exclusion](../architecture/2026-09-18-gateway-per-mapping-disable.md) 与 [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](../architecture/2026-09-20-gateway-per-model-auto-disable.md) 记录该选择谓词，不受影响。
