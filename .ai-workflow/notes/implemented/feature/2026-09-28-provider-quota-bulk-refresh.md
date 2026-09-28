# Agent Note: Upstream Provider List Bulk-Refreshes Quota for the Current Filter Results

Status: implemented

English | [中文](2026-09-28-provider-quota-bulk-refresh.zh.md)

## Problem

The CommandCode quota block and the OpenCode Go usage block each offered only a per-card manual refresh. With several eligible providers on screen, refreshing every visible card required one click per card, and there was no action that refreshed exactly the providers selected by the active status and tag filters. A bulk action also had to preserve the existing boundaries: the mount fetch stays non-forced, an unchanged token, an unrelated re-render or a configuration reload must not add upstream requests, and a provider removed by the filter at click time must not be refreshed retroactively when it reappears.

## Decision

`UpstreamProviderList` renders a bulk button in its header toolbar (`data-testid="ai-gateway-providers-refresh"`) whenever the provider list is non-empty. A click increments a `quotaRefreshToken` state value, and the button is disabled with the `aiGatewayProvidersRefreshQuotaDisabled` explanation when the current filtered result has no eligible provider.

Eligibility is computed from the current filtered result (status filter plus tag filter): a provider qualifies when `isCommandCodeProvider` or `isOpencodeGoProvider` accepts it — the shared CommandCode host `api.commandcode.ai`, or host `opencode.ai` with a path containing `/zen/go` — and its key pool is non-empty, that is `providerKeyPool(provider).length > 0` regardless of each key's enabled or runtime-marked state. Providers with an empty key pool are skipped, providers removed by the filter at click time receive no forced fetch, and a provider that reappears after the click performs only its normal mount fetch. The list uses this eligibility set only to enable or disable the button; the prop reaches every rendered quota-capable block, and each block independently re-checks that its own key pool is non-empty before forcing.

`ProviderQuotaBlock` and `ProviderGoUsageBlock` accept the optional `refreshToken?: number` prop, defaulting to `0`. The mount fetch remains `forceRefresh=false`. When the token differs from the value the block observed at mount or last handled, and the provider's key pool is non-empty, the block calls its loader with `forceRefresh=true` exactly once. An unchanged token, an unrelated re-render or a configuration reload adds no forced fetch, and a block mounted while the token is already non-zero only performs its normal mount fetch, so mounting alone never forces a refresh. The existing per-card manual refresh button is unchanged.

New bilingual keys in `src/i18n.ts`: `aiGatewayProvidersRefreshQuota` (en "Refresh quota", zh "刷新额度"), `aiGatewayProvidersRefreshQuotaAria` (en "Refresh quota for supported providers in the current filter results", zh "刷新当前筛选结果中支持额度监控的服务商") and `aiGatewayProvidersRefreshQuotaDisabled` (en "No providers in the current filter results support quota refresh", zh "当前筛选结果中没有可刷新额度的服务商").

## Alternatives considered

- Poll the quota and usage blocks periodically: declined because it spends upstream requests and cache lifetime on freshness the user can request explicitly instead.
- Refresh all providers regardless of the current filter: declined because it violates the "current filter results" intent and would refresh cards the user cannot see.
- Lift quota and usage state into the parent list: declined because it is a larger refactor with no user-visible gain over a token prop.
- Re-mount the blocks through a key change to force a fetch: declined because it would also force-refresh ineligible and unmounted providers and lose each block's local state.
- Reload the gateway configuration as part of the action: declined because the action concerns quota and usage data only, not configuration data.

## Consequences

- The bulk action reuses the existing block loaders and only adds `forceRefresh=true`; it introduces no new command, endpoint, cache rule or persisted field, and every CommandCode quota and OpenCode Go usage boundary from the earlier records remains in force.
- Only blocks of providers in the current filtered result that are both quota-capable and have a non-empty key pool react: empty-pool and ineligible providers are skipped, providers removed by the filter at click time are not refreshed, and a provider that reappears later performs only its normal mount fetch.
- Token handling is edge-triggered: a changed token forces exactly one refresh while an unchanged token, an unrelated re-render or a configuration reload adds none, and a block mounted with a non-zero token performs only its normal mount fetch; the per-card manual refresh button is unchanged.
- Implementation is in `src/components/AiGateway/UpstreamProviderList.tsx`, `src/components/AiGateway/ProviderQuotaBlock.tsx`, `src/components/AiGateway/ProviderGoUsageBlock.tsx` and `src/i18n.ts`; the bilingual copy keys are `aiGatewayProvidersRefreshQuota`, `aiGatewayProvidersRefreshQuotaAria` and `aiGatewayProvidersRefreshQuotaDisabled`.
- Verification: the RED phase added the new failing tests first; `npx vitest run` over `src/components/AiGateway/UpstreamProviderList.test.tsx`, `src/components/AiGateway/ProviderQuotaBlock.test.tsx`, `src/components/AiGateway/ProviderGoUsageBlock.test.tsx` and `src/i18n.test.ts` passed 123 tests, the full `npm test` passed 67 files and 1215 tests, and `npx tsc -b`, `npm run lint` (0 errors, 445 pre-existing warnings) and `npm run build` exited 0.
- Supersession: no supersession. This record extends [CommandCode Provider Cards Show Account Quota](2026-09-23-commandcode-provider-quota.md) and [OpenCode Go Provider Cards Show Usage](2026-09-24-opencode-go-provider-usage.md); their endpoints, host rules, caches and per-card manual refresh remain in force, and the non-empty key-pool eligibility relies on the pool model defined by [Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md).
