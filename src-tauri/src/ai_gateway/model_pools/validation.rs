use super::candidate::expand_fallback_model_in;
use super::planning::reserved_aliases;
use super::super::{
    GatewayConfig, GatewayUpstreamProvider, ModelPool, ModelPoolCandidate, UpstreamProtocol,
    MAX_PROVIDER_WEIGHT, MIN_PROVIDER_WEIGHT,
};

/// Whether one new candidate reference exists in the latest configuration: the
/// provider, the original trimmed mapping pair and its effective protocol.
pub(in crate::ai_gateway) fn reference_resolves(
    config: &GatewayConfig,
    candidate: &ModelPoolCandidate,
) -> bool {
    reference_resolves_in(&config.providers, candidate)
}

/// [`reference_resolves`] against an explicit provider list, so a catalog path
/// that already holds the provider slice never needs the full configuration.
pub(in crate::ai_gateway) fn reference_resolves_in(
    providers: &[GatewayUpstreamProvider],
    candidate: &ModelPoolCandidate,
) -> bool {
    providers.iter().any(|provider| {
        provider.id.trim() == candidate.provider_id.trim()
            && provider.mappings.iter().any(|mapping| {
                mapping.local_model.trim() == candidate.local_model.trim()
                    && mapping.upstream_model.trim() == candidate.upstream_model.trim()
                    && mapping.effective_protocol(provider.protocol) == candidate.protocol
            })
    })
}

fn candidate_tuple(candidate: &ModelPoolCandidate) -> (String, String, String, UpstreamProtocol) {
    (
        candidate.provider_id.trim().to_string(),
        candidate.local_model.trim().to_string(),
        candidate.upstream_model.trim().to_string(),
        candidate.protocol,
    )
}

/// Validate and normalize a submitted pool against the latest configuration.
///
/// A new reference must resolve against the latest providers; a reference that
/// was already stored stays valid even when its source later becomes
/// unavailable, so the operator can retain it or explicitly remove it. The
/// submitted alias must not collide with an ordinary mapping row or another
/// current pool, and every candidate needs a nonblank tuple and a weight in
/// 1..=100.
pub(in crate::ai_gateway) fn validate_pool_upsert(
    config: &GatewayConfig,
    incoming: &ModelPool,
) -> Result<ModelPool, String> {
    let mut pool = incoming.clone();
    pool.id = pool.id.trim().to_string();
    pool.name = pool.name.trim().to_string();
    pool.model = pool.model.trim().to_string();
    if pool.name.is_empty() {
        return Err("model pool name must not be blank".to_string());
    }
    if pool.model.is_empty() {
        return Err("model pool public model id must not be blank".to_string());
    }
    if pool.candidates.is_empty() {
        return Err("a model pool requires at least one candidate".to_string());
    }
    let mut seen: Vec<(String, String, String, UpstreamProtocol)> = Vec::new();
    for candidate in &mut pool.candidates {
        candidate.provider_id = candidate.provider_id.trim().to_string();
        candidate.local_model = candidate.local_model.trim().to_string();
        candidate.upstream_model = candidate.upstream_model.trim().to_string();
        if candidate.provider_id.is_empty() {
            return Err("a model pool candidate requires a provider id".to_string());
        }
        if candidate.local_model.is_empty() || candidate.upstream_model.is_empty() {
            return Err(
                "a model pool candidate requires a nonblank local and upstream model".to_string(),
            );
        }
        if candidate.weight < MIN_PROVIDER_WEIGHT || candidate.weight > MAX_PROVIDER_WEIGHT {
            return Err(format!(
                "candidate weight must be between {MIN_PROVIDER_WEIGHT} and {MAX_PROVIDER_WEIGHT}"
            ));
        }
        let tuple = candidate_tuple(candidate);
        if seen.contains(&tuple) {
            return Err(format!(
                "duplicate model pool candidate '{} -> {}'",
                candidate.local_model, candidate.upstream_model
            ));
        }
        seen.push(tuple);
    }
    // The ordered fallback list carries gateway local model ids only. Every
    // entry is trimmed, nonblank and unique, may not name a current or retired
    // pool alias, and a newly submitted entry must resolve to at least one
    // provider mapping. An unchanged saved entry stays retainable and clearable
    // after its source disappears.
    let forbidden = {
        let mut aliases = reserved_aliases(config);
        aliases.insert(pool.model.clone());
        aliases
    };
    let existing_pool = config
        .model_pools
        .iter()
        .find(|stored| stored.id == pool.id);
    let mut fallback_models: Vec<String> = Vec::with_capacity(pool.fallback_models.len());
    for model in &pool.fallback_models {
        let model = model.trim().to_string();
        if model.is_empty() {
            return Err("a model pool fallback model must not be blank".to_string());
        }
        if fallback_models.contains(&model) {
            return Err(format!(
                "duplicate model pool fallback model '{model}'"
            ));
        }
        if forbidden.contains(&model) {
            return Err(format!(
                "model pool fallback model '{model}' is a model-pool alias and cannot recurse into a pool"
            ));
        }
        if expand_fallback_model_in(&config.providers, &model).is_empty() {
            let retained = existing_pool.is_some_and(|stored| {
                stored
                    .fallback_models
                    .iter()
                    .any(|stored| stored.trim() == model)
            });
            if !retained {
                return Err(format!(
                    "model pool fallback model '{model}' is not served by any provider mapping"
                ));
            }
        }
        fallback_models.push(model);
    }
    pool.fallback_models = fallback_models;
    for provider in &config.providers {
        for mapping in &provider.mappings {
            if mapping.local_model.trim() == pool.model {
                return Err(format!(
                    "public model id '{}' conflicts with an existing provider mapping",
                    pool.model
                ));
            }
        }
    }
    for other in &config.model_pools {
        if other.id != pool.id && other.model.trim() == pool.model {
            return Err(format!(
                "public model id '{}' is already used by another model pool",
                pool.model
            ));
        }
    }
    let existing = existing_pool;
    for candidate in &pool.candidates {
        if reference_resolves(config, candidate) {
            continue;
        }
        let retained = existing.is_some_and(|stored| {
            stored
                .candidates
                .iter()
                .any(|other| candidate_tuple(other) == candidate_tuple(candidate))
        });
        if !retained {
            return Err(format!(
                "model pool candidate references an unknown provider/mapping/protocol: {} -> {} ({})",
                candidate.local_model,
                candidate.upstream_model,
                candidate.protocol.endpoint_path()
            ));
        }
    }
    super::super::reasoning::validate_pool_reasoning(config, existing, &pool)?;
    Ok(pool)
}

/// Push one retired alias into the reservation list without duplicating it.
pub(in crate::ai_gateway) fn reserve_alias(config: &mut GatewayConfig, alias: &str) {
    let alias = alias.trim();
    if alias.is_empty() {
        return;
    }
    if !config
        .model_pool_reserved_models
        .iter()
        .any(|reserved| reserved.trim() == alias)
    {
        config.model_pool_reserved_models.push(alias.to_string());
    }
}

/// Generate a stable pool identity.
pub(in crate::ai_gateway) fn new_pool_id() -> String {
    format!("gwpool-{}", uuid::Uuid::new_v4().simple())
}
