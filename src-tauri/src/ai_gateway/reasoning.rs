//! Pool-owned reasoning policy validation (Step 1 of
//! `20261015-pool-only-reasoning`: REQ-001 / REQ-002, the config half of
//! AC-001..AC-004).
//!
//! This module owns the pool-local default and candidate native profile / value
//! set / budget range / mapping matrix introduced by
//! [`super::types_config`]. It validates a submitted pool inside the existing
//! atomic configuration transaction, so an invalid policy preserves the original
//! encrypted bytes and no source provider or sibling pool is touched.
//!
//! Only the deliberately bounded profile shapes are accepted (REQ-002). A
//! missing declaration stays unknown and is never inferred from source effort
//! labels. The Step 2 request-time parser, eligibility filter and native
//! injector consume these same types; this module deliberately holds no request
//! or transport logic yet.

use super::{
    GatewayConfig, ModelPool, ModelPoolCandidate, PoolCandidateReasoning,
    PoolDefaultReasoningEffort, PoolReasoningAction, PoolReasoningEffort, PoolReasoningProfile,
    UpstreamProtocol,
};

/// Minimum native Anthropic thinking budget the adapter accepts (REQ-002).
pub(in crate::ai_gateway) const MIN_MANUAL_REASONING_BUDGET_TOKENS: u32 = 1024;

/// Validate the pool-owned reasoning policy of one submitted pool against the
/// latest configuration.
///
/// `existing` is the stored pool with the same id, when the save edits one.
///
/// Rejects an unknown default, an unknown/unsupported candidate profile, a
/// positive preference mapped to a disable/omission, a `none` preference mapped
/// to a positive action, a disable mapping without declared support, an enum
/// target outside the declared value set or targeting native omission, a switch
/// positive other than `enabled`, a non-manual-budget profile declaring a budget
/// range, an inconsistent or too-small budget range and a budget mapping outside
/// its declared range.
///
/// A concrete (non-auto) default keeps the last-declaration guard: the same save
/// may not delete the default's last mapping. A *new or changed* concrete default
/// additionally requires at least one currently valid mapping, resolved against
/// the exact provider, full source pair and effective protocol, so a declaration
/// may not be blessed merely because its same-id candidate is retained. An
/// unchanged default whose declaration is left untouched may stay retained even
/// when its source later becomes unavailable.
pub(in crate::ai_gateway) fn validate_pool_reasoning(
    config: &GatewayConfig,
    existing: Option<&ModelPool>,
    pool: &ModelPool,
) -> Result<(), String> {
    if let PoolDefaultReasoningEffort::Unknown(raw) = &pool.default_reasoning_effort {
        return Err(format!("unknown default reasoning effort '{raw}'"));
    }
    for (index, candidate) in pool.candidates.iter().enumerate() {
        if let Some(reasoning) = candidate.reasoning.as_ref() {
            validate_candidate_reasoning(candidate, reasoning).map_err(|error| {
                format!(
                    "model pool candidate {} ('{} -> {}') reasoning policy is invalid: {error}",
                    index + 1,
                    candidate.local_model,
                    candidate.upstream_model
                )
            })?;
        }
    }
    validate_default_reasoning(config, existing, pool)
}

/// Guard the pool-owned concrete default (REQ-001 / AC-002).
fn validate_default_reasoning(
    config: &GatewayConfig,
    existing: Option<&ModelPool>,
    pool: &ModelPool,
) -> Result<(), String> {
    let Some(effort) = concrete_default(&pool.default_reasoning_effort) else {
        return Ok(());
    };
    // A save may not delete the retained concrete default's last declaration,
    // even when the default field itself is unchanged.
    if !pool_declares_effort(pool, effort) {
        return Err(format!(
            "the concrete default reasoning effort '{}' requires at least one candidate mapping for it",
            effort.as_str()
        ));
    }
    let currently_valid = pool_has_valid_effort_mapping(config, pool, effort);
    if existing.and_then(|stored| concrete_default(&stored.default_reasoning_effort)) == Some(effort)
    {
        // The default is retained. Its untouched declaration may survive an
        // unavailable source, but a declaration changed by this save still needs
        // a currently valid mapping.
        if !currently_valid && !declaration_unchanged(existing, pool, effort) {
            return Err(format!(
                "the changed '{}' declaration of the retained concrete default requires a currently valid candidate mapping",
                effort.as_str()
            ));
        }
        return Ok(());
    }
    // A new or changed concrete default requires a currently valid mapping.
    if !currently_valid {
        return Err(format!(
            "the concrete default reasoning effort '{}' requires at least one currently valid candidate mapping",
            effort.as_str()
        ));
    }
    Ok(())
}

/// Whether any candidate declares a mapping for `effort`.
fn pool_declares_effort(pool: &ModelPool, effort: PoolReasoningEffort) -> bool {
    pool.candidates.iter().any(|candidate| {
        candidate.reasoning.as_ref().is_some_and(|reasoning| {
            reasoning
                .mappings
                .iter()
                .any(|mapping| mapping.effort == effort)
        })
    })
}

/// Whether any candidate both declares a mapping for `effort` and currently
/// resolves against the exact provider, full source pair and effective protocol.
fn pool_has_valid_effort_mapping(
    config: &GatewayConfig,
    pool: &ModelPool,
    effort: PoolReasoningEffort,
) -> bool {
    pool.candidates.iter().any(|candidate| {
        super::model_pools::reference_resolves(config, candidate)
            && candidate
                .reasoning
                .as_ref()
                .is_some_and(|reasoning| {
                    reasoning
                        .mappings
                        .iter()
                        .any(|mapping| mapping.effort == effort)
                })
    })
}

/// Whether the submitted declaration(s) for `effort` are identical to the stored
/// ones, keyed by the full candidate identity so a moved or rewritten declaration
/// is detected.
fn declaration_unchanged(
    existing: Option<&ModelPool>,
    pool: &ModelPool,
    effort: PoolReasoningEffort,
) -> bool {
    existing.is_some_and(|stored| {
        effort_declarations(stored, effort) == effort_declarations(pool, effort)
    })
}

/// The mapping action declared for `effort` by each candidate, keyed by a stable
/// full-identity token and sorted so declaration order never affects the result.
fn effort_declarations(
    pool: &ModelPool,
    effort: PoolReasoningEffort,
) -> Vec<(String, PoolReasoningAction)> {
    let mut declarations = pool
        .candidates
        .iter()
        .filter_map(|candidate| {
            let action = candidate
                .reasoning
                .as_ref()?
                .mappings
                .iter()
                .find(|mapping| mapping.effort == effort)?;
            Some((candidate_identity_token(candidate), action.action.clone()))
        })
        .collect::<Vec<_>>();
    declarations.sort_by(|a, b| a.0.cmp(&b.0));
    declarations
}

/// Stable full-identity token for one candidate: provider id, trimmed source
/// pair and effective protocol.
fn candidate_identity_token(candidate: &ModelPoolCandidate) -> String {
    format!(
        "{}|{}|{}|{}",
        candidate.provider_id.trim(),
        candidate.local_model.trim(),
        candidate.upstream_model.trim(),
        candidate.protocol.endpoint_path(),
    )
}

/// Map a concrete (non-`auto`, non-unknown) default to the preference it
/// resolves to; `auto`/unknown have no mapping requirement.
fn concrete_default(default: &PoolDefaultReasoningEffort) -> Option<PoolReasoningEffort> {
    match default {
        PoolDefaultReasoningEffort::Low => Some(PoolReasoningEffort::Low),
        PoolDefaultReasoningEffort::Medium => Some(PoolReasoningEffort::Medium),
        PoolDefaultReasoningEffort::High => Some(PoolReasoningEffort::High),
        PoolDefaultReasoningEffort::Xhigh => Some(PoolReasoningEffort::Xhigh),
        PoolDefaultReasoningEffort::None => Some(PoolReasoningEffort::None),
        PoolDefaultReasoningEffort::Auto | PoolDefaultReasoningEffort::Unknown(_) => None,
    }
}

/// Validate one candidate's declared profile, range and mapping matrix against
/// its own stored effective protocol.
fn validate_candidate_reasoning(
    candidate: &ModelPoolCandidate,
    reasoning: &PoolCandidateReasoning,
) -> Result<(), String> {
    let profile = &reasoning.profile;
    if let PoolReasoningProfile::Unknown(raw) = profile {
        return Err(format!("unknown reasoning profile '{raw}'"));
    }
    if !profile_supports_protocol(profile, candidate.protocol) {
        return Err(format!(
            "reasoning profile '{}' is not supported on the '{}' protocol",
            profile.as_str(),
            candidate.protocol.endpoint_path()
        ));
    }
    if reasoning.budget.is_some() && !profile_accepts_budget(profile) {
        return Err(format!(
            "reasoning profile '{}' cannot declare a manual budget range",
            profile.as_str()
        ));
    }
    if let Some(budget) = reasoning.budget {
        if budget.min > budget.max {
            return Err(
                "the declared budget range is inverted (min is greater than max)".to_string(),
            );
        }
        if budget.min < MIN_MANUAL_REASONING_BUDGET_TOKENS {
            return Err(format!(
                "the declared budget minimum must be at least {MIN_MANUAL_REASONING_BUDGET_TOKENS} tokens"
            ));
        }
    }
    let mut seen: Vec<PoolReasoningEffort> = Vec::new();
    for mapping in &reasoning.mappings {
        if seen.contains(&mapping.effort) {
            return Err(format!(
                "duplicate reasoning mapping for the '{}' preference",
                mapping.effort.as_str()
            ));
        }
        seen.push(mapping.effort);
        if mapping.effort == PoolReasoningEffort::None {
            if !matches!(mapping.action, PoolReasoningAction::Disable) {
                return Err(
                    "the 'none' preference may only map to an explicit disable action".to_string(),
                );
            }
        } else {
            match &mapping.action {
                PoolReasoningAction::Disable => {
                    return Err(format!(
                        "the positive '{}' preference cannot map to disabled reasoning",
                        mapping.effort.as_str()
                    ));
                }
                PoolReasoningAction::Enum { value } => {
                    if !profile_accepts_enum(profile) {
                        return Err(format!(
                            "reasoning profile '{}' does not support an enum mapping",
                            profile.as_str()
                        ));
                    }
                    if !reasoning.values.iter().any(|declared| declared == value) {
                        return Err(format!(
                            "native value '{value}' is not in the candidate's declared value set"
                        ));
                    }
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        return Err(
                            "a positive preference cannot map to a blank native value".to_string(),
                        );
                    }
                    if trimmed.eq_ignore_ascii_case("none") {
                        return Err(format!(
                            "the positive '{}' preference cannot map to the native disable/omission value 'none'",
                            mapping.effort.as_str()
                        ));
                    }
                    if matches!(profile, PoolReasoningProfile::ChatThinkingSwitch)
                        && trimmed != "enabled"
                    {
                        return Err(
                            "a chat thinking switch profile's positive native value must be 'enabled'"
                                .to_string(),
                        );
                    }
                }
                PoolReasoningAction::Budget { budget_tokens } => {
                    if !profile_accepts_budget(profile) {
                        return Err(format!(
                            "reasoning profile '{}' does not support a budget mapping",
                            profile.as_str()
                        ));
                    }
                    let Some(budget) = reasoning.budget else {
                        return Err("a budget mapping requires a declared budget range".to_string());
                    };
                    if *budget_tokens < budget.min || *budget_tokens > budget.max {
                        return Err(format!(
                            "budget {budget_tokens} is outside the declared range {}..={}",
                            budget.min, budget.max
                        ));
                    }
                }
            }
        }
        if matches!(mapping.action, PoolReasoningAction::Disable) && !reasoning.supports_disable {
            return Err("a disable mapping requires supports_disable = true".to_string());
        }
    }
    Ok(())
}

/// Whether a profile may use an enum-shaped positive action.
fn profile_accepts_enum(profile: &PoolReasoningProfile) -> bool {
    matches!(
        profile,
        PoolReasoningProfile::OpenaiEffort
            | PoolReasoningProfile::AnthropicEffortInherit
            | PoolReasoningProfile::AnthropicEffortAdaptive
            | PoolReasoningProfile::ChatThinkingSwitch
    )
}

/// Whether a profile may use a manual-budget positive action.
fn profile_accepts_budget(profile: &PoolReasoningProfile) -> bool {
    matches!(profile, PoolReasoningProfile::AnthropicManualBudget)
}

/// Whether a profile's declared target endpoint family matches the candidate's
/// stored effective protocol.
fn profile_supports_protocol(profile: &PoolReasoningProfile, protocol: UpstreamProtocol) -> bool {
    match profile {
        PoolReasoningProfile::OpenaiEffort => matches!(
            protocol,
            UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
        ),
        PoolReasoningProfile::ChatThinkingSwitch => {
            matches!(protocol, UpstreamProtocol::ChatCompletions)
        }
        PoolReasoningProfile::AnthropicEffortInherit
        | PoolReasoningProfile::AnthropicEffortAdaptive
        | PoolReasoningProfile::AnthropicManualBudget => {
            matches!(protocol, UpstreamProtocol::AnthropicMessages)
        }
        PoolReasoningProfile::Unknown(_) => false,
    }
}
