# Agent Note: AI Gateway Model Context Limits Are Declared Metadata

Status: implemented

English | [中文](2026-10-22-ai-gateway-model-context-limits.zh.md)

## Problem

The gateway had no owned per-model total context window. Terminal integrations need this metadata, but deriving it from model names, prices, completion limits or a second catalog would publish guesses that can exceed what a provider actually serves. Several configured sources can serve the same public model, so a single advertised value is only safe when every contributing source declares a known bound, and inherited copies must not let a stale editor or a health transition invent a larger value.

## Decision

`ProviderTemplateModel` and `ModelMapping` now carry optional `context_window: Option<u32>` measured in total context tokens. Both fields are serde-defaulted and skipped when absent, so old configuration files read as unspecified rather than zero, and the configuration schema stays at version 4 with no migration. Because the persisted type is `u32`, negative, fractional and above-`u32` declarations are rejected during decoding; a submitted zero is rejected before any write.

A template-bound provider is read-only for this field. The mapping follows the latest effective template, matched by trimmed, case-sensitive upstream model ID with the existing first-match rule, including aliases and user-disabled or automatically disabled rows. A manual template save and `ai_gateway_upsert_provider` both canonicalize submitted bound values against the latest template, so a stale dialog cannot establish an override. Creation, restoration and rebinding inherit the current template value; unbinding retains the last inherited value for later manual maintenance; removing a template model clears only that orphan inherited value without changing the existing mapping-retirement behavior; a template without the model yields unspecified.

Remote model lists are decoded only for declarations. Top-level `context_length` and nested `limit.context` each accept a positive integer or an explicit `null`; when both are declared they must agree, including both being `null`. A missing or `null` `limit` parent, or an object without `context`, declares no nested field, while any other non-object `limit` is invalid. Declarations are validated only after the existing hidden-entry, unsupported-endpoint and first-duplicate filters, so a rejected entry's malformed metadata cannot fail the sync. An invalid declared type, non-positive value, fraction or overflow fails the entire sync without publishing templates, mappings or timestamps. An absent declaration preserves the previous value for the same model ID (a new model stays unspecified), an explicit `null` clears the value, and a valid declaration replaces a previously manual value. Context is never inferred from prices, input or output limits, maximum completion tokens or model names.

`src-tauri/src/ai_gateway/context_limits.rs` exposes `resolve_public_context_window(&GatewayConfig, &str, UpstreamProtocol) -> Option<u32>` with `pub(in crate::ai_gateway)` visibility. It aggregates ordinary aliases across providers and every pool candidate plus expanded ordered fallback sources, deduplicating exact source identities of provider, local model, upstream model and effective protocol. The aggregate is the minimum only when every contributing source declares a known positive value; any unknown or unresolved configured source makes it unknown. Configured disabled and automatically disabled sources may conservatively contribute, while key marks, cooldowns, runtime health and recovery never raise the advertised bound. Pool aliases do not resolve on native Anthropic Messages ingress, and a `default_model` fallback route resolves only from the same provider's exact upstream declaration. The resolver reads declarative configuration only and does not touch routing or the network.

## Alternatives considered

- Infer a window from model names, pricing, output limits or maximum completion tokens: declined because such a value is a guess that can overstate the provider's real capacity.
- Let a bound provider keep a manual override: declined because a stale dialog or an older template could establish a value that disagrees with the authoritative template.
- Advertise the largest or first known source value when another source is unknown: declined because a public model must not claim more than every configured contributor supports.
- Treat an absent remote declaration as zero, unlimited or a clear: declined because absence preserves an existing value and only an explicit `null` clears it.
- Add a second model catalog or a configuration migration: declined because the template and mapping already own the metadata and an additive optional field keeps schema version 4 readable.

## Consequences

- Existing encrypted configurations stay readable with unspecified context; values round-trip only when present, and an older build may discard them on a later save.
- A remote refresh is atomic for this metadata: malformed or conflicting declarations leave the prior configuration and sync timestamp unchanged, while a valid, absent or `null` declaration follows the replace, preserve or clear rule.
- Bound rows cannot diverge from their template, and unbinding preserves the last inherited value until the operator edits the now-unbound mapping.
- Public advertising is conservative: complete aggregates report the minimum of all configured contributors, and any required unknown keeps the aggregate unknown instead of borrowing a larger value.
- [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) is partially superseded for its template-model shape and remote-sync merge facts by the optional declared context window and its ownership rules; its reduced catalog, manual per-template sync, pricing and retirement decisions remain in force.
- `MEMORY.md` and the authoritative navigation index register the new module, symbols and test paths in the same change, and the generated navigation view and note pairing record are refreshed alongside them.
- This change covers gateway-side declaration and resolution only; native terminal projection and operator UI are outside its delivered facts.
