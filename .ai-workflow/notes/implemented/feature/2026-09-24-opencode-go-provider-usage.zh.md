# Agent Note: OpenCode Go Provider Cards Show Usage

Status: implemented

[English](2026-09-24-opencode-go-provider-usage.md) | 中文

## Problem

OpenCode Go 服务商卡片需要展示账户的滚动、每周与每月用量，同时不能让仅供信息展示的端点成为网关转发的一部分，也不能持久化临时账户快照。同一页面也支持 CommandCode 账户额度，但两个服务商使用不同的端点与识别边界。

## Decision

当服务商的有效配额策略为 `opencode_go`——由绑定模板经 generic 回退解析得到，绝不来自其 URL——时，前端才渲染 `ProviderGoUsageBlock`。前端用 `resolveEffectiveTemplatePolicy` 解析该策略，后端请求守卫 `resolve_go_usage_request` 在读取任何 key 或 base URL 之前拒绝有效策略不是 `opencode_go` 的服务商，因此未绑定但看起来像 Go 的服务商保持 generic（[Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md)）。请求始终访问 `GET https://opencode.ai/zen/go/v1/usage`，并使用固定的源 key——按列表顺序第一个启用且未被标记的 key，全部启用 key 都被标记时取第一个启用的 key，没有任何启用 key 时取第一个存储的 key；密钥池为空时沿用既有 no-key 错误——作为 Bearer token。请求超时为 15 秒，且不跟随重定向。

成功的用量快照按服务商缓存在进程内存中五分钟。固定的源 key 的值或基础 URL 变化后不再复用缓存；强制刷新绕过缓存，失败不缓存。该查询为只读操作，不写入网关配置或用量日志。错误绝不包含 Key。

前端公开类型为 `GoUsageWindow`、`GoUsage` 和 `ProviderGoUsage`；`aiGatewayProviderGoUsage` 调用已注册的 `ai_gateway_provider_go_usage` 命令。CommandCode 既有 `GET https://api.commandcode.ai/alpha/billing/credits` 与 `ProviderQuotaBlock` 行为保持独立且不变。

## Alternatives considered

- 复用 CommandCode 额度检测或计费端点：未采纳，因为 OpenCode Go 使用独立的用量端点、响应与 host/path 边界。
- 按 URL 匹配所有 `opencode.ai` 服务商：当时未采纳，因为 Zen `/zen/v1` 服务商不是 OpenCode Go，需用路径规则区分。[Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md) 后来用服务商的有效 `opencode_go` 模板策略取代 URL 匹配，因此未绑定但看起来像 Go 的服务商不再合格。
- 持久化快照或写入用量日志：未采纳，因为响应是信息展示用的账户快照，不是网关流量用量；仅在内存缓存可保持只读行为。
- 跟随重定向：未采纳，因为命令限定访问固定上游端点，不应把 Bearer 凭据发送给重定向目标。

## Consequences

- 只有有效策略为 `opencode_go` 的服务商才显示 Go 用量；未绑定、损坏绑定或绑定到其他模板的服务商不会被查询，绑定到 OpenCode Go 模板的服务商使用 Go base URL。
- 仅当服务商 ID、固定的源 key 与基础 URL 仍符合条件时才复用五分钟成功快照；显式刷新绕过复用，抓取或解析失败均不缓存。
- 查询不改变服务商配置、网关用量日志或转发状态；错误绝不泄露 API Key。
- 实现与验证路径为 `src-tauri/src/ai_gateway/go_usage.rs`、`src-tauri/src/ai_gateway.rs`、`src-tauri/src/app_runtime/run_app.rs`、`src-tauri/src/ai_gateway/tests/go_usage.rs`、`src/lib/aiGateway.ts`、`src/lib/aiGateway.test.ts`、`src/components/AiGateway/ProviderGoUsageBlock.tsx`、`src/components/AiGateway/ProviderGoUsageBlock.test.tsx`、`src/components/AiGateway/UpstreamProviderList.tsx` 和 `src/i18n.ts`。
- Supersession：部分取代。[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md) 只取代本记录的单一保存 Key 来源，改为固定查询第一个启用的 key；[Gateway Key Health Rotates Bare 429s, Recovers Quota Marks after a TTL, Retries a Single Candidate and Pins the Query Source Key](../architecture/2026-09-25-gateway-key-health-lifecycle.md) 把该来源细化为第一个启用且未标记的 key、其次第一个启用的 key、最后第一个存储的 key（空池沿用既有 no-key 错误）；[Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md) 把本记录基于 URL host/path 的派发替换为服务商的有效 `opencode_go` 配额策略，而端点、固定源 key、五分钟缓存、只读边界与区块形状继续有效。独立的 CommandCode 额度展示与其记录保持独立，并由同一密钥池、密钥健康与策略决策修订。
