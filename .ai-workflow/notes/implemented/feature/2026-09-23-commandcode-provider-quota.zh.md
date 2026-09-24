# Agent Note: CommandCode Provider Cards Show Account Quota

Status: implemented

[English](2026-09-23-commandcode-provider-quota.md) | 中文

## Problem

CommandCode 服务商卡片需要展示账户额度与已报告的使用窗口，同时不能让网关转发依赖一个未公开的计费端点。端点 `GET https://api.commandcode.ai/alpha/billing/credits` 没有文档，可能变更或消失。额度展示还必须使用对应服务商保存的 Key，并将端点故障与服务商可用性及所有持久化网关状态隔离。

## Decision

仅当前端谓词 `isCommandCodeProvider` 接受该服务商时，才在映射摘要下方渲染 `ProviderQuotaBlock`。该谓词解析 `base_url` 并以不区分大小写的方式匹配主机名 `api.commandcode.ai`；路径和端口不影响匹配。后端在发起请求前通过 `resolve_quota_request` 再次执行同一主机名规则，因此前后端检测与后端守卫共享相同的主机边界。

唯一数据源是 `GET https://api.commandcode.ai/alpha/billing/credits`。`ai_gateway_provider_quota` 命令使用该服务商保存的 API Key，在 `Authorization: Bearer` 请求头中发送，并设置 15 秒超时。原始 Key 还仅由每服务商五分钟缓存保留在进程内存中，唯一用途是在 Key 或基础 URL 变化时使快照失效。该 Key 绝不持久化、记录日志、出现在错误文本或返回字段中。前端封装 `aiGatewayProviderQuota` 位于 `src/lib/aiGateway.ts`。

成功快照按服务商缓存在进程内存中五分钟。API Key 或基础 URL 变更后，该服务商的快照不再符合复用条件；显式刷新会绕过缓存，失败永不缓存。额度查询为只读操作：不写配置、用量日志行或终端同步状态。错误以内联消息显示在额度区块内，不会禁用或重写服务商，也不会中断卡片操作或转发。

## Alternatives considered

- 将 alpha 端点视为官方稳定的服务商契约：未采纳，因为该端点没有文档且可能变更或消失；把响应隔离在卡片内的查询中可以限制风险，而不让路由依赖它。
- 按模板标识或 URL 路径识别 CommandCode：未采纳，因为服务商可以独立配置且未必来自模板，路径也可能变化；现有最稳定的区分依据是服务商端点主机名，比较时忽略大小写、路径与端口。
- 禁用服务商或将故障显示为阻断整张卡片的错误：未采纳，因为额度仅供参考，端点故障不得阻止转发或服务商管理；失败只保留在额度区块内。
- 持久化额度结果，或复用用量日志/终端同步流程：未采纳，因为结果是临时账户快照且与这些流程无关；只读请求与按服务商内存缓存避免配置、日志及终端副作用。

## Consequences

- 服务商卡片只有一个额度数据源；不符合共享主机规则的服务商不会发起查询。命令守卫还会在创建请求前独立拒绝空 Key 或非 CommandCode 主机。
- 五分钟缓存按服务商 ID 隔离，只有该服务商 API Key 与基础 URL 未变时才复用。强制刷新绕过缓存，失败的抓取或载荷解析结果不会缓存。
- 端点变化或故障只影响内联额度消息；不会禁用或重写服务商，也不会影响转发。不写配置、用量日志或终端同步状态。
- 请求使用 15 秒超时。保存的 Key 在 `Authorization: Bearer` 请求头中发送，并仅由每服务商五分钟缓存额外保留在进程内存中，唯一用途是在 Key 或基础 URL 变化时使快照失效；它绝不持久化、记录日志、出现在错误文本或返回字段中。
- 验证由 `src-tauri/src/ai_gateway/tests/quota.rs` 中的额度命令、解析与缓存测试，`src/components/AiGateway/ProviderQuotaBlock.test.tsx` 中的渲染测试，以及服务商列表测试覆盖。
- Supersession：无。授权的 implemented-feature 范围内相关的网关活动记录涉及模板刷新或终端同步，不会改变或取代本额度决策；这些记录仍然有效。
