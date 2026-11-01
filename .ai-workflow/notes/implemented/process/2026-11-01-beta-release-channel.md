# Agent Note: Beta Release Channel with a Stable Fallback

Status: implemented

English | [中文](2026-11-01-beta-release-channel.zh.md)

## Problem

OneSpace could only publish stable releases. Maintainers had no supported way to ship several test builds of an upcoming version without advancing the stable line, users could not opt into those builds through the normal updater, and a beta install was indistinguishable from a stable one. A beta channel also had to exercise the production data path instead of a separate profile, and an experimental update lookup must never strand a user without the stable channel or offer a downgrade.

## Decision

The release helper adds `beta <base> [--dry-run]` over the four release files `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`. The base is an explicit `X.Y.Z`; a current stable version below the base starts `<base>-beta.1`, a current version on the same base's beta increments its number, and invalid or non-base targets, a base at or below the current stable version, a base below the current beta base, and mismatched release files are rejected without writing. `--dry-run` reports the next version only. The existing `tag` operation accepts the resulting prerelease version.

`previous-tag` and `is-prerelease` expose the release range to the release workflow. Beta tags `v<base>-beta.N` are published as GitHub prereleases; a stable release compares its changelog against the previous stable tag and skips betas, while a beta release compares against the previous tag.

Beta release builds resolve the same `~/.config/onespace` directory, configuration and data as stable builds. No beta profile or migration exists, and the development profile `~/.config/onespace-dev` is unchanged. `receive_beta_updates` is an optional, device-local configuration field that defaults to false, is shared by stable and beta builds on the same device, is not synchronized across devices, and is ignored by older builds.

Settings → Updates exposes "Receive beta updates". When it is enabled, an update check selects the highest non-draft GitHub release that publishes a `latest.json` asset, including prereleases, and checks that release through the existing signature-verified updater before falling back sequentially to the configured stable endpoint; a failed lookup, no eligible release, or a missing platform or error in the selected beta leaves the stable check in place. When it is disabled only the stable endpoint is used, and no path downgrades a user.

`src/lib/appVersion.ts` exposes `useAppVersion`, a failure-safe runtime version hook, and `isBetaVersion`, which matches only `X.Y.Z-beta.N`. The main shell renders a BETA badge in the top-left brand area for a beta runtime version, in both expanded and collapsed sidebars; stable, missing or malformed versions render no badge. The frontend fallback comparison orders a stable version above its own beta and a higher beta number above a lower one.

## Alternatives considered

- A separate beta application directory, configuration profile or data migration: declined because beta builds must exercise the production data path, and a second profile would add migration and dual-data risk for no user benefit.
- Automatic beta enrollment or a beta-only preference: declined because the preference stays explicit and off by default, and the same device preference must govern both builds.
- Selecting a beta release by version alone or ignoring `latest.json`: declined because drafts, invalid tags and releases without a usable manifest would become update endpoints.
- A single combined beta and stable endpoint check instead of a sequential second check: declined because a beta release without the running platform or a beta check error must not block a valid stable update.
- Deriving the badge from a duplicated build-time version source: declined because the runtime `getVersion()` value is the single source and a failed lookup must not throw into the shell.

## Consequences

- Beta tags are valid semantic-version prereleases and remain additive: the configured stable endpoint, signature verification, artifact naming and signing certificate are unchanged, and stable clients that never enable the preference see no difference.
- The preference is shared on one device across stable and beta builds and is not synchronized, so enabling beta offers affects both installs on that machine; older builds accept and ignore the field.
- Update availability depends on the GitHub Releases API and each selected release's `latest.json`; an outage, a rate limit, an empty result or a platform gap falls back to the stable endpoint without a downgrade.
- The beta badge depends only on the runtime version, so a malformed or missing version shows nothing and never breaks the shell.
- Behavior coverage lives in the release-helper shell test, the frontend badge, preference and comparison tests, and the backend selection, fallback and configuration tests; the documentation, navigation index and this note are validated with the project commands.
