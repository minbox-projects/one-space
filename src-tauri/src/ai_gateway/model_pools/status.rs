use super::candidate::{expand_fallback_model_in, resolve_candidate, resolve_pool_candidates};
use super::super::{now_ts, GatewayConfig, ModelPool, ModelPoolCandidate, UpstreamProtocol};
use serde::Serialize;

/// Nonsecret readiness of one stored pool candidate for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolCandidateStatus {
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub local_model: String,
    pub upstream_model: String,
    pub protocol: UpstreamProtocol,
    pub weight: u32,
    pub ready: bool,
    pub availability: String,
    pub reason: Option<String>,
    pub cooling_until: Option<u64>,
}

/// One canonical row of a pool's static reasoning coverage (REQ-005). The count
/// is configuration-derived and never reflects cooldown, key availability,
/// health or source enable state.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolReasoningCoverage {
    pub effort: String,
    pub mapped_candidates: usize,
    pub total_candidates: usize,
}

/// Nonsecret readiness of one expanded provider mapping serving a fallback
/// gateway model. It carries no key, endpoint, header or message body.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolFallbackSourceStatus {
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub local_model: String,
    pub upstream_model: String,
    pub protocol: UpstreamProtocol,
    pub ready: bool,
    pub availability: String,
    pub reason: Option<String>,
    pub cooling_until: Option<u64>,
}

/// Nonsecret readiness of one stored fallback gateway model and every provider
/// mapping that currently serves it, in the stored list order.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolFallbackModelStatus {
    /// The trimmed gateway local model id stored in the pool.
    pub model: String,
    /// Whether at least one expanded source can serve right now.
    pub ready: bool,
    /// Number of expanded sources that are ready.
    pub ready_sources: usize,
    /// Number of known provider mappings serving the model.
    pub total_sources: usize,
    pub sources: Vec<ModelPoolFallbackSourceStatus>,
    /// Bounded nonsecret explanation when no source is ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Nonsecret readiness of one stored pool for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPoolStatus {
    pub id: String,
    pub name: String,
    pub model: String,
    pub enabled: bool,
    /// Canonical pool default token (`auto` or a preference).
    pub default_reasoning_effort: String,
    /// Supported canonical preferences in canonical order.
    pub reasoning_efforts: Vec<String>,
    /// The five canonical coverage rows.
    pub reasoning_coverage: Vec<ModelPoolReasoningCoverage>,
    /// Regular candidate readiness; always regular-only.
    pub candidates: Vec<ModelPoolCandidateStatus>,
    /// Ordered per-model readiness of the stored fallback list. Empty when the
    /// pool configures no fallback model.
    pub fallback_models: Vec<ModelPoolFallbackModelStatus>,
}

/// Nonsecret status of one stored candidate reference. The regular and fallback
/// roles share this shape so their identity and availability stay comparable.
fn candidate_status(
    config: &GatewayConfig,
    stored: &ModelPoolCandidate,
    resolved: &super::candidate::ResolvedCandidate,
) -> ModelPoolCandidateStatus {
    let provider_name = config
        .providers
        .iter()
        .find(|provider| provider.id == resolved.identity.provider_id)
        .map(|provider| provider.name.clone());
    ModelPoolCandidateStatus {
        provider_id: stored.provider_id.trim().to_string(),
        provider_name,
        local_model: stored.local_model.trim().to_string(),
        upstream_model: stored.upstream_model.trim().to_string(),
        protocol: stored.protocol,
        weight: stored.weight,
        ready: resolved.available,
        availability: resolved.availability.to_string(),
        reason: resolved.reason.clone(),
        cooling_until: resolved.cooling_until,
    }
}

/// Nonsecret status of one expanded fallback source mapping.
fn fallback_source_status(
    config: &GatewayConfig,
    stored: &ModelPoolCandidate,
    resolved: &super::candidate::ResolvedCandidate,
) -> ModelPoolFallbackSourceStatus {
    let provider_name = config
        .providers
        .iter()
        .find(|provider| provider.id == resolved.identity.provider_id)
        .map(|provider| provider.name.clone());
    ModelPoolFallbackSourceStatus {
        provider_id: stored.provider_id.trim().to_string(),
        provider_name,
        local_model: stored.local_model.trim().to_string(),
        upstream_model: stored.upstream_model.trim().to_string(),
        protocol: stored.protocol,
        ready: resolved.available,
        availability: resolved.availability.to_string(),
        reason: resolved.reason.clone(),
        cooling_until: resolved.cooling_until,
    }
}

/// Build one fallback model's status record from every provider mapping that
/// serves it, independent of any request ingress.
fn fallback_model_status(
    config: &GatewayConfig,
    pool: &ModelPool,
    alias: &str,
    model: &str,
    now: u64,
) -> ModelPoolFallbackModelStatus {
    let model = model.trim().to_string();
    let expanded = expand_fallback_model_in(&config.providers, &model);
    let sources: Vec<ModelPoolFallbackSourceStatus> = expanded
        .iter()
        .map(|candidate| {
            let resolved = resolve_candidate(config, pool, alias, candidate, None, None, now);
            fallback_source_status(config, candidate, &resolved)
        })
        .collect();
    let ready_sources = sources.iter().filter(|source| source.ready).count();
    let total_sources = sources.len();
    let reason = if ready_sources > 0 {
        None
    } else if total_sources == 0 {
        Some("no provider mapping serves this fallback model".to_string())
    } else {
        sources
            .iter()
            .find_map(|source| source.reason.clone())
            .or_else(|| Some("no expanded source is currently available".to_string()))
    };
    ModelPoolFallbackModelStatus {
        model,
        ready: ready_sources > 0,
        ready_sources,
        total_sources,
        sources,
        reason,
    }
}

/// Build the nonsecret per-pool/per-candidate status from the current
/// configuration and the process-memory cooldown deadlines. No network or quota
/// probe is issued and nothing is persisted.
pub(in crate::ai_gateway) fn pool_status(config: &GatewayConfig) -> Vec<ModelPoolStatus> {
    let now = now_ts();
    config
        .model_pools
        .iter()
        .map(|pool| {
            let alias = pool.model.trim();
            let resolved = resolve_pool_candidates(config, pool, alias, None, None, now);
            let reasoning_catalog =
                super::super::reasoning::pool_reasoning_catalog(&config.providers, pool);
            let candidates = pool
                .candidates
                .iter()
                .zip(resolved.iter())
                .map(|(stored, resolved)| candidate_status(config, stored, resolved))
                .collect();
            // Each stored fallback model is resolved independently of any request
            // ingress, so its record reflects only its own expanded readiness.
            let fallback_models = pool
                .fallback_models
                .iter()
                .map(|model| fallback_model_status(config, pool, alias, model, now))
                .collect();
            ModelPoolStatus {
                id: pool.id.clone(),
                name: pool.name.trim().to_string(),
                model: alias.to_string(),
                enabled: pool.enabled,
                default_reasoning_effort: reasoning_catalog.default_reasoning_effort,
                reasoning_efforts: reasoning_catalog.reasoning_efforts,
                reasoning_coverage: reasoning_catalog
                    .reasoning_coverage
                    .into_iter()
                    .map(|row| ModelPoolReasoningCoverage {
                        effort: row.effort,
                        mapped_candidates: row.mapped_candidates,
                        total_candidates: row.total_candidates,
                    })
                    .collect(),
                candidates,
                fallback_models,
            }
        })
        .collect()
}
