# Agent Note: OpenCode Go Provider Cards Show Usage

Status: implemented

[English](2026-09-24-opencode-go-provider-usage.md) | 中文

## Problem

OpenCode Go 服务商卡片需要展示账户的滚动、每周与每月用量，同时不能让仅供信息展示的端点成为网关转发的一部分，也不能持久化临时账户快照。同一页面也支持 CommandCode 账户额度，但两个服务商使用不同的端点与识别边界。

## Decision

前端仅为 `isOpencodeGoProvider` 接受的服务商渲染 `ProviderGoUsageBlock`。前端谓词与后端请求守卫都要求 `base_url` 的 host 为 `opencode.ai`（不区分大小写），且 path 含 `/zen/go`（不区分大小写）；OpenCode Zen `/zen/v1` 会被拒绝。请求始终访问 `GET https://opencode.ai/zen/go/v1/usage`，不受匹配到的配置路径影响，并使用固定的源 key——按列表顺序第一个启用的 key，全部禁用时取第一个；密钥池为空时沿用既有 no-key 错误——作为 Bearer token。请求超时为 15 秒，且不跟随重定向。

成功的用量快照按服务商缓存在进程内存中五分钟。固定的源 key 的值或基础 URL 变化后不再复用缓存；强制刷新绕过缓存，失败不缓存。该查询为只读操作，不写入网关配置或用量日志。错误绝不包含 Key。

前端公开类型为 `GoUsageWindow`、`GoUsage` 和 `ProviderGoUsage`；`aiGatewayProviderGoUsage` 调用已注册的 `ai_gateway_provider_go_usage` 命令。CommandCode 既有 `GET https://api.commandcode.ai/alpha/billing/credits` 与 `ProviderQuotaBlock` 行为保持独立且不变。

## Alternatives considered

- 复用 CommandCode 额度检测或计费端点：未采纳，因为 OpenCode Go 使用独立的用量端点、响应与 host/path 边界。
- 匹配所有 `opencode.ai` 服务商：未采纳，因为 Zen `/zen/v1` 服务商不是 OpenCode Go；要求包含 `/zen/go` 才能区分 Go 端点。
- 持久化快照或写入用量日志：未采纳，因为响应是信息展示用的账户快照，不是网关流量用量；仅在内存缓存可保持只读行为。
- 跟随重定向：未采纳，因为命令限定访问固定上游端点，不应把 Bearer 凭据发送给重定向目标。

## Consequences

- 只有符合前后端共用 host 与 path 规则的服务商才显示 Go 用量；无效端点与 Zen `/zen/v1` 会在请求前被拒绝。
- 仅当服务商 ID、固定的源 key 与基础 URL 仍符合条件时才复用五分钟成功快照；显式刷新绕过复用，抓取或解析失败均不缓存。
- 查询不改变服务商配置、网关用量日志或转发状态；错误绝不泄露 API Key。
- 实现与验证路径为 `src-tauri/src/ai_gateway/go_usage.rs`、`src-tauri/src/ai_gateway.rs`、`src-tauri/src/app_runtime/run_app.rs`、`src-tauri/src/ai_gateway/tests/go_usage.rs`、`src/lib/aiGateway.ts`、`src/lib/aiGateway.test.ts`、`src/components/AiGateway/ProviderGoUsageBlock.tsx`、`src/components/AiGateway/ProviderGoUsageBlock.test.tsx`、`src/components/AiGateway/UpstreamProviderList.tsx` 和 `src/i18n.ts`。
- Supersession：部分取代。[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md) 只取代本记录的单一保存 Key 来源，改为固定查询第一个启用的 key（全部禁用时取第一个，空池沿用既有 no-key 错误）；端点、共享 host/path 规则、五分钟缓存、只读边界与区块形状继续有效。独立的 CommandCode 额度展示与其记录保持独立，并由同一密钥池决策修订。
