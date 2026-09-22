# Agent Note: API Gateway Resolves the Listening Port per Build Profile

Status: implemented

English | [中文](2026-09-22-gateway-build-profile-port.zh.md)

## Problem

The API Gateway stores its configuration in a single encrypted `api_gateway.json` under `get_app_dir()`, which is the same `~/.config/onespace` directory for `tauri dev` (debug) and the installed release build. The stored `port` field was written by an earlier release run as the release default `17688`, and `normalize_config` only replaced it when it was `0`. `default_port()` already returned `DEV_DEFAULT_PORT` (`17689`) in debug builds, but that value only applied when no config file existed, so `npm run tauri dev` still read the stored `17688` and competed with the running installed app for the same loopback port. The sidebar DEV badge (driven by the frontend `import.meta.env.DEV`) therefore had no relation to the port actually in use.

## Decision

`types_config::resolve_port(stored: u16, is_dev: bool) -> u16` resolves the effective listening port for the current build profile: a stored `0` falls back to the profile default; the two canonical defaults are translated per profile, so debug builds resolve `17688` to `DEV_DEFAULT_PORT` (`17689`) and release builds resolve `17689` back to `DEFAULT_PORT` (`17688`); every other stored value is a genuine custom port and is returned unchanged. `storage::normalize_config` applies `resolve_port(config.port, cfg!(debug_assertions))` on every read and write, so `read_config` (and therefore `api_gateway_get_config`, `api_gateway_status`, `start_server`, autostart and terminal sync base URLs) always see the profile's effective port, and a config written by one profile can never move the other profile off its own port. Setting the other profile's canonical default while writing is what makes the shared file safe in both directions.

## Alternatives considered

- A separate dev config file such as `api_gateway.dev.json`: declined because it would isolate providers, local keys and the enabled flag, so a dev session would start with an empty gateway instead of the user's real setup.
- An environment-variable override wired through npm scripts: declined because the build profile already distinguishes the two runtimes, and a wrapper would add brittle script plumbing without solving the release side reading a dev-written value.
- Disabling the mapping under `#[cfg(test)]` so existing fixtures keep `17688`: declined as dishonest test behavior; the test build is the debug profile developers run, so one port-dependent terminal-sync fixture was moved to a custom port instead.
- Always forcing the profile default and ignoring the stored value: declined because it would discard genuine custom ports and break the existing guarantee that a configured port is not rewritten on bind failure.

## Consequences

- `npm run tauri dev` automatically resolves the shared `17688` to `17689`, while the release build resolves a dev-written `17689` back to `17688`, so both can listen at the same time without manual edits.
- Custom ports other than `0`, `17688` and `17689` are never rewritten, and bind failure still reports the configured port without falling back to another one.
- Because tests run in the debug profile, API Gateway tests now exercise the dev mapping; the terminal-sync seam fixture uses port `19000` to stay independent of the canonical mapping.
- Providers, local keys, the enabled flag and the terminal sync ledger remain shared between profiles; when both apps run at once, `synced_base_url` may alternate between the two ports and show as pending sync in the other app, which is accepted and not solved here.
- `MEMORY.md` and `docs/USAGE.md` describe the per-profile port resolution in the same change, and the navigation index is unchanged because no path, ownership or public symbol changed.
