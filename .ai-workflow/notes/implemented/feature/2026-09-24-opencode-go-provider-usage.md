# Agent Note: OpenCode Go Provider Cards Show Usage

Status: implemented

English | [中文](2026-09-24-opencode-go-provider-usage.zh.md)

## Problem

OpenCode Go provider cards need to show the account's rolling, weekly and monthly usage without making an informational endpoint part of gateway forwarding or persisting a transient account snapshot. The same page also supports CommandCode account credits, but the two providers have different endpoints and identification boundaries.

## Decision

The frontend renders `ProviderGoUsageBlock` when the provider's effective quota strategy is `opencode_go` — resolved from its bound template with the generic fallback — never from its URL. The frontend resolves the policy with `resolveEffectiveTemplatePolicy` and the backend request guard `resolve_go_usage_request` rejects any provider whose effective strategy is not `opencode_go` before reading a key or the base URL, so an unbound Go-looking provider stays generic ([Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md)). The request always targets `GET https://opencode.ai/zen/go/v1/usage`, independent of the configured matching path, and sends the provider's pinned source key — the first enabled unmarked key in list order, then the first enabled key when every enabled key is marked, then the first stored key when no key is enabled; an empty pool keeps the existing no-key error — as a Bearer token. The request has a 15-second timeout and does not follow redirects.

Successful usage snapshots are cached per provider in process memory for five minutes. The cache is not reused if the pinned source key's value or the base URL changes; forced refresh bypasses it, and failures are not cached. The query is read-only and writes neither gateway configuration nor usage logs. The key is never included in errors.

The public frontend types are `GoUsageWindow`, `GoUsage` and `ProviderGoUsage`; `aiGatewayProviderGoUsage` invokes the registered `ai_gateway_provider_go_usage` command. CommandCode's existing `GET https://api.commandcode.ai/alpha/billing/credits` and `ProviderQuotaBlock` behavior remain separate and unchanged.

## Alternatives considered

- Reuse CommandCode quota detection or its billing endpoint: declined because OpenCode Go has a separate usage endpoint, response and host/path boundary.
- Match all `opencode.ai` providers by URL: declined at the time because Zen `/zen/v1` providers were not OpenCode Go, so a path rule distinguished them. [Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md) later replaced URL matching with the provider's effective `opencode_go` template policy, so an unbound Go-looking provider no longer qualifies.
- Persist snapshots or write a usage-log entry: declined because the response is an informational account snapshot, not gateway traffic usage; memory-only caching preserves read-only behavior.
- Follow redirects: declined because the command is scoped to a fixed upstream endpoint and should not transfer the Bearer credential to a redirected target.

## Consequences

- Go usage is shown only for a provider whose effective policy is `opencode_go`; an unbound, broken or differently bound provider is not queried, and a provider bound to the OpenCode Go template uses the Go base URL.
- Successful snapshots are reused for five minutes only while provider ID, the pinned source key and base URL remain eligible; explicit refresh bypasses reuse, and failed fetch or parsing results are not cached.
- The query does not change provider configuration, gateway usage logs or forwarding state; errors never reveal the API key.
- Implementation and verification paths are `src-tauri/src/ai_gateway/go_usage.rs`, `src-tauri/src/ai_gateway.rs`, `src-tauri/src/app_runtime/run_app.rs`, `src-tauri/src/ai_gateway/tests/go_usage.rs`, `src/lib/aiGateway.ts`, `src/lib/aiGateway.test.ts`, `src/components/AiGateway/ProviderGoUsageBlock.tsx`, `src/components/AiGateway/ProviderGoUsageBlock.test.tsx`, `src/components/AiGateway/UpstreamProviderList.tsx` and `src/i18n.ts`.
- Supersession: partial supersession. [Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md) replaced only this record's single saved-key source with the pinned first-enabled-key source, and [Gateway Key Health Rotates Bare 429s, Recovers Quota Marks after a TTL, Retries a Single Candidate and Pins the Query Source Key](../architecture/2026-09-25-gateway-key-health-lifecycle.md) refines that source to the first enabled unmarked key, then the first enabled key, then the first stored key (the existing no-key error when the pool is empty); [Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md) replaces this record's URL host/path dispatch with the provider's effective `opencode_go` quota policy, while the endpoint, the pinned source key, the five-minute cache, the read-only boundary and the block shape remain in force. The independent CommandCode quota display and its own note remain separate and are revised by the same key-pool, key-health and policy decisions.
