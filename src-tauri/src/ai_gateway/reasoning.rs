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
    GatewayConfig, GatewayUpstreamProvider, ModelPool, ModelPoolCandidate, PoolCandidateReasoning,
    PoolDefaultReasoningEffort, PoolNativeAction, PoolNativeReasoning, PoolReasoningAction,
    PoolReasoningEffort, PoolReasoningProfile, UpstreamProtocol,
};
use serde::Serialize;
use serde_json::Value;

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
    if let Some(fallback) = pool.fallback.as_ref() {
        if let Some(reasoning) = fallback.reasoning.as_ref() {
            validate_candidate_reasoning(fallback, reasoning).map_err(|error| {
                format!(
                    "model pool fallback ('{} -> {}') reasoning policy is invalid: {error}",
                    fallback.local_model, fallback.upstream_model
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
    if existing.and_then(|stored| concrete_default(&stored.default_reasoning_effort))
        == Some(effort)
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

/// The configured source references whose declarations form the pool's static
/// reasoning union: every regular candidate plus the optional fallback.
fn configured_candidates(pool: &ModelPool) -> impl Iterator<Item = &ModelPoolCandidate> {
    pool.candidates.iter().chain(pool.fallback.iter())
}

/// Whether any configured reference declares a mapping for `effort`.
fn pool_declares_effort(pool: &ModelPool, effort: PoolReasoningEffort) -> bool {
    configured_candidates(pool).any(|candidate| {
        candidate.reasoning.as_ref().is_some_and(|reasoning| {
            reasoning
                .mappings
                .iter()
                .any(|mapping| mapping.effort == effort)
        })
    })
}

/// Whether any configured reference both declares a mapping for `effort` and
/// currently resolves against the exact provider, full source pair and effective
/// protocol.
fn pool_has_valid_effort_mapping(
    config: &GatewayConfig,
    pool: &ModelPool,
    effort: PoolReasoningEffort,
) -> bool {
    configured_candidates(pool).any(|candidate| {
        super::model_pools::reference_resolves(config, candidate)
            && candidate.reasoning.as_ref().is_some_and(|reasoning| {
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
    let mut declarations = configured_candidates(pool)
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
        PoolDefaultReasoningEffort::Max => Some(PoolReasoningEffort::Max),
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
                            "a positive preference cannot map to a blank native value".to_string()
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

// ---------------------------------------------------------------------------
// Request-time parsing, classification and native injection (Step 2 of
// `20261015-pool-only-reasoning`: REQ-001/REQ-003/REQ-006, AC-001/AC-005/AC-006).
//
// Everything below runs only after an exact current pool alias is recognized on
// an OpenAI ingress. The parser and classifier never mutate scheduler, key,
// probe or health state, and the injector shapes the constructed body from the
// same resolved control recorded as attempt evidence.
// ---------------------------------------------------------------------------

/// The effective output cap of one pool request, used to filter Anthropic
/// manual-budget mappings. `field` names the client-facing output-limit field
/// the cap was taken from (or the target default field when the client omitted
/// it) so the incompatibility error can name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) struct PoolOutputCap {
    pub(in crate::ai_gateway) field: &'static str,
    pub(in crate::ai_gateway) tokens: u64,
}

/// The pool reasoning context of one inbound request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) struct PoolReasoningRequest {
    /// The canonical preference the client explicitly requested, if any.
    pub(in crate::ai_gateway) requested: Option<PoolReasoningEffort>,
    pub(in crate::ai_gateway) cap: PoolOutputCap,
}

/// The request-time reasoning state of one pool candidate.
pub(in crate::ai_gateway) enum CandidateReasoning {
    /// The candidate stores no mapping for the resolved preference and is
    /// excluded from the request.
    Unmapped,
    /// The declaration exists but cannot serve against the current source
    /// identity/protocol or declared profile; the reason is nonsecret.
    Invalid(String),
    /// The mapping is valid but its native budget is not strictly below the
    /// request's effective output cap.
    CapIncompatible { budget: u32 },
    /// The mapping is valid and usable; its wire control and evidence.
    Control(PoolNativeReasoning),
}

fn field_error(field: impl AsRef<str>, message: impl Into<String>) -> String {
    format!("field '{}': {}", field.as_ref(), message.into())
}

/// Parse the recognized pool reasoning entry control of one OpenAI ingress path.
///
/// Chat accepts `reasoning_effort` and the compatibility spelling
/// `reasoningEffort`; Responses accepts `reasoning.effort`. Values are trimmed
/// and checked against the canonical lowercase set; missing, null and blank mean
/// unspecified. An unknown identifier, a literal `auto`, a case variant, a
/// non-string value, conflicting Chat spellings and an opaque/extra Responses
/// `reasoning` member are field-named errors returned before any state change.
pub(in crate::ai_gateway) fn parse_pool_request_effort(
    path: &str,
    body: &Value,
) -> Result<Option<PoolReasoningEffort>, String> {
    match path.split('?').next().unwrap_or(path) {
        "/v1/responses" => parse_responses_effort(body),
        _ => parse_chat_effort(body),
    }
}

fn parse_chat_effort(body: &Value) -> Result<Option<PoolReasoningEffort>, String> {
    let mut agreed: Option<(&str, &'static str)> = None;
    for field in ["reasoning_effort", "reasoningEffort"] {
        let Some(raw) = body.get(field) else {
            continue;
        };
        if raw.is_null() {
            continue;
        }
        let Some(text) = raw.as_str() else {
            return Err(field_error(field, "must be a string"));
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        match agreed {
            None => agreed = Some((trimmed, field)),
            Some((existing, _)) if existing == trimmed => {}
            Some(_) => {
                return Err(field_error(
                    "reasoning",
                    "conflicting reasoning effort spellings",
                ))
            }
        }
    }
    // The nested Responses `reasoning` object is not a Chat Completions control.
    if body.get("reasoning").is_some_and(|value| !value.is_null()) {
        return Err(field_error(
            "reasoning",
            "is not a Chat Completions reasoning control",
        ));
    }
    match agreed {
        Some((value, field)) => Ok(Some(canonical_effort(value, field)?)),
        None => Ok(None),
    }
}

fn parse_responses_effort(body: &Value) -> Result<Option<PoolReasoningEffort>, String> {
    let Some(reasoning) = body.get("reasoning") else {
        return Ok(None);
    };
    if reasoning.is_null() {
        return Ok(None);
    }
    let Some(object) = reasoning.as_object() else {
        return Err(field_error(
            "reasoning",
            "must be an object with an effort field; opaque reasoning state is not supported",
        ));
    };
    for key in object.keys() {
        if key != "effort" {
            return Err(field_error(
                "reasoning",
                "opaque reasoning state is not supported by the model-pool contract",
            ));
        }
    }
    let Some(effort) = object.get("effort") else {
        return Ok(None);
    };
    if effort.is_null() {
        return Ok(None);
    }
    let Some(text) = effort.as_str() else {
        return Err(field_error("reasoning", "effort must be a string"));
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(canonical_effort(trimmed, "reasoning")?))
}

fn canonical_effort(value: &str, field: &str) -> Result<PoolReasoningEffort, String> {
    match value {
        "low" => Ok(PoolReasoningEffort::Low),
        "medium" => Ok(PoolReasoningEffort::Medium),
        "high" => Ok(PoolReasoningEffort::High),
        "xhigh" => Ok(PoolReasoningEffort::Xhigh),
        "max" => Ok(PoolReasoningEffort::Max),
        "none" => Ok(PoolReasoningEffort::None),
        other => Err(field_error(
            field,
            format!("unknown reasoning effort '{other}'"),
        )),
    }
}

/// The reasoning entry controls recognized on one OpenAI ingress path, matching
/// [`parse_pool_request_effort`]'s ingress classification. Chat Completions
/// recognizes exactly `reasoning_effort`/`reasoningEffort`; Responses recognizes
/// exactly the nested `reasoning` control. Any other spelling is an unrecognized
/// field the portable contract must still validate, so it is deliberately never
/// stripped.
fn recognized_entry_controls(path: &str) -> &'static [&'static str] {
    match path.split('?').next().unwrap_or(path) {
        "/v1/responses" => &["reasoning"],
        _ => &["reasoning_effort", "reasoningEffort"],
    }
}

/// Remove only the recognized pool reasoning entry controls of one ingress from
/// a request body, so the clean portable body can be validated and converted by
/// the existing adapters while any unrecognized reasoning field keeps its
/// ordinary portable-contract rejection.
pub(in crate::ai_gateway) fn strip_pool_entry_controls(path: &str, body: &Value) -> Value {
    let Some(object) = body.as_object() else {
        return body.clone();
    };
    let mut cleaned = object.clone();
    for field in recognized_entry_controls(path) {
        cleaned.remove(*field);
    }
    Value::Object(cleaned)
}

/// Byte form of [`strip_pool_entry_controls`] for the forwarding path.
pub(in crate::ai_gateway) fn strip_pool_entry_controls_bytes(
    path: &str,
    body: &[u8],
) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    serde_json::to_vec(&strip_pool_entry_controls(path, &value)).map_err(|error| error.to_string())
}

/// Merge one candidate's resolved native control into the constructed body.
/// `auto` (`provider_default`) carries no parameters and leaves the body
/// unchanged.
pub(in crate::ai_gateway) fn inject_native_controls(
    body: Vec<u8>,
    native: &PoolNativeReasoning,
) -> Result<Vec<u8>, String> {
    let parameters = native.wire_parameters();
    if parameters.is_empty() {
        return Ok(body);
    }
    let mut value: Value = serde_json::from_slice(&body).map_err(|error| error.to_string())?;
    let Some(object) = value.as_object_mut() else {
        return Err("request body must be a JSON object".to_string());
    };
    for (key, control) in parameters {
        object.insert(key, control);
    }
    serde_json::to_vec(&value).map_err(|error| error.to_string())
}

/// Resolve the request's preference: an explicit canonical preference wins,
/// otherwise the pool default maps to a concrete preference or `auto`.
pub(in crate::ai_gateway) fn resolve_pool_preference(
    pool: &ModelPool,
    requested: Option<PoolReasoningEffort>,
) -> Option<PoolReasoningEffort> {
    requested.or_else(|| concrete_default(&pool.default_reasoning_effort))
}

/// Canonical resolved token for logs and evidence: `auto` or a preference.
pub(in crate::ai_gateway) fn resolved_effort_token(
    pool: &ModelPool,
    requested: Option<PoolReasoningEffort>,
) -> String {
    match resolve_pool_preference(pool, requested) {
        Some(preference) => preference.as_str().to_string(),
        None => "auto".to_string(),
    }
}

/// Classify one candidate against the resolved preference and the request's
/// effective output cap without mutating any state.
pub(in crate::ai_gateway) fn classify_candidate_reasoning(
    config: &GatewayConfig,
    candidate: &ModelPoolCandidate,
    preference: PoolReasoningEffort,
    cap: &PoolOutputCap,
) -> CandidateReasoning {
    let Some(declared) = candidate.reasoning.as_ref() else {
        return CandidateReasoning::Unmapped;
    };
    let Some(mapping) = declared
        .mappings
        .iter()
        .find(|mapping| mapping.effort == preference)
    else {
        return CandidateReasoning::Unmapped;
    };
    if preference == PoolReasoningEffort::None
        && !matches!(&mapping.action, PoolReasoningAction::Disable)
    {
        return CandidateReasoning::Invalid(candidate_reason(
            candidate,
            "the 'none' preference must map to an explicit disable action",
        ));
    }
    if !super::model_pools::reference_resolves(config, candidate) {
        return CandidateReasoning::Invalid(candidate_reason(
            candidate,
            "source mapping is missing or changed",
        ));
    }
    if let PoolReasoningProfile::Unknown(raw) = &declared.profile {
        return CandidateReasoning::Invalid(candidate_reason(
            candidate,
            &format!("unknown reasoning profile '{raw}'"),
        ));
    }
    let profile = &declared.profile;
    if !profile_supports_protocol(profile, candidate.protocol) {
        return CandidateReasoning::Invalid(candidate_reason(
            candidate,
            &format!(
                "reasoning profile '{}' is not supported on the '{}' protocol",
                profile.as_str(),
                candidate.protocol.endpoint_path()
            ),
        ));
    }
    match &mapping.action {
        PoolReasoningAction::Enum { value } => {
            // The declared value set and the positive/omission boundaries are
            // enforced at save time by [`validate_pool_reasoning`]; the request
            // path trusts the stored mapping and only requires the profile to be
            // representable on the candidate's target protocol.
            if !profile_accepts_enum(profile) {
                return CandidateReasoning::Invalid(candidate_reason(
                    candidate,
                    "the profile does not support an enum mapping",
                ));
            }
            CandidateReasoning::Control(PoolNativeReasoning {
                resolved: preference.as_str().to_string(),
                protocol: candidate.protocol,
                action: PoolNativeAction::Enum {
                    profile: profile.clone(),
                    value: value.trim().to_string(),
                },
            })
        }
        PoolReasoningAction::Budget { budget_tokens } => {
            if !matches!(profile, PoolReasoningProfile::AnthropicManualBudget) {
                return CandidateReasoning::Invalid(candidate_reason(
                    candidate,
                    "the profile does not support a budget mapping",
                ));
            }
            let Some(range) = declared.budget else {
                return CandidateReasoning::Invalid(candidate_reason(
                    candidate,
                    "a budget mapping requires a declared budget range",
                ));
            };
            if range.min > range.max || range.min < MIN_MANUAL_REASONING_BUDGET_TOKENS {
                return CandidateReasoning::Invalid(candidate_reason(
                    candidate,
                    "the declared budget range is inconsistent or below the adapter minimum",
                ));
            }
            if *budget_tokens < range.min || *budget_tokens > range.max {
                return CandidateReasoning::Invalid(candidate_reason(
                    candidate,
                    "the budget token is outside the declared range",
                ));
            }
            if u64::from(*budget_tokens) >= cap.tokens {
                return CandidateReasoning::CapIncompatible {
                    budget: *budget_tokens,
                };
            }
            CandidateReasoning::Control(PoolNativeReasoning {
                resolved: preference.as_str().to_string(),
                protocol: candidate.protocol,
                action: PoolNativeAction::Budget {
                    budget_tokens: *budget_tokens,
                },
            })
        }
        PoolReasoningAction::Disable => {
            if preference != PoolReasoningEffort::None {
                return CandidateReasoning::Invalid(candidate_reason(
                    candidate,
                    "a disable mapping may only serve the 'none' preference",
                ));
            }
            if !declared.supports_disable {
                return CandidateReasoning::Invalid(candidate_reason(
                    candidate,
                    "a disable mapping requires supports_disable = true",
                ));
            }
            CandidateReasoning::Control(PoolNativeReasoning {
                resolved: preference.as_str().to_string(),
                protocol: candidate.protocol,
                action: PoolNativeAction::Disable {
                    profile: profile.clone(),
                },
            })
        }
    }
}

/// A bounded, nonsecret reason naming one candidate's source identity.
fn candidate_reason(candidate: &ModelPoolCandidate, message: &str) -> String {
    format!(
        "{} -> {}: {message}",
        candidate.local_model.trim(),
        candidate.upstream_model.trim()
    )
}

// ---------------------------------------------------------------------------
// Static pool reasoning capability catalogs (Step 3 of
// `20261015-pool-only-reasoning`: REQ-005/REQ-007, AC-009/AC-012).
//
// Discovery rows, pool status rows and terminal pool rows share these helpers so
// the advertised support set, default and coverage are identical everywhere and
// derive only from the stored configuration. Temporary readiness (cooldown, key
// availability, health, source enable state) and the request-time output cap
// never enter the count.
// ---------------------------------------------------------------------------

/// The canonical public pool preferences in canonical order. Every catalog
/// surface (`reasoning_efforts`, coverage rows, OpenCode variants) derives its
/// order from this list.
pub(in crate::ai_gateway) const CANONICAL_POOL_EFFORTS: [PoolReasoningEffort; 6] = [
    PoolReasoningEffort::Low,
    PoolReasoningEffort::Medium,
    PoolReasoningEffort::High,
    PoolReasoningEffort::Xhigh,
    PoolReasoningEffort::Max,
    PoolReasoningEffort::None,
];

/// One configuration-derived row of a pool's static reasoning coverage table.
#[derive(Debug, Clone, Serialize)]
pub(in crate::ai_gateway) struct PoolReasoningCoverage {
    pub(in crate::ai_gateway) effort: String,
    pub(in crate::ai_gateway) mapped_candidates: usize,
    pub(in crate::ai_gateway) total_candidates: usize,
}

/// The static, configuration-derived reasoning capability of one pool: the
/// supported canonical preferences in canonical order, the canonical default and
/// the five canonical coverage rows.
#[derive(Debug, Clone)]
pub(in crate::ai_gateway) struct PoolReasoningCatalog {
    pub(in crate::ai_gateway) reasoning_efforts: Vec<String>,
    pub(in crate::ai_gateway) default_reasoning_effort: String,
    pub(in crate::ai_gateway) reasoning_coverage: Vec<PoolReasoningCoverage>,
}

/// Whether one candidate statically serves `effort`: its exact source identity
/// currently resolves and it declares a mapping the candidate's own profile makes
/// valid. Runtime readiness and the request output cap are deliberately excluded,
/// so the advertised support set and coverage stay stable through cooldown, key
/// exhaustion, health changes and source enable toggles.
pub(in crate::ai_gateway) fn candidate_serves_effort(
    providers: &[GatewayUpstreamProvider],
    candidate: &ModelPoolCandidate,
    effort: PoolReasoningEffort,
) -> bool {
    let Some(declared) = candidate.reasoning.as_ref() else {
        return false;
    };
    let Some(mapping) = declared
        .mappings
        .iter()
        .find(|mapping| mapping.effort == effort)
    else {
        return false;
    };
    if !super::model_pools::reference_resolves_in(providers, candidate) {
        return false;
    }
    if !profile_supports_protocol(&declared.profile, candidate.protocol) {
        return false;
    }
    if effort == PoolReasoningEffort::None {
        return matches!(mapping.action, PoolReasoningAction::Disable) && declared.supports_disable;
    }
    match &mapping.action {
        // A positive preference can never be a disable action.
        PoolReasoningAction::Disable => false,
        PoolReasoningAction::Enum { value } => {
            if !profile_accepts_enum(&declared.profile) {
                return false;
            }
            let trimmed = value.trim();
            !trimmed.is_empty()
                && !trimmed.eq_ignore_ascii_case("none")
                && !(matches!(declared.profile, PoolReasoningProfile::ChatThinkingSwitch)
                    && trimmed != "enabled")
        }
        PoolReasoningAction::Budget { budget_tokens } => {
            if !matches!(
                declared.profile,
                PoolReasoningProfile::AnthropicManualBudget
            ) {
                return false;
            }
            let Some(range) = declared.budget else {
                return false;
            };
            range.min <= range.max
                && range.min >= MIN_MANUAL_REASONING_BUDGET_TOKENS
                && *budget_tokens >= range.min
                && *budget_tokens <= range.max
        }
    }
}

/// Derive one pool's static reasoning catalog from the current provider list.
///
/// The support set and coverage count the configured union of regular candidates
/// and the optional fallback, while the regular candidate rows themselves remain
/// regular-only. Static support never consults transient readiness.
pub(in crate::ai_gateway) fn pool_reasoning_catalog(
    providers: &[GatewayUpstreamProvider],
    pool: &ModelPool,
) -> PoolReasoningCatalog {
    let configured: Vec<&ModelPoolCandidate> = configured_candidates(pool).collect();
    let total_candidates = configured.len();
    let reasoning_coverage: Vec<PoolReasoningCoverage> = CANONICAL_POOL_EFFORTS
        .iter()
        .map(|effort| PoolReasoningCoverage {
            effort: effort.as_str().to_string(),
            mapped_candidates: configured
                .iter()
                .filter(|candidate| candidate_serves_effort(providers, candidate, *effort))
                .count(),
            total_candidates,
        })
        .collect();
    let reasoning_efforts = reasoning_coverage
        .iter()
        .filter(|row| row.mapped_candidates > 0)
        .map(|row| row.effort.clone())
        .collect();
    PoolReasoningCatalog {
        reasoning_efforts,
        default_reasoning_effort: pool.default_reasoning_effort.as_str().to_string(),
        reasoning_coverage,
    }
}
