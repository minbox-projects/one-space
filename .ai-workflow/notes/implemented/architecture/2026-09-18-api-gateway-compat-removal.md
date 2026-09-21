# Agent Note: API Gateway Removes Legacy api_fusion File Compatibility

Status: implemented

English | [中文](2026-09-18-api-gateway-compat-removal.zh.md)

## Problem

The previous change added file-level read compatibility for the pre-rename `api_fusion` storage names: `LEGACY_CONFIG_FILE` (`api_fusion.json`) with a storage fallback read, `LEGACY_USAGE_DB_FILE` (`api_fusion_usage.db`) with a usage-log fallback open, and `LEGACY_GATEWAY_MARKER_KEY` (`api_fusion_gateway`) recognized alongside the new `api_gateway_gateway` marker. That compatibility was added without a decision record, and the user has now explicitly required no compatibility: the gateway must use only the new names. Keeping the fallback would preserve an undocumented second source of truth for config, usage history and terminal-sync markers, while deleting the old files is out of scope — they are simply left orphaned.

## Decision

All legacy file-level compatibility is removed; the gateway reads and writes only the new names. `LEGACY_CONFIG_FILE` and its storage fallback read are deleted, so a directory containing only `api_fusion.json` loads as the default empty configuration and the old file is never read, written or deleted. `LEGACY_USAGE_DB_FILE` and its usage-log fallback open are deleted, so usage history stored in `api_fusion_usage.db` stays in the old file while the gateway opens a new `api_gateway_usage.db`. `LEGACY_GATEWAY_MARKER_KEY` and the dual-marker recognition are deleted, so a provider carrying only the old `api_fusion_gateway` marker is treated as unmarked: re-sync never reuses its id and never rewrites it, and instead creates a new independent gateway record with a fresh UUID following the existing stale-ledger rule.

Removed legacy names and their replacements:

| Removed legacy name | Current name |
| --- | --- |
| `api_fusion.json` (`LEGACY_CONFIG_FILE`) | `api_gateway.json` (`CONFIG_FILE`) |
| `api_fusion_usage.db` (`LEGACY_USAGE_DB_FILE`) | `api_gateway_usage.db` (`USAGE_DB_FILE`) |
| `api_fusion_gateway` (`LEGACY_GATEWAY_MARKER_KEY`) | `api_gateway_gateway` (`GATEWAY_MARKER_KEY`) |

Historical-name note: mentions of `api_fusion.json`, `api_fusion_usage.db` and the `api_fusion_gateway` marker in the related records below refer to these removed legacy names; the current facts are the new names above.

## Alternatives considered

- Keep the read-only legacy fallback: declined because the user explicitly required no compatibility, and a silent second read source would keep config, usage and marker state ambiguous.
- Copy or migrate the old files into the new names on first read: declined because migration would write user data the user did not ask to move, and the old files stay orphaned on disk instead.
- Keep recognizing the old `api_fusion_gateway` marker while dropping the file fallbacks: declined because a half-removed compatibility keeps the same dual-source ambiguity for terminal-sync reuse decisions.
- Delete the orphaned old files: declined because removing user data is out of scope; the old files are left untouched and never read.

## Consequences

- A user with only legacy files starts from the default empty configuration; providers, local keys and `terminal_syncs` from `api_fusion.json` are not carried over, and the old file is left untouched on disk.
- Usage history in `api_fusion_usage.db` is not visible to the gateway; new requests are recorded in `api_gateway_usage.db`, and the old database is never opened, written or deleted.
- A provider marked only with `api_fusion_gateway` is treated as an unmarked user record: terminal re-sync creates a new independent `API Gateway` provider with a fresh UUID and never rewrites the old record, per the existing stale-ledger rule.
- Partial supersession: [API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md), [Gateway Per-Model Mapping Disable Is an Explicit Exclusion](2026-09-18-gateway-per-mapping-disable.md), [Gateway Single-Candidate Fast Fail and Standard Error Responses](2026-09-18-gateway-fast-fail-and-standard-errors.md), [New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md) and [API Fusion Terminal Sync Writes an Independent Gateway Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md) are retained and cross-linked. This record replaces only the legacy-compatibility facet — any read-fallback or dual-marker premise — while their decisions (SQLite logging with record-time pricing and retention, per-mapping exclusion, single-candidate fast fail with the standard error envelope, backend-generated key entropy, and the independent terminal-sync gateway provider) still stand; no active note is fully superseded and none is deleted or rewritten.
