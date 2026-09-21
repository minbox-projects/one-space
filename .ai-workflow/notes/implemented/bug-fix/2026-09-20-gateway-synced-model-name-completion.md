# Agent Note: Gateway Synced Model Names Cover the Whole Identifier

Status: implemented

English | [中文](2026-09-20-gateway-synced-model-name-completion.zh.md)

## Problem

When a provider template synced its model list, `parse_model_list_source` stored an object entry's source `name` verbatim as the template model's `display_name`. Upstream names are often shorter than their identifiers: `poolside/laguna-s-2.1-free` arrived with source name "Laguna S 2.1", so the gateway model list and a later terminal sync showed "Laguna S 2.1" and dropped the distinguishing `free` suffix — along with other identifier-only parts such as `paid`, `:free`, date suffixes and size suffixes. The stored name therefore did not represent the complete upstream identifier, while the untouched-only propagation rule faithfully spread the incomplete name to derived providers.

## Decision

A model-list sync now stores a local name that covers the whole identifier. `complete_model_display_name` (`src-tauri/src/api_gateway/templates.rs`), beside `parse_model_list_source`, takes a base plus the upstream identifier: the trimmed source `name` is the base, a missing or blank source name falls back to the previously stored display name for the same identifier, and when no local name exists either the stored name is the transformed identifier segment on its own. Coverage compares the identifier's last `/`-separated segment (the whole identifier when that segment is empty) against the base as a case-insensitive ASCII alphanumeric sequence; the consumed prefix counts as expressed, and the raw remainder starting right after the last consumed alphanumeric character is transformed — leading separators dropped, separator runs turned into single spaces, each lowercase word start upper-cased, version numbers such as `2.1` kept intact — and appended after a single space. A base that already covers the segment is stored trimmed and verbatim, keeping its casing, punctuation and any extra content such as `(latest)`; the vendor prefix before the last `/` is never appended. Applying the same model-list response twice produces byte-identical names. The rule adds no write beyond `display_name`: `local_model`, the protocol handling, the enabled flag, price rows, the ignored-model set and the atomic single-`write_config` persistence stay exactly as they are, loading an older `api_gateway.json` rewrites nothing, and derived mappings take the completed name only under the existing untouched-only rule. [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) is corrected in place for its stored-`name`-as-display-name fact while its remaining decisions stay in force.

## Alternatives considered

- Transform every identifier unconditionally and ignore the source name: declined because the endpoint authors the official name; discarding it would lose casing, punctuation and extra content the identifier does not carry, and the live catalog shows complete source names must survive verbatim.
- Rewrite already persisted names at load or startup: declined because loading must not migrate data; names change only on the next sync of their template, which keeps the write boundary at the template state and the derived providers.
- Keep the source name winning permanently and store it verbatim: declined because that is the rule that dropped the `free`, `paid`, `:free`, date and size parts; the endpoint stays the authority for the protocol, but the display name must express the whole identifier.

## Consequences

- The reported case now completes: `poolside/laguna-s-2.1-free` with source name "Laguna S 2.1" stores "Laguna S 2.1 Free", while the local model id stays `poolside/laguna-s-2.1-free`.
- Complete source names are preserved byte-for-byte apart from trimming, and no vendor prefix or duplicated word is ever appended.
- A missing or blank source name completes the previously stored local name against the identifier, or stores the transformed identifier segment alone when no local name exists.
- Syncing the same response twice yields byte-identical names, and loading a configuration without a sync leaves every persisted name unchanged.
- Behavior coverage lives in `src-tauri/src/api_gateway/tests/templates.rs`, including the 71-entry live payload fixture captured from `https://api.commandcode.ai/provider/v1/models` on 2026-09-20.
- `MEMORY.md` and the `api-gateway` / `api-gateway-backend` entries in `navigation.json` state the completed-name rule in the same change, and `navigation.md` was regenerated from the authoritative JSON.
