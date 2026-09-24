# Agent Note: CommandCode Provider Cards Show Account Quota

Status: implemented

English | [中文](2026-09-23-commandcode-provider-quota.zh.md)

## Problem

CommandCode provider cards needed to show account credits and reported usage windows without turning an undocumented billing endpoint into a dependency of gateway forwarding. The endpoint, `GET https://api.commandcode.ai/alpha/billing/credits`, is undocumented and may change or disappear. The display also needed to use the correct provider's saved key while keeping endpoint failures isolated from provider availability and all persisted gateway state.

## Decision

`ProviderQuotaBlock` is rendered below the mapping summary only when the frontend `isCommandCodeProvider` predicate accepts the provider. The predicate parses `base_url` and matches the host `api.commandcode.ai` case-insensitively; path and port do not affect the match. The backend repeats the same host rule in `resolve_quota_request` before issuing a request, so frontend detection and the backend guard share the host boundary.

The sole data source is `GET https://api.commandcode.ai/alpha/billing/credits`, called by the `ai_gateway_provider_quota` command with the provider's saved API key in the `Authorization: Bearer` header and a 15-second timeout. The raw key is additionally retained only in process memory by the per-provider five-minute cache, solely to invalidate a snapshot when the key or base URL changes. It is never persisted, logged, included in error text or returned fields. The frontend wrapper is `aiGatewayProviderQuota` in `src/lib/aiGateway.ts`.

Successful snapshots are cached in process memory per provider for five minutes. A changed API key or base URL makes that provider's snapshot ineligible for reuse; an explicit refresh bypasses the cache, and failures are never cached. The quota query is read-only: it writes no configuration, usage-log row or terminal-sync state. Errors render as an inline message within the quota block and do not disable or rewrite the provider or interrupt its card actions or forwarding.

## Alternatives considered

- Treat the alpha endpoint as an officially stable provider contract: declined because it is undocumented and may change or disappear; isolating its response in a card-local query contains that risk without making routing depend on it.
- Detect CommandCode by a template identifier or URL path: declined because providers may be configured independently of a template and paths can vary; the provider endpoint host is the stable available discriminator, with case-insensitive comparison and path/port ignored.
- Disable a provider or surface failure as a card-level blocking error: declined because quota is informational and endpoint failure must not prevent forwarding or provider management; the failure remains local to the quota block.
- Persist quota results or use usage-log/terminal-sync flows: declined because the result is a transient account snapshot and those flows are unrelated; the read-only request and per-provider in-memory cache avoid configuration, log and terminal side effects.

## Consequences

- The provider card has one quota data source and performs no query for providers outside the shared host rule. The command guard independently rejects a blank key or a non-CommandCode host before request creation.
- The five-minute cache is isolated by provider id and is reused only while that provider's API key and base URL are unchanged. Forced refresh bypasses it, and unsuccessful fetches or payload parsing are not cached.
- Endpoint changes or outages affect only the inline quota message; they never disable or rewrite a provider and never affect forwarding. There are no configuration, usage-log or terminal-sync writes.
- The request uses a 15-second timeout. The stored key is sent in the `Authorization: Bearer` header and is additionally retained only in process memory by the per-provider five-minute cache solely to invalidate the snapshot when the key or base URL changes; it is never persisted, logged, included in error text or returned fields.
- Verification is covered by the quota command and parser/cache tests in `src-tauri/src/ai_gateway/tests/quota.rs`, the rendered block tests in `src/components/AiGateway/ProviderQuotaBlock.test.tsx`, and the provider-list tests.
- Supersession: none. The related active gateway feature notes in the authorized implemented-feature scope concern template refresh or terminal synchronization and do not change or replace this quota decision; they remain in force.
