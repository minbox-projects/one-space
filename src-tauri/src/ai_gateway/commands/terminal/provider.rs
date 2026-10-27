use super::super::{GATEWAY_MARKER_KEY, TerminalSyncProfile};
use super::{
    gateway_protocol_servable, is_supported_terminal_tool, non_empty, TerminalContextCoverage,
    TerminalContextSummary,
};
use crate::ai_gateway::context_limits::resolve_public_context_window_from;
use crate::ai_gateway::reasoning::pool_reasoning_catalog;
use crate::ai_gateway::{
    ClaudeModelSelection, GatewayConfig, GatewayUpstreamProvider, ModelPool, UpstreamProtocol,
};
use serde_json::{json, Map, Value};

/// npm override for an opencode model the gateway can serve only through the
/// Responses protocol: `@ai-sdk/openai` POSTs `/responses` instead of the
/// `/chat/completions` used by `@ai-sdk/openai-compatible`.
const OPENCODE_RESPONSES_NPM: &str = "@ai-sdk/openai";
/// Build the terminal provider record written by AI Gateway for one tool.
///
/// The record is always marked with `profile.marker()` under `tool_config`,
/// carries the resolved default local key value as its `api_key` (top-level and,
/// for opencode, `tool_config.options.apiKey`), and never carries an
/// `active`/`is_active` flag. The `provider_key` and display name come from
/// `profile`, so the release and dev profiles register distinct records.
/// Opencode activation plus projection to opencode.json are applied separately
/// via the service-provider active list and projection after the upsert
/// succeeds.
#[cfg(test)]
pub(in crate::ai_gateway) fn build_gateway_provider(
    provider_id: &str,
    tool: &str,
    base_url: &str,
    api_key: &str,
    gateways: &[GatewayUpstreamProvider],
    profile: TerminalSyncProfile,
) -> Result<serde_json::Value, String> {
    build_gateway_provider_with_pools(
        provider_id,
        tool,
        base_url,
        api_key,
        gateways,
        &[],
        None,
        profile,
    )
}

/// Pool-aware variant of [`build_gateway_provider`].
///
/// `pools` contributes the public alias of every enabled configured pool to the
/// generated catalog: an OpenCode Chat-compatible row with no guessed reasoning
/// variants, and a Codex generated model that only fills a gap. `previous_model`
/// is the model already stored on the owned record, used only to retain a valid
/// previously selected enabled pool alias on a Codex resync. The original
/// `build_gateway_provider` keeps its signature and passes no pools.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_gateway_provider_with_pools(
    provider_id: &str,
    tool: &str,
    base_url: &str,
    api_key: &str,
    gateways: &[GatewayUpstreamProvider],
    pools: &[ModelPool],
    previous_model: Option<&str>,
    profile: TerminalSyncProfile,
) -> Result<serde_json::Value, String> {
    let tool = tool.trim().to_ascii_lowercase();
    if !is_supported_terminal_tool(&tool) {
        return Err(format!(
            "unsupported terminal tool '{tool}': only opencode and codex are supported"
        ));
    }

    let mut tool_config = Map::new();
    tool_config.insert(GATEWAY_MARKER_KEY.to_string(), profile.marker());

    let mut object = Map::new();
    object.insert("id".to_string(), Value::String(provider_id.to_string()));
    object.insert(
        "name".to_string(),
        Value::String(profile.provider_name().to_string()),
    );
    object.insert("tool".to_string(), Value::String(tool.clone()));
    object.insert("base_url".to_string(), Value::String(base_url.to_string()));
    object.insert("api_key".to_string(), Value::String(api_key.to_string()));

    if tool == "opencode" {
        object.insert(
            "provider_key".to_string(),
            Value::String(profile.provider_key().to_string()),
        );
        tool_config.insert(
            "npm".to_string(),
            Value::String("@ai-sdk/openai-compatible".to_string()),
        );
        let mut options = Map::new();
        options.insert("baseURL".to_string(), Value::String(base_url.to_string()));
        options.insert("apiKey".to_string(), Value::String(api_key.to_string()));
        tool_config.insert("options".to_string(), Value::Object(options));

        let mut models = Map::new();
        // A gateway contributes rows only while it is user-enabled, and a row
        // only while it is user-enabled and not auto-disabled: the provider-level
        // legacy `auto_disabled` field never filters.
        for gateway in gateways.iter().filter(|gateway| gateway.enabled) {
            for mapping in gateway
                .mappings
                .iter()
                .filter(|mapping| mapping.enabled && !mapping.auto_disabled)
            {
                let Some(local_model) = non_empty(Some(mapping.local_model.as_str())) else {
                    continue;
                };
                if models.contains_key(&local_model) {
                    continue;
                }
                // Per-local-model servability across every enabled gateway: a
                // model the gateway can serve over Chat or Anthropic Messages
                // keeps opencode's existing Chat adapter (the Chat entry
                // converts for an Anthropic upstream), so only a genuinely
                // Responses-only model gets the `@ai-sdk/openai` npm override.
                let chat_servable = gateway_protocol_servable(
                    gateways,
                    &local_model,
                    UpstreamProtocol::ChatCompletions,
                );
                let responses_servable =
                    gateway_protocol_servable(gateways, &local_model, UpstreamProtocol::Responses);
                let anthropic_servable = gateway_protocol_servable(
                    gateways,
                    &local_model,
                    UpstreamProtocol::AnthropicMessages,
                );
                let name = non_empty(mapping.display_name.as_deref())
                    .or_else(|| non_empty(Some(mapping.upstream_model.as_str())))
                    .unwrap_or_default();
                let mut model = Map::new();
                model.insert("name".to_string(), Value::String(name));

                // OpenCode exposes a reasoning model's selectable strengths as
                // model variants. Carry the mapping's configured levels so the
                // tool offers exactly the gateway's tiers instead of falling
                // back to its own catalog/built-in guess for the model id.
                let mut variants = Map::new();
                for effort in &mapping.reasoning_efforts {
                    let Some(effort) = non_empty(Some(effort.as_str())) else {
                        continue;
                    };
                    if variants.contains_key(&effort) {
                        continue;
                    }
                    let mut variant = Map::new();
                    variant.insert("reasoningEffort".to_string(), Value::String(effort.clone()));
                    variants.insert(effort, Value::Object(variant));
                }
                if !variants.is_empty() {
                    model.insert("reasoning".to_string(), Value::Bool(true));
                    model.insert("variants".to_string(), Value::Object(variants));
                }

                if !chat_servable && !anthropic_servable && responses_servable {
                    let mut provider = Map::new();
                    provider.insert(
                        "npm".to_string(),
                        Value::String(OPENCODE_RESPONSES_NPM.to_string()),
                    );
                    model.insert("provider".to_string(), Value::Object(provider));
                }

                if let Some(limit) = opencode_model_limit(gateways, pools, &local_model) {
                    model.insert("limit".to_string(), limit);
                }

                models.insert(local_model, Value::Object(model));
            }
        }
        // Every enabled configured pool contributes its public alias exactly
        // once as a Chat-compatible row. The row publishes the pool's static
        // reasoning capability: canonical variants (the preference as both the
        // variant name and `reasoningEffort`) plus the same support/default/
        // coverage metadata as discovery. A pool never borrows a source model's
        // reasoning variants, never exposes a native mapped value, is never given
        // the Responses npm override, and fabricates no price/credential field. An
        // unmapped legacy pool keeps no `variants`. A disabled
        // pool is never exposed.
        for pool in pools.iter().filter(|pool| pool.enabled) {
            let Some(local_model) = non_empty(Some(pool.model.as_str())) else {
                continue;
            };
            if models.contains_key(&local_model) {
                continue;
            }
            let name = non_empty(Some(pool.name.as_str())).unwrap_or_else(|| local_model.clone());
            let catalog = pool_reasoning_catalog(gateways, pool);
            let mut model = Map::new();
            model.insert("name".to_string(), Value::String(name));
            // `interleaved` keeps historical reasoning replayable through the
            // gateway's Chat adapter for every enabled pool row, including a pool
            // that declares no selectable effort. It does not declare effort
            // support or infer any upstream capability.
            let mut interleaved = Map::new();
            interleaved.insert(
                "field".to_string(),
                Value::String("reasoning_content".to_string()),
            );
            model.insert("interleaved".to_string(), Value::Object(interleaved));
            if !catalog.reasoning_efforts.is_empty() {
                let mut variants = Map::new();
                for effort in &catalog.reasoning_efforts {
                    let mut variant = Map::new();
                    variant.insert("reasoningEffort".to_string(), Value::String(effort.clone()));
                    variants.insert(effort.clone(), Value::Object(variant));
                }
                model.insert("reasoning".to_string(), Value::Bool(true));
                model.insert("variants".to_string(), Value::Object(variants));
            }
            model.insert(
                "reasoning_efforts".to_string(),
                Value::Array(
                    catalog
                        .reasoning_efforts
                        .iter()
                        .map(|effort| Value::String(effort.clone()))
                        .collect(),
                ),
            );
            model.insert(
                "default_reasoning_effort".to_string(),
                Value::String(catalog.default_reasoning_effort),
            );
            model.insert(
                "reasoning_coverage".to_string(),
                Value::Array(
                    catalog
                        .reasoning_coverage
                        .iter()
                        .map(|row| {
                            json!({
                                "effort": row.effort,
                                "mapped_candidates": row.mapped_candidates,
                                "total_candidates": row.total_candidates,
                            })
                        })
                        .collect(),
                ),
            );
            if let Some(limit) = opencode_model_limit(gateways, pools, &local_model) {
                model.insert("limit".to_string(), limit);
            }
            models.insert(local_model, Value::Object(model));
        }
        tool_config.insert("models".to_string(), Value::Object(models));
    } else {
        tool_config.insert("wire_api".to_string(), Value::String("chat".to_string()));
        let model = select_codex_model(gateways, pools, previous_model);
        if let Some(model) = model {
            let context = resolve_public_context_window_from(
                gateways,
                pools,
                &model,
                UpstreamProtocol::ChatCompletions,
            );
            tool_config.insert(
                "model_context_window".to_string(),
                context.map_or(Value::Null, |value| json!(value)),
            );
            object.insert("model".to_string(), Value::String(model));
        } else {
            // The managed scalar always travels so a merged canonical record can
            // clear a value left by a previously selected model.
            tool_config.insert("model_context_window".to_string(), Value::Null);
        }
    }

    object.insert("tool_config".to_string(), Value::Object(tool_config));
    Ok(Value::Object(object))
}
/// The model a Codex sync generates: a retained enabled pool alias, else the
/// first enabled ordinary mapping, else an enabled provider default, else the
/// first enabled pool alias.
fn select_codex_model(
    gateways: &[GatewayUpstreamProvider],
    pools: &[ModelPool],
    previous_model: Option<&str>,
) -> Option<String> {
    let ordinary = gateways
        .iter()
        .filter(|gateway| gateway.enabled)
        .find_map(|gateway| {
            gateway
                .mappings
                .iter()
                .filter(|mapping| mapping.enabled && !mapping.auto_disabled)
                .find_map(|mapping| non_empty(Some(mapping.local_model.as_str())))
        })
        .or_else(|| {
            gateways
                .iter()
                .filter(|gateway| gateway.enabled)
                .find_map(|gateway| non_empty(gateway.default_model.as_deref()))
        });
    // Retain an already selected *enabled* pool alias on resync; a disabled or
    // retired alias is not retainable, so an ordinary default wins.
    let retained_pool = previous_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| {
            pools
                .iter()
                .any(|pool| pool.enabled && pool.model.trim() == *value)
        })
        .map(str::to_string);
    // A pool-only configuration (no ordinary model) uses the first enabled pool
    // alias as its generated fallback; an ordinary default is never replaced just
    // because a pool exists.
    let fallback_pool = pools
        .iter()
        .filter(|pool| pool.enabled)
        .find_map(|pool| non_empty(Some(pool.model.as_str())));
    retained_pool.or(ordinary).or(fallback_pool)
}
/// Resolve one OpenCode row's context under the protocol its generated row
/// actually uses: the Chat adapter when servable, otherwise the Responses
/// override.
fn opencode_model_context(
    gateways: &[GatewayUpstreamProvider],
    pools: &[ModelPool],
    local_model: &str,
) -> Option<u32> {
    let protocol = if gateway_protocol_servable(
        gateways,
        local_model,
        UpstreamProtocol::ChatCompletions,
    ) {
        UpstreamProtocol::ChatCompletions
    } else {
        UpstreamProtocol::Responses
    };
    resolve_public_context_window_from(gateways, pools, local_model, protocol)
}

/// The OpenCode `limit` object for one generated model, or `None` when the
/// conservative aggregate is unknown. `output: 0` keeps the native
/// unspecified-output behavior without asserting another capability.
fn opencode_model_limit(
    gateways: &[GatewayUpstreamProvider],
    pools: &[ModelPool],
    local_model: &str,
) -> Option<Value> {
    let context = opencode_model_context(gateways, pools, local_model)?;
    Some(json!({ "context": context, "output": 0 }))
}
fn coverage_from(values: impl IntoIterator<Item = Option<u32>>) -> TerminalContextCoverage {
    let mut coverage = TerminalContextCoverage {
        total: 0,
        known: 0,
        unknown: 0,
    };
    for value in values {
        coverage.total += 1;
        if value.filter(|value| *value > 0).is_some() {
            coverage.known += 1;
        } else {
            coverage.unknown += 1;
        }
    }
    coverage
}

/// Coverage over the same ordinary and pool models an OpenCode sync generates.
pub(super) fn opencode_context_summary(
    config: &GatewayConfig,
) -> TerminalContextSummary {
    let mut models: Vec<String> = Vec::new();
    for gateway in config.providers.iter().filter(|gateway| gateway.enabled) {
        for mapping in gateway
            .mappings
            .iter()
            .filter(|mapping| mapping.enabled && !mapping.auto_disabled)
        {
            let Some(local_model) = non_empty(Some(mapping.local_model.as_str())) else {
                continue;
            };
            if !models.iter().any(|existing| existing == &local_model) {
                models.push(local_model);
            }
        }
    }
    for pool in config.model_pools.iter().filter(|pool| pool.enabled) {
        let Some(local_model) = non_empty(Some(pool.model.as_str())) else {
            continue;
        };
        if !models.iter().any(|existing| existing == &local_model) {
            models.push(local_model);
        }
    }
    let coverage = coverage_from(models.iter().map(|model| {
        opencode_model_context(&config.providers, &config.model_pools, model)
    }));
    let reasons = if coverage.unknown > 0 {
        vec!["unknown_context".to_string()]
    } else {
        Vec::new()
    };
    TerminalContextSummary {
        mode: "per_model".to_string(),
        scope: "generated_models".to_string(),
        coverage,
        selected_model: None,
        raw_context: None,
        applied_context: None,
        reasons,
    }
}
/// Coverage and effective scalar for the synchronized Codex selection only.
pub(super) fn codex_context_summary(
    config: &GatewayConfig,
    previous_model: Option<&str>,
) -> TerminalContextSummary {
    let selected = select_codex_model(&config.providers, &config.model_pools, previous_model);
    let resolved = selected.as_deref().and_then(|model| {
        resolve_public_context_window_from(
            &config.providers,
            &config.model_pools,
            model,
            UpstreamProtocol::ChatCompletions,
        )
    });
    let coverage = if selected.is_some() {
        coverage_from(std::iter::once(resolved))
    } else {
        TerminalContextCoverage {
            total: 0,
            known: 0,
            unknown: 0,
        }
    };
    let mut reasons = Vec::new();
    if selected.is_none() {
        reasons.push("incomplete_selection".to_string());
    } else if resolved.is_none() {
        reasons.push("unknown_context".to_string());
    }
    TerminalContextSummary {
        mode: "selected_model".to_string(),
        scope: "activation".to_string(),
        coverage,
        selected_model: selected,
        raw_context: resolved,
        applied_context: resolved,
        reasons,
    }
}
/// The shared Claude compaction target for the deduplicated choices.
pub(super) fn claude_context_summary(
    config: &GatewayConfig,
    selection: Option<&ClaudeModelSelection>,
) -> TerminalContextSummary {
    let Some(selection) = selection else {
        return TerminalContextSummary {
            mode: "shared_compaction".to_string(),
            scope: "global_and_isolated_settings".to_string(),
            coverage: TerminalContextCoverage {
                total: 0,
                known: 0,
                unknown: 0,
            },
            selected_model: None,
            raw_context: None,
            applied_context: None,
            reasons: vec!["incomplete_selection".to_string()],
        };
    };

    let mut families: Vec<&str> = Vec::new();
    let mut incomplete = false;
    for choice in [&selection.haiku, &selection.sonnet, &selection.opus] {
        let choice = choice.trim();
        if choice.is_empty() {
            incomplete = true;
            continue;
        }
        if !families.contains(&choice) {
            families.push(choice);
        }
    }
    let default_model = selection.default_model.trim();
    if default_model.is_empty() {
        incomplete = true;
    }

    let family_values: Vec<Option<u32>> = families
        .iter()
        .map(|model| {
            resolve_public_context_window_from(
                &config.providers,
                &config.model_pools,
                model,
                UpstreamProtocol::AnthropicMessages,
            )
        })
        .collect();
    let coverage = coverage_from(family_values.iter().copied());
    let default_value = (!default_model.is_empty()).then(|| {
        resolve_public_context_window_from(
            &config.providers,
            &config.model_pools,
            default_model,
            UpstreamProtocol::AnthropicMessages,
        )
    });
    let all_known = !incomplete
        && default_value.flatten().is_some()
        && family_values.iter().all(|value| value.is_some());

    let mut reasons = Vec::new();
    if incomplete {
        reasons.push("incomplete_selection".to_string());
    }
    if !incomplete && !all_known {
        reasons.push("unknown_context".to_string());
    }
    let mut raw_context = None;
    let mut applied_context = None;
    if all_known {
        let mut values: Vec<u32> = family_values.iter().flatten().copied().collect();
        values.push(default_value.flatten().unwrap_or_default());
        let minimum = values.into_iter().min().unwrap_or_default();
        raw_context = Some(minimum);
        if minimum >= 100_000 {
            applied_context = Some(minimum.min(1_000_000));
            if minimum > 1_000_000 {
                reasons.push("capped_at_1000000".to_string());
            }
        } else {
            reasons.push("below_minimum".to_string());
        }
    }

    TerminalContextSummary {
        mode: "shared_compaction".to_string(),
        scope: "global_and_isolated_settings".to_string(),
        coverage,
        selected_model: None,
        raw_context,
        applied_context,
        reasons,
    }
}
/// A managed scalar comparison value: absent, JSON null and non-positive values
/// all mean "not configured" without pretending to be a fresh declaration.
pub(super) fn context_scalar(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64).filter(|value| *value > 0)
}

/// Read the four persisted Claude model choices back from a canonical,
/// marker-owned Claude service provider as it appears in the terminal service
/// provider list. A missing mirrored default or any missing/blank family row
/// yields `None`, so an incomplete selection is never resent as a partial one.
pub(super) fn claude_selection_from_provider(
    provider: &Value,
) -> Option<ClaudeModelSelection> {
    let default_model = provider
        .get("claude_default_model")
        .and_then(Value::as_str)
        .or_else(|| provider.get("model").and_then(Value::as_str))?
        .trim();
    if default_model.is_empty() {
        return None;
    }
    let mappings = provider.get("claude_model_mappings")?.as_array()?;
    let family = |name: &str| {
        mappings
            .iter()
            .find(|row| row.get("family").and_then(Value::as_str) == Some(name))
            .and_then(|row| row.get("upstream_model").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(ClaudeModelSelection {
        default_model: default_model.to_string(),
        haiku: family("haiku")?,
        sonnet: family("sonnet")?,
        opus: family("opus")?,
    })
}

/// A collision-safe Claude profile code derived from the new provider UUID.
/// The owned canonical record's code is reused verbatim on later syncs; this
/// fallback only runs when no owned record (and therefore no code) exists yet.
pub(super) fn derived_claude_code(provider_id: &str) -> String {
    let mut suffix: String = provider_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect();
    if suffix.is_empty() {
        suffix = uuid::Uuid::new_v4().simple().to_string();
    }
    format!("gw-claude-{}", &suffix[..suffix.len().min(24)])
}

/// Build the gateway-owned Claude service-provider payload for one sync.
///
/// It carries the tool-specific origin (never a trailing `/v1`), the resolved
/// default local key, the default and three family local mapping ids, a
/// UUID-derived or reused `code`, and the profile marker under `tool_config`.
/// It never carries an `active`/`is_active` flag; activation and projection are
/// applied separately after the upsert succeeds.
pub(super) fn build_claude_gateway_provider(
    provider_id: &str,
    origin: &str,
    api_key: &str,
    selection: &ClaudeModelSelection,
    code: &str,
    auto_compact_window: Option<u32>,
    profile: TerminalSyncProfile,
) -> Value {
    let mapping = |family: &str, display_name: &str, upstream_model: &str| {
        json!({
            "family": family,
            "display_name": display_name,
            "upstream_model": upstream_model,
        })
    };
    let mut tool_config = Map::new();
    tool_config.insert(GATEWAY_MARKER_KEY.to_string(), profile.marker());
    // The managed scalar always travels; `null` clears a threshold left by a
    // previous selection while leaving unrelated Claude settings untouched.
    tool_config.insert(
        "claude_auto_compact_window".to_string(),
        auto_compact_window.map_or(Value::Null, |value| json!(value)),
    );

    let mut object = Map::new();
    object.insert("id".to_string(), Value::String(provider_id.to_string()));
    object.insert(
        "name".to_string(),
        Value::String(profile.provider_name().to_string()),
    );
    object.insert("tool".to_string(), Value::String("claude".to_string()));
    object.insert("base_url".to_string(), Value::String(origin.to_string()));
    object.insert("api_key".to_string(), Value::String(api_key.to_string()));
    object.insert(
        "model".to_string(),
        Value::String(selection.default_model.clone()),
    );
    object.insert("code".to_string(), Value::String(code.to_string()));
    object.insert(
        "claude_api_format".to_string(),
        Value::String("anthropic_messages".to_string()),
    );
    object.insert(
        "claude_connection_mode".to_string(),
        Value::String("native_anthropic".to_string()),
    );
    object.insert(
        "claude_auth_env_key".to_string(),
        Value::String("ANTHROPIC_API_KEY".to_string()),
    );
    object.insert(
        "claude_model_mappings".to_string(),
        Value::Array(vec![
            mapping("haiku", "Haiku", &selection.haiku),
            mapping("sonnet", "Sonnet", &selection.sonnet),
            mapping("opus", "Opus", &selection.opus),
        ]),
    );
    object.insert("tool_config".to_string(), Value::Object(tool_config));
    Value::Object(object)
}

/// Validate an explicitly submitted Claude selection: every value must be a
/// non-blank, enabled, native Anthropic-Messages-servable local mapping id (an
/// Anthropic mapping row or an Anthropic-protocol provider default). The same
/// id may fill several slots.
pub(super) fn validate_claude_selection(
    config: &GatewayConfig,
    selection: &ClaudeModelSelection,
) -> Result<(), String> {
    for (label, id) in [
        ("default", &selection.default_model),
        ("haiku", &selection.haiku),
        ("sonnet", &selection.sonnet),
        ("opus", &selection.opus),
    ] {
        let id = id.trim();
        if id.is_empty() {
            return Err(format!("Claude {label} model must not be blank"));
        }
        if !gateway_protocol_servable(&config.providers, id, UpstreamProtocol::AnthropicMessages) {
            return Err(format!(
                "Claude {label} model '{id}' is not an enabled native Anthropic Messages mapping"
            ));
        }
    }
    Ok(())
}
