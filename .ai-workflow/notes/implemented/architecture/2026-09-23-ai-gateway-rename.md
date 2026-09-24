# Agent Note: Rename API Gateway to AI Gateway

Status: implemented

English | [中文](2026-09-23-ai-gateway-rename.zh.md)

## Problem

The gateway was named `API Gateway` / `API 网关` even though it is an AI-model relay: it lives in the sidebar's `AI 能力` group between `AI 终端服务商` and `AI 用量统计`, and its settings live in the `ai-gateway` section. The generic "API gateway" name reads as general API management rather than AI traffic, while `api-*` identifiers coexisted with the product's `ai-*` vocabulary across components, commands, events, navigation ids, file names and the terminal-sync provider record, so one feature had to be carried under two names in code, documentation and agent context.

## Decision

Every internal identifier and display name now uses `AI Gateway` / `AI 网关`:

- Front end: `src/components/AiGateway/` and `src/lib/aiGateway.ts`; `AiGateway*` component names, `aiGateway*` command wrappers and `AI_GATEWAY_*` constants; events `ai-gateway-status-update` and `ai-gateway-config-update`; navigation ids `ai-gateway` and `ai-gateway-backend`; settings section `ai-gateway`.
- Back end: `src-tauri/src/ai_gateway.rs` with its submodules (`types_config`, `storage`, `selection`, `runtime_http`, `forwarding`, `commands`, `usage_log`, `templates`, `migration`) and the `ai_gateway_*` Tauri commands.
- Persistence and terminal integration: `ai_gateway.json`, `ai_gateway_usage.db`, the `ai_gateway_gateway` terminal-sync marker, and the synced provider record name `AI Gateway`.
- `migrate_legacy_files()` under `get_app_dir()` renames the pre-rename config file and usage SQLite main database to the current names: each is renamed only when its legacy file exists and its current file is absent; the usage database's `-wal`/`-shm` sidecars follow only when this call successfully renamed the legacy main database, and each of them is renamed only when its own current target is absent; when the current main database already exists or the legacy main database is missing, legacy sidecars stay untouched and a stray sidecar is never attached to the existing database. The operation never overwrites, is idempotent and silently ignores every error. Accepted boundary: if the process is interrupted between the main-database rename and the sidecar renames, the legacy sidecars are not migrated afterwards and uncheckpointed transactions in the old WAL are not recovered. Its call sites are config read, config write, `ai_gateway_autostart` and `UsageLogStore::default_store`.
- `has_legacy_gateway_marker()` recognizes the pre-rename terminal marker at the top level or under `tool_config`; new writes always use `ai_gateway_gateway`, and the next sync reuses the legacy record's id and upgrades it in place instead of creating a duplicate.
- `src-tauri/src/ai_gateway/migration.rs` is a permanent, version-gated module and the only place allowed to name the pre-rename identifiers; its former one-release deletion contract is replaced by the configuration schema version and the usage-database version, so it can upgrade any older file at any later release instead of depending on an upgrade window a release cannot observe ([Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)).
- The former `cleanup_legacy_files()` and the API Fusion era legacy constants are deleted: `api_fusion.json` and `api_fusion_usage.db` are no longer deleted automatically and are never read.

## Alternatives considered

- Keep `API Gateway` and rename only user-visible text: declined because the identifiers would still disagree with the `AI 能力` grouping and the `ai-gateway` settings section, and agents would keep switching between two vocabularies.
- Rename without a migration module: declined because existing installations would start from an empty configuration and stop seeing their usage history, and the old files could no longer be migrated once new files are written.
- Keep dual-name compatibility for files and markers: declined because it preserves the undocumented second source of truth that the earlier legacy-compatibility removal deliberately eliminated.
- Rename the historical decision records as well: declined because the referenced notes are active `implemented/` records, and the notes README keeps delivered decisions as history instead of rewriting them retroactively; a changed decision or rationale is recorded in a new note, which is what this record does, and the old notes keep their file names as historical slugs so existing cross-links stay traceable.

## Consequences

- Existing installations keep their configuration and usage history through the in-place rename; each file is renamed only when its current name is absent, so a partially migrated directory converges with the current file winning and the old file left untouched.
- Accepted boundary: if the process is interrupted between the main-database rename and the sidecar renames, the legacy `-wal`/`-shm` sidecars are not migrated afterwards, so uncheckpointed transactions in the old WAL are not recovered; the config and main-database data are still retained.
- A provider carrying only the legacy marker still reads as a gateway record; after the next sync upgrades it in place, the record uses the current marker only.
- `api_fusion.json` and `api_fusion_usage.db` stay orphaned on disk: the earlier automatic deletion is deliberately withdrawn, and neither file is read, so this rename deletes no user data.
- `migration.rs` stays the single exemption to the all-`ai_gateway` naming rule, now permanently gated by the configuration schema version and the usage-database version; deleting it would break every installation still holding an older file, so the upgrade path is carried instead of assumed.
- Partial supersession: [API Gateway Deletes Legacy api_fusion Files](2026-09-18-api-gateway-legacy-cleanup.md) is retained and cross-linked; this record replaces only its deletion behavior and cleanup call points, while its premise that the legacy files are never read still stands. [API Gateway Removes Legacy api_fusion File Compatibility](2026-09-18-api-gateway-compat-removal.md) is retained and cross-linked; its removal of read compatibility still stands, and only the current-name column of its naming table is superseded. The gateway's technical records ([API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md), [API Gateway Smooth Weighted Round Robin Routes Requests and Fallback Candidates](2026-09-20-api-gateway-weighted-routing.md), [API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md)) remain valid with renamed identifiers; no active note is fully superseded and no record is deleted or rewritten. Partial supersession: [Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) replaces only this record's one-release deletion contract for `migration.rs`; the naming and file-rename decisions here stand.
