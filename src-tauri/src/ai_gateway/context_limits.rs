//! Conservative public context-window resolution.
//!
//! A public model may be served by several configured sources. Advertising a
//! single context window is only safe when every distinct source that can serve
//! the model under the ingress protocol declares a known positive value: the
//! advertised value is their minimum. An unknown or unresolved source makes the
//! whole aggregate unknown instead of borrowing a larger declared value. Runtime
//! health, cooldowns and disabled sources never raise the advertised bound, so
//! this resolver reads only the declared configuration and never touches
//! request routing, the filesystem or the network.

use super::model_pools::expand_fallback_model_in;
use super::selection::can_convert_from;
use super::{
    GatewayConfig, GatewayUpstreamProvider, ModelPool, ModelPoolCandidate, UpstreamProtocol,
};

/// One exact configured source identity: the owning provider, the public local
/// model, the upstream model and the effective protocol. The same identity is
/// counted once even when several pool candidates or fallback expansions repeat
/// it.
type SourceIdentity = (String, String, String, UpstreamProtocol);

/// Accumulate declared source values conservatively.
struct ContextAggregate {
    seen: Vec<SourceIdentity>,
    unknown: bool,
    minimum: Option<u32>,
}

impl ContextAggregate {
    fn new() -> Self {
        Self {
            seen: Vec::new(),
            unknown: false,
            minimum: None,
        }
    }

    /// Record one source. A repeated identity contributes once; an unknown
    /// declaration poisons the aggregate.
    fn add(&mut self, identity: SourceIdentity, value: Option<u32>) {
        if self.seen.iter().any(|existing| existing == &identity) {
            return;
        }
        self.seen.push(identity);
        match value.filter(|value| *value > 0) {
            Some(value) => {
                self.minimum = Some(self.minimum.map_or(value, |current| current.min(value)));
            }
            None => self.unknown = true,
        }
    }

    /// Record a configured source whose exact mapping could not be resolved.
    fn mark_unresolved(&mut self) {
        self.unknown = true;
    }

    /// The complete aggregate, or `None` when nothing contributed or any
    /// required source was unknown.
    fn finish(self) -> Option<u32> {
        if self.unknown || self.seen.is_empty() {
            None
        } else {
            self.minimum
        }
    }
}

/// Whether a pool candidate protocol can serve an OpenAI ingress. Pools are
/// fenced from native Anthropic ingress by the request planner; across the two
/// OpenAI endpoints their candidates may convert in either direction.
fn pool_candidate_serves(candidate: UpstreamProtocol, ingress: UpstreamProtocol) -> bool {
    let candidate_openai = matches!(
        candidate,
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
    );
    let ingress_openai = matches!(
        ingress,
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
    );
    can_convert_from(candidate, ingress) || (candidate_openai && ingress_openai)
}

/// Resolve the exact mapping a stored pool candidate references.
fn pool_candidate_context(
    providers: &[GatewayUpstreamProvider],
    candidate: &ModelPoolCandidate,
) -> Option<Option<u32>> {
    let provider_id = candidate.provider_id.trim();
    let local_model = candidate.local_model.trim();
    let upstream_model = candidate.upstream_model.trim();
    let provider = providers
        .iter()
        .find(|provider| provider.id == provider_id)?;
    let mapping = provider.mappings.iter().find(|mapping| {
        mapping.local_model.trim() == local_model
            && mapping.upstream_model.trim() == upstream_model
    })?;
    if mapping.effective_protocol(provider.protocol) != candidate.protocol {
        return None;
    }
    Some(mapping.context_window)
}

/// Aggregate every configured source of one pool alias.
fn pool_context_window(
    providers: &[GatewayUpstreamProvider],
    pool: &ModelPool,
    ingress: UpstreamProtocol,
) -> Option<u32> {
    if ingress == UpstreamProtocol::AnthropicMessages {
        return None;
    }
    if let Some(explicit) = pool.context_window.filter(|v| *v > 0) {
        return Some(explicit);
    }
    let mut aggregate = ContextAggregate::new();
    for candidate in &pool.candidates {
        if !pool_candidate_serves(candidate.protocol, ingress) {
            aggregate.mark_unresolved();
            continue;
        }
        let identity = (
            candidate.provider_id.trim().to_string(),
            candidate.local_model.trim().to_string(),
            candidate.upstream_model.trim().to_string(),
            candidate.protocol,
        );
        match pool_candidate_context(providers, candidate) {
            Some(value) => aggregate.add(identity, value),
            None => aggregate.mark_unresolved(),
        }
    }
    for model in &pool.fallback_models {
        let model = model.trim();
        if model.is_empty() {
            continue;
        }
        let expanded = expand_fallback_model_in(providers, model);
        if expanded.is_empty() {
            aggregate.mark_unresolved();
            continue;
        }
        for candidate in expanded {
            let identity = (
                candidate.provider_id.trim().to_string(),
                candidate.local_model.trim().to_string(),
                candidate.upstream_model.trim().to_string(),
                candidate.protocol,
            );
            match pool_candidate_context(providers, &candidate) {
                Some(value) => aggregate.add(identity, value),
                None => aggregate.mark_unresolved(),
            }
        }
    }
    aggregate.finish()
}

/// Aggregate every configured ordinary mapping that serves `public_model`
/// under `ingress`.
///
/// A public model with no explicit mapping is only reachable through a
/// provider's `default_model` fallback. That route has no per-public-model
/// declaration of its own, so it stays unknown rather than borrowing another
/// provider's value. When a provider's default model is itself the public model,
/// an exact same-provider mapping declaration supplies its value.
fn ordinary_context_window(
    providers: &[GatewayUpstreamProvider],
    public_model: &str,
    ingress: UpstreamProtocol,
) -> Option<u32> {
    let mut aggregate = ContextAggregate::new();
    let mut has_explicit_source = false;
    for provider in providers {
        for mapping in &provider.mappings {
            if mapping.local_model.trim() != public_model {
                continue;
            }
            let effective = mapping.effective_protocol(provider.protocol);
            if !can_convert_from(effective, ingress) {
                continue;
            }
            has_explicit_source = true;
            let identity = (
                provider.id.trim().to_string(),
                mapping.local_model.trim().to_string(),
                mapping.upstream_model.trim().to_string(),
                effective,
            );
            aggregate.add(identity, mapping.context_window);
        }
    }
    if !has_explicit_source {
        return None;
    }

    // A default-model route whose public id is the default model itself resolves
    // only from that same provider's exact upstream declaration.
    for provider in providers {
        let Some(default_model) = provider
            .default_model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if default_model != public_model {
            continue;
        }
        if !can_convert_from(provider.protocol, ingress) {
            continue;
        }
        let declared = provider
            .mappings
            .iter()
            .find(|mapping| mapping.upstream_model.trim() == default_model)
            .map(|mapping| mapping.context_window);
        match declared {
            Some(value) => aggregate.add(
                (
                    provider.id.trim().to_string(),
                    public_model.to_string(),
                    default_model.to_string(),
                    provider.protocol,
                ),
                value,
            ),
            None => aggregate.mark_unresolved(),
        }
    }
    aggregate.finish()
}

/// The conservative advertised total context window of `public_model` under
/// `protocol`, or `None` when the model is not available there or any required
/// configured source is unknown.
pub(in crate::ai_gateway) fn resolve_public_context_window_from(
    providers: &[GatewayUpstreamProvider],
    pools: &[ModelPool],
    public_model: &str,
    protocol: UpstreamProtocol,
) -> Option<u32> {
    let public_model = public_model.trim();
    if public_model.is_empty() {
        return None;
    }
    if let Some(pool) = pools
        .iter()
        .find(|pool| pool.model.trim() == public_model)
    {
        return pool_context_window(providers, pool, protocol);
    }
    ordinary_context_window(providers, public_model, protocol)
}

/// Resolve from a complete configuration.
pub(in crate::ai_gateway) fn resolve_public_context_window(
    config: &GatewayConfig,
    public_model: &str,
    protocol: UpstreamProtocol,
) -> Option<u32> {
    resolve_public_context_window_from(
        &config.providers,
        &config.model_pools,
        public_model,
        protocol,
    )
}
