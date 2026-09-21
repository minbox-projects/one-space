# Agent Note: API Gateway Deletes Legacy api_fusion Files

Status: implemented

English | [中文](2026-09-18-api-gateway-legacy-cleanup.zh.md)

## Problem

[API Gateway Removes Legacy api_fusion File Compatibility](2026-09-18-api-gateway-compat-removal.md) removed the legacy read fallback and recorded that the old files are never deleted: deleting orphaned user data was out of scope, so `api_fusion.json` and `api_fusion_usage.db` under `~/.config/onespace` were left untouched. The user has now explicitly required those two old files to be deleted. The retained premise contradicts the delivered fact, and both reviews require the same change to add a new record.

## Decision

`api_gateway` adds `cleanup_legacy_files()`, which deletes only the two legacy paths and never reads them. The current files are never touched, and every deletion is best-effort with errors ignored.

| Deleted legacy path | Current file left untouched |
| --- | --- |
| `api_fusion.json` | `api_gateway.json` |
| `api_fusion_usage.db` | `api_gateway_usage.db` |

The cleanup runs at four points: autostart calls `cleanup_legacy_files()` unconditionally, and it is also called after a config write, after the usage store opens, and after a config read loads the new configuration.

- `autostart` calls `cleanup_legacy_files()` unconditionally.
- After a config write, `cleanup_legacy_files()` runs again.
- After the usage store opens, `cleanup_legacy_files()` runs again.
- After a config read loads the new configuration, `cleanup_legacy_files()` runs again.

The only-old-files case is covered by the unconditional autostart call.

## Alternatives considered

- Keep the old files orphaned and untouched: declined because the user explicitly required synchronous deletion, so the prior retention premise no longer stands.
- Copy or migrate the old files into the new names before deleting: declined because the user asked for deletion rather than migration, and reading the old files would reintroduce the removed second source of truth.
- Delete more broadly, including the current files: declined because the current files must never be touched; only the two legacy paths are removed.

## Consequences

- Once deleted, the old configuration and usage history are irreversibly lost; the old files are only deleted, never read or migrated.
- The old marker is not part of this deletion: recognition of the legacy marker was already removed in the prior round, so there is no marker file to delete here.
- The current `api_gateway.json` and `api_gateway_usage.db` are never touched by the cleanup.
- Partial supersession: [API Gateway Removes Legacy api_fusion File Compatibility](2026-09-18-api-gateway-compat-removal.md) is retained and cross-linked. This record replaces only its no-deletion premise; its unique rationale still stands, including the removal of the legacy read fallback, the refusal of silent migration, and the treatment of a record carrying only the old marker as unmarked.
