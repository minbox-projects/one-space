# Agent Note: OpenCode Session Storage Compatibility

Status: implemented

English | [中文](2026-09-20-opencode-session-storage-compatibility.zh.md)

## Problem

OpenCode moved session storage from legacy JSON to SQLite in v1.2.0, and the formal v2 format renamed the SQLite tables. Migration preserves session IDs and can leave the older stores in place, so reading only one format loses history while independently aggregating every format counts the same logical session more than once.

## Decision

- Parse OpenCode CLI versions as full SemVer: extract `1.x` and `2.x` values including prerelease and build metadata, and compare installations by SemVer precedence — numeric prerelease identifiers before alphanumeric, stable above prerelease, build metadata preserved in the reported text but ignored for precedence.
- Read history and usage from legacy JSON, SQLite v1 tables `session` and `message`, and SQLite v2 tables `session_v2` and `session_message`.
- Normalize session identity by its trimmed ID. When the same ID exists in multiple stores, select exactly one source with fixed priority v2 > v1 > JSON; lower-priority sources contribute only session IDs absent from higher-priority sources.
- For usage, retain every message from the selected source and derive tokens from message-level data. This supports early v1 records and matches v2 without accumulating the same session across sources.
- Keep the Tauri schema, frontend, and resolver unchanged. This decision does not add `OPENCODE_DB` or channel-database discovery.

## Alternatives considered

- Read only the storage format implied by the currently detected CLI version. Declined because migration can preserve older stores and sessions, so installed version alone does not identify all readable history.
- Add tokens from every source. Declined because preserved session IDs represent the same logical session and would double-count usage.
- Support SQLite only. Declined because legacy JSON installations and sessions not represented in SQLite would disappear.

## Consequences

- History and usage remain available across the JSON-to-SQLite and v1-to-v2 transitions.
- Source selection is deterministic per trimmed session ID, while unique sessions from older sources remain visible.
- Usage includes all messages from the selected source but never repeats a session solely because migration left another copy behind.
- Version probing reports the exact printed version, including prerelease and build metadata, and update checking tolerates build metadata without changing comparison outcomes.
- Compatibility stays inside OpenCode CLI probing and session storage readers; public Tauri data shape, frontend behavior, resolver behavior, and database-discovery scope do not expand.
