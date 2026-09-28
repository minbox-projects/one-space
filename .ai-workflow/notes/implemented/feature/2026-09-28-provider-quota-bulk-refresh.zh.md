# Agent Note: Upstream Provider List Bulk-Refreshes Quota for the Current Filter Results

Status: implemented

[English](2026-09-28-provider-quota-bulk-refresh.md) | 中文

## Problem

CommandCode 额度区块与 OpenCode Go 用量区块此前都只提供逐卡片手动刷新。屏幕上有多个合格服务商时，刷新每张可见卡片需要逐张点击，也不存在只刷新当前状态筛选与标签筛选所选服务商的动作。批量动作还必须保留既有边界：挂载拉取保持非强制；token 未变化、无关重渲染或配置重载都不得增加上游请求；点击时被筛选移除的服务商在重新出现后不得被追溯强制刷新。

## Decision

服务商列表非空时，`UpstreamProviderList` 在头部工具栏渲染批量按钮（`data-testid="ai-gateway-providers-refresh"`）。点击会递增 `quotaRefreshToken` 状态值；当前筛选结果中没有合格服务商时，按钮禁用并显示 `aiGatewayProvidersRefreshQuotaDisabled` 说明。

合格性由当前筛选结果（状态筛选加标签筛选）计算：当 `isCommandCodeProvider` 或 `isOpencodeGoProvider` 接受该服务商——共享的 CommandCode 主机 `api.commandcode.ai`，或 host 为 `opencode.ai` 且 path 含 `/zen/go`——且其密钥池非空，即 `providerKeyPool(provider).length > 0`（与每个 key 的 enabled 或运行时标记状态无关）时合格。空密钥池服务商跳过；点击时被筛选移除的服务商不发起强制刷新；点击后重新出现的服务商只执行普通挂载拉取。列表只把该合格集合用于启用或禁用按钮；属性会传给每个已渲染的额度能力区块，各区块在强制刷新前独立复查自身密钥池非空。

`ProviderQuotaBlock` 与 `ProviderGoUsageBlock` 接受可选 `refreshToken?: number` 属性，默认值为 `0`。挂载拉取保持 `forceRefresh=false`。当 token 与该区块在挂载时或上次处理时观察到的值不同，且服务商密钥池非空时，区块以 `forceRefresh=true` 恰好调用一次其加载器。token 不变、无关重渲染或配置重载都不追加强制刷新；挂载时 token 已非零的区块只执行普通挂载拉取，因此仅挂载绝不强制刷新。既有逐卡片手动刷新按钮保持不变。

`src/i18n.ts` 新增双语键：`aiGatewayProvidersRefreshQuota`（en "Refresh quota"，zh "刷新额度"）、`aiGatewayProvidersRefreshQuotaAria`（en "Refresh quota for supported providers in the current filter results"，zh "刷新当前筛选结果中支持额度监控的服务商"）与 `aiGatewayProvidersRefreshQuotaDisabled`（en "No providers in the current filter results support quota refresh"，zh "当前筛选结果中没有可刷新额度的服务商"）。

## Alternatives considered

- 定期轮询额度与用量区块：未采纳，因为这会为用户可以显式请求的新鲜度消耗上游请求与缓存寿命。
- 无视当前筛选刷新全部服务商：未采纳，因为违背“当前筛选结果”意图，会刷新用户不可见的卡片。
- 把额度与用量状态下沉到父列表：未采纳，因为相比 token 属性这是没有用户可见收益的更大重构。
- 通过 key 变化重新挂载区块以强制拉取：未采纳，因为这会同时强制刷新不合格与未挂载的服务商，并丢失区块本地状态。
- 在动作中重载网关配置：未采纳，因为该动作只涉及额度与用量数据，不涉及配置数据。

## Consequences

- 批量动作复用既有区块加载器，只追加 `forceRefresh=true`；不引入新命令、端点、缓存规则或持久化字段，此前记录中的全部 CommandCode 额度与 OpenCode Go 用量边界继续有效。
- 只有当前筛选结果中同时具备额度能力且密钥池非空的服务商区块会响应：空密钥池与不合格服务商跳过，点击时被筛选移除的服务商不刷新，之后重新出现的服务商只执行普通挂载拉取。
- token 处理是边沿触发的：token 变化恰好强制刷新一次，token 不变、无关重渲染或配置重载都不追加；挂载时 token 已非零的区块只执行普通挂载拉取；逐卡片手动刷新按钮保持不变。
- 实现位于 `src/components/AiGateway/UpstreamProviderList.tsx`、`src/components/AiGateway/ProviderQuotaBlock.tsx`、`src/components/AiGateway/ProviderGoUsageBlock.tsx` 与 `src/i18n.ts`；双语文案键为 `aiGatewayProvidersRefreshQuota`、`aiGatewayProvidersRefreshQuotaAria` 与 `aiGatewayProvidersRefreshQuotaDisabled`。
- 验证：RED 阶段先加入新的失败测试；`npx vitest run` 运行 `src/components/AiGateway/UpstreamProviderList.test.tsx`、`src/components/AiGateway/ProviderQuotaBlock.test.tsx`、`src/components/AiGateway/ProviderGoUsageBlock.test.tsx` 与 `src/i18n.test.ts` 通过 123 个测试，完整 `npm test` 通过 67 个文件、1215 个测试，且 `npx tsc -b`、`npm run lint`（0 错误，445 个既有警告）与 `npm run build` 均以 0 退出。
- Supersession：无取代。本记录扩展 [CommandCode Provider Cards Show Account Quota](2026-09-23-commandcode-provider-quota.md) 与 [OpenCode Go Provider Cards Show Usage](2026-09-24-opencode-go-provider-usage.md)；两者的端点、主机规则、缓存与逐卡片手动刷新继续有效，非空密钥池合格性依赖 [Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md) 定义的密钥池模型。
