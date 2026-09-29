# Agent Note: Dev and Release Builds Isolate Application State and AI Gateway Terminal Identity

Status: implemented

English | [中文](2026-09-29-dev-release-environment-isolation.zh.md)

## Problem

The debug (`tauri dev`) and installed release builds both resolved `get_app_dir()` to `~/.config/onespace`, so they shared one mutable application data tree: the encrypted `ai_gateway.json` with its upstream providers, local keys and terminal sync ledger, the storage roots, the skills and subagents local caches, and the AI Environments legacy providers fallback. The [port decision](2026-09-22-gateway-build-profile-port.md) separated only the listening port and explicitly accepted that providers, local keys, the enabled flag and the ledger remain shared, so a dev terminal sync could rewrite the release's OpenCode `gateway` provider entry to `http://127.0.0.1:17689/v1`, and dev experiments ran against real release state.

## Decision

- **Build profile**: `config::is_dev_build()` is true only for `cfg(all(debug_assertions, not(test)))`; `config::app_dir_for(home, is_dev)` returns `~/.config/onespace-dev` for dev and `~/.config/onespace` for release, and the bound `get_app_dir()` derives every application-managed path from it. Unit-test compilations keep the release resolution, so existing test fixtures do not move.
- **Startup seed**: `config::seed_dev_gateway_files_on_start()` runs at the top of `run()` before CLI handling. On every debug start, `config::seed_gateway_files(release_dir, dev_dir)` first copies `.local_key` when the release source exists and the dev destination is missing, then copies `ai_gateway.json` only when its destination is missing and the dev `.local_key` bytes equal the release `.local_key` bytes. It never overwrites existing dev files, never copies the usage database, logs failures so startup continues and the next start retries, and never writes the release directory.
- **Profile-derived storage and caches**: `config::default_storage_type_for(is_dev)` gives a dev build without `config.json` the `local` storage backend, never the release profile's macOS iCloud default, and the local and shared storage roots, the local-data mirror (`ensure_local_data_mirror_initialized_at`), the skills and subagents local caches, and the AI Environments legacy providers fallback all resolve under the profile app directory.
- **Terminal identity**: `TerminalSyncProfile` (`RELEASE`, `DEV`, `current()`) drives the payload identity and marker claims: release owns the OpenCode `provider_key = gateway`, the `AI Gateway` name and the boolean `ai_gateway_gateway = true` marker, while dev owns `gateway-dev`, `AI Gateway (Dev)` and the string `"dev"` marker. Each profile claims only its own marker and treats the other profile's records and the legacy rename markers as foreign, with release keeping legacy ownership. The existing key-scoped OpenCode projection writes only the payload's own provider key, so a dev sync never creates, updates or removes `provider.gateway`; Codex registration uses a distinct provider id and Codex activation stays manual.
- **Ports unchanged**: release still resolves `17688` and dev still resolves `17689`; `resolve_port` keeps translating the two canonical defaults per profile because a first-run seed or legacy shared history can carry the other profile's default, and custom ports stay untouched.
- **Out of scope**: AI Environments service-provider projections, MCP projections, `~/.agents/skills` content, an explicitly selected iCloud or custom storage path, OS-level shortcut and tray conflicts, and the Windows-only message-store fallback remain shared and unchanged.

## Alternatives considered

- No seed, keeping the profiles fully separate: declined because a dev session would start with an empty gateway and developers would recreate providers and keys instead of exercising the real setup.
- A full copy of the release directory, or copying more files including the usage database: declined because dev does not need the release usage history, and an unconditional copy would overwrite in-app dev edits; only the two gateway core files are seeded, and only while missing.
- Hard-linking or symlinking `ai_gateway.json` instead of copying it once: declined because writes through either profile would mutate the other file, recreating the shared mutable state this change removes.
- Reusing the release iCloud storage path for dev: declined because dev would write test data into the cloud-synced release tree and could propagate it to other devices; an explicitly selected iCloud or custom path stays out of scope instead.
- Keeping the shared `gateway` terminal provider identity: declined because both profiles would claim and overwrite the same OpenCode provider entry, so each profile now writes and reads only its own records.

## Consequences

- `npm run tauri dev` creates and uses `~/.config/onespace-dev`; the first debug start seeds `.local_key` and `ai_gateway.json` from `~/.config/onespace`, and deleting the dev directory or a single seeded gateway file restores only that missing file from the release source on the next start.
- The release directory is never written by the seed; release keeps its directory, port, storage defaults and its `gateway` record, and existing boolean-marked and legacy-marked terminal records stay owned by release.
- Both builds can run concurrently: each gateway listens on its own port and each terminal projection touches only its own provider key with an atomic write, so an OpenCode configuration can hold `provider.gateway` and `provider.gateway-dev` side by side; the release's next sync repairs a stale `provider.gateway` that points at `http://127.0.0.1:17689/v1`.
- Codex keeps manual activation; activating the dev provider later projects only its own `model_providers.onespace_<id>` entry, and the release entry stays untouched until it is manually changed.
- The shared-state consequence of [the port decision](2026-09-22-gateway-build-profile-port.md) is superseded: providers, local keys, the enabled flag and the terminal sync ledger are no longer shared between profiles, while its per-profile `resolve_port` translation remains in force.
- `MEMORY.md`, `README.md` and `docs/USAGE.md` describe the isolated dev directory, the seed rule and the two provider identities in the same change, and the navigation index records the profile app-directory helpers and the terminal profile value.
