# Agent Note: CommandCode Plan Templates Split Sync, Quota and Migration by Official Subscription Plan

Status: implemented

English | [中文](2026-11-23-commandcode-plan-templates.zh.md)

## Problem

- A single built-in `commandcode` provider template synced the whole official model list, so every CommandCode subscriber saw models their own subscription plan cannot serve; the ordinary list carries membership, names and endpoint declarations, while only the official client catalog declares each model's `minimumPlan` subscription tier.
- CommandCode sells four ascending plans whose allowed model sets are cumulative and whose official catalog keeps changing, so plan scoping has to keep the ordinary entries a tier allows instead of shipping a hand-maintained per-plan model list that would drift.
- Existing installations bound providers and persisted template state to the legacy `commandcode` identity, whose model set equals the entire catalog, so the split had to remove that duplicate built-in while preserving every operator-maintained field.

## Decision

- The built-in snapshot ships four CommandCode plan templates in ascending order before the OpenCode templates: `commandcode-go`, `commandcode-goat`, `commandcode-pro` and `commandcode-max`, all sharing `base_url` `https://api.commandcode.ai/provider/v1`, `protocol` `chat_completions`, `sync_strategy` `generic`, `quota_strategy` `commandcode`, `error_strategy` `generic`, `source` and `models_url` `https://api.commandcode.ai/provider/v1/models`, `icon` `commandcode` and an empty `models` list; the legacy `commandcode` built-in is removed.
- The plan gate lives in `src-tauri/src/ai_gateway/templates/source.rs`: one of those four ids whose trimmed `models_url` is exactly the canonical URL fetches the ordinary list and then `https://api.commandcode.ai/provider/v1/models?view=client` through the same credential-free 15-second client, while every other URL or template id is fetched once as configured.
- The client catalog and each entry's `minimumPlan` are validated before any publication: absent, `null` and string declarations are accepted, a trimmed blank string counts as no plan, and any other shape fails the whole sync.
- Tiers are cumulative over the ordinary entries: Go keeps `individual-go`, GOAT keeps Go plus `individual-goat`, Pro keeps GOAT plus `individual-pro`, and Max keeps every ordinary entry.
- For Go, GOAT and Pro an ordinary entry absent from the catalog or declaring a missing, blank, null, unknown or higher plan is dropped, a client-only entry never adds a model, and a filtered-empty set fails without persisting.
- The exact-id capability join, the five-value reasoning-effort filtering, the pricing merge and the GOAT documentation-page `contextTiers` fallback keep their previous behavior.
- `GATEWAY_CONFIG_SCHEMA_VERSION` is 10. The version-gated migration rebinds providers and persisted template state whose `template_id` is exactly `commandcode` to `commandcode-max`, preserving every user-maintained field and renaming only the default template name `CommandCode` to `CommandCode Max`; a stale legacy state is dropped when `commandcode-max` already exists, `commandcode` is removed from `deleted_template_ids` without adding any plan id, URL-only or other-template providers are never migrated, a second schema-10 read is byte-stable, a future version is not downgraded and a failed rewrite keeps the old encrypted bytes.
- Quota monitoring, bulk refresh and icon resolution continue from the `commandcode` quota strategy and icon for all four ids.

## Alternatives considered

- Ship a static model list per plan: declined because the official catalog changes across tiers, so a shipped list would drift from the endpoint and silently misstate what a plan can serve.
- Keep the legacy `commandcode` next to the plan templates: declined because its model set equals the Max plan and a second built-in with that set would duplicate one template.
- Add four new `SyncStrategy` variants: declined because it expands the persisted enum and the UI surface without value beyond four built-in templates that share one sync strategy.

## Consequences

- Delivered code is `src-tauri/src/ai_gateway/provider_templates.json`, `src-tauri/src/ai_gateway/templates/source.rs` for the plan tier, the client-catalog gate and the filtering, `src-tauri/src/ai_gateway/migration.rs` for the schema-10 rebind and `src-tauri/src/ai_gateway/types_config/config.rs` for `GATEWAY_CONFIG_SCHEMA_VERSION`.
- A CommandCode plan template now syncs only the models its subscription serves while `commandcode-max` keeps the unfiltered catalog, and an existing `commandcode` binding becomes `commandcode-max` on the next read without a template reset or a new plan binding.
- Verification: `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::templates` passed 99, `...--lib ai_gateway::quota` passed 2, `...--lib ai_gateway::tests::quota` passed 39, `...--lib ai_gateway::tests::go_usage` passed 26, `...--lib ai_gateway::tests::context_terminal_sync` passed 23 and `...--lib ai_gateway::tests::migration` passed 45; the three frontend smoke files passed 222 tests with Node's experimental web-storage global disabled, and the broader `--lib ai_gateway` run passed 1229 with two pre-existing failures (`opencode_zen_sidecar_transport_resolution_red`, a stale Zen base-URL assertion, and a network-dependent blackhole timeout) that reproduce on the untouched base commit.
- `MEMORY.md` records the four plan templates and their canonical fields, the cumulative `minimumPlan` filtering with its fail-closed boundaries and the schema-10 rebind in the same change.
- Supersession: partial. This record partially supersedes [Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md) for the built-in catalog membership, [Provider Template Reasoning Capability Sync](2026-10-16-provider-template-reasoning-sync.md) for the client-catalog gate identity and [Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md) for the current schema version, while their closed strategy fields, binding validation, exact-id join semantics, absence-versus-empty semantics and migration permanence remain in force; [CommandCode Provider Cards Show Account Quota](2026-09-23-commandcode-provider-quota.md) and [OpenCode Go Provider Cards Show Usage](2026-09-24-opencode-go-provider-usage.md) are retained without supersession because their sources, guards and caches are unchanged.
- Reintroduction condition: plan scoping rests on the published `minimumPlan` declarations, so a removed or renamed declaration must never be inferred from model names, and a plan template that cannot reach the client catalog fails rather than falling back to the unfiltered list.
