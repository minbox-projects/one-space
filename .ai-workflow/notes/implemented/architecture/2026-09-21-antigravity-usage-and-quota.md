# Agent Note: Antigravity Usage and Quota: Legacy Token Scan, Transcript Call Counts and Quota Command

Status: implemented

English | [中文](2026-09-21-antigravity-usage-and-quota.zh.md)

## Problem

The baseline held that Antigravity persists no token usage on disk, so `src-tauri/src/ai_sessions/usage.rs` carried a never-parse comment and a guard test enforced a permanent `unavailable`. That decision is now wrong in both directions: the legacy `~/.gemini/tmp` files do contain per-message token data and users have accumulated history there, while the new brain transcripts (`transcript_full.jsonl`) carry conversation rows but no token usage at all. The dual-axis review blocker N-F1 and the major finding N-F3 require this reversal to be recorded: which source yields token records, which source yields only call counts, how `empty` is decided, and where quota comes from.

## Decision

- Legacy `~/.gemini/tmp` is scanned recursively for `session-`-prefixed `.json` and `.jsonl` files (`collect_antigravity_usage_records` in `src-tauri/src/ai_sessions/usage.rs`), replacing the old always-`unavailable` decision: the old files do contain tokens and users have existing data. Token parsing keeps the pre-migration gemini caliber: `.json` messages sum `tokens.cached` and `tokens.cache` into cache tokens, the model resolves through `message.model`, `message.modelName`, `message.metadata.model` and finally the file-level model, and `.jsonl` rows read `tokens.cached`; messages and rows with all-zero input, output, cache and total are skipped. Only this legacy path produces `UsageRecord`s.
- New-format brain transcripts are call counts only. Every per-conversation `transcript_full.jsonl` under both brain roots (`~/.gemini/antigravity-cli/brain` and `~/.gemini/antigravity/brain`, via `antigravity_brain_roots` and `find_antigravity_transcript` in `src-tauri/src/ai_sessions/history.rs`) contributes its in-window `USER_INPUT` rows — matched with `eq_ignore_ascii_case`, timestamped with the shared `antigravity_entry_timestamp_ms` helper, window-filtered to `[start_ms, end_ms)` — to `scanned_sessions` and `scanned_calls` (`transcript_calls`), and never to `records`. Summary, daily buckets and model stats therefore stay free of token pollution: the new format has no tokens, and a call count is the only honest caliber.
- The `empty` contract is unchanged from the baseline: `aggregate_tool_usage` reports `empty` exactly when the scan says `available` and `scanned_sessions` is zero. When sources exist but carry no tokens, the frontend shows `available` plus the Antigravity-specific token-unavailable explanation (`aiUsageTokenUnavailableLocally`, `Token usage locally unavailable` in `src/components/AiUsageStats.tsx`), scoped to `tool === "antigravity"` with `source_status === "empty"`, `scanned_sessions > 0` and zero summary calls; no other tool's copy is affected.
- Quota comes from the dedicated `sessions_antigravity_quota` command (`#[tauri::command(async)]` in `src-tauri/src/ai_sessions/usage.rs`): it runs `agy -p /usage --output-format json --print-timeout 30s` with a 35-second hard timeout, parses the `status` / `command.data.groups` envelope (`parse_antigravity_quota_envelope` rejects a non-`SUCCESS` status or missing groups, and a bucket without a numeric `remaining_fraction` fails the whole envelope), caches only successful snapshots for a 5-minute TTL, never caches failures, reports bilingual Chinese/English errors, and reads directly instead of going through the 30-second usage-scan cache (`USAGE_SCAN_CACHE_TTL`).

## Alternatives considered

- Keep the always-`unavailable` baseline: declined because the legacy files do contain token data and users have accumulated history there; refusing to parse discards real usage.
- Promote transcript `USER_INPUT` rows into token records with estimated tokens: declined because the new format carries no token usage and any estimate would pollute token aggregation; the call count is the only honest caliber.
- Treat sources-without-tokens as `unavailable` instead of `empty`: declined because the baseline contract (`available` with zero scanned sessions means `empty`) still holds, and the frontend already distinguishes the case with an Antigravity-scoped explanation.
- Route the quota command through the usage-scan cache: declined because quota is live data with its own 5-minute success-only TTL and must read directly on demand.

## Consequences

- Legacy `~/.gemini/tmp` history again yields token records at the pre-migration gemini caliber, while new-format brain transcripts contribute only `scanned_sessions` and `scanned_calls`.
- Because transcripts carry no tokens, agy interactive sessions from September onward still have no token detail; only call and session counts are reported for them.
- The quota card reads live `agy` output with a 5-minute success-only cache; failures surface bilingual errors and are retried on demand.
- Known coverage gap, stated as-is: the quota process path and zero-quota cost behavior have no test proof; only the envelope parsing (`parse_antigravity_quota_envelope`) is covered by tests.
- Incremental recording for print mode is deferred as a follow-up item.
- Supersession: no supersession. No active note records the old never-parse baseline or any Antigravity usage semantics, so there is nothing to retain or cross-link; the terminal-sync record that mentions Antigravity ([API Fusion Terminal Independent Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md)) only states that `claude`/`antigravity` records are untouched by gateway sync and is unrelated to usage scanning.
