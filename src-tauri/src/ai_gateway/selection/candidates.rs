use super::super::{GatewayUpstreamProvider, UpstreamProtocol};

/// Outcome of resolving a request model against one provider for an inbound protocol.
#[derive(Debug)]
pub(in crate::ai_gateway) enum ModelResolution {
    /// Forward using this upstream model.
    Serve(String),
    /// A valid mapping row matches the requested model but declares another
    /// protocol. The provider must not serve this request and must never fall
    /// back to `default_model`; the payload carries the protocol it targets so
    /// the error can name the required endpoint.
    ProtocolMismatch(UpstreamProtocol),
    /// Neither a matching row nor an eligible default model.
    NoMatch,
}

pub(in crate::ai_gateway) fn can_convert_from(
    protocol: UpstreamProtocol,
    inbound: UpstreamProtocol,
) -> bool {
    protocol == inbound
        || (protocol == UpstreamProtocol::AnthropicMessages
            && matches!(
                inbound,
                UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
            ))
}

/// Resolve the upstream model for a provider under the inbound `protocol`.
///
/// Rows with an empty `upstream_model` are discarded and count as no match.
/// Disabled and auto-disabled rows never serve and never cause a protocol
/// mismatch, but a request that matches only such rows is a `NoMatch` and cannot
/// fall back to the default model. An enabled matching row is served with its own
/// remote model only when the row's effective protocol (its own declaration, else
/// the provider protocol) equals `protocol`; when enabled matching rows exist but
/// none matches, the request is a `ProtocolMismatch` and the default model is
/// not used as a fallback. The default model serves an unmapped model only when
/// the provider protocol itself matches.
pub(in crate::ai_gateway) fn resolve_model_for_protocol(
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> ModelResolution {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    let mut configured: Option<UpstreamProtocol> = None;
    let mut disabled_match = false;
    if let Some(requested) = requested {
        for mapping in provider.mappings.iter().filter(|mapping| {
            mapping.local_model.trim() == requested && !mapping.upstream_model.trim().is_empty()
        }) {
            // A disabled or auto-disabled row never serves and never produces a
            // protocol mismatch; it only records that the requested model is
            // mapped but switched off, which later blocks the default-model
            // fallback.
            if !mapping.enabled || mapping.auto_disabled {
                disabled_match = true;
                continue;
            }
            let effective = mapping.effective_protocol(provider.protocol);
            if can_convert_from(effective, protocol) {
                return ModelResolution::Serve(mapping.upstream_model.trim().to_string());
            }
            configured.get_or_insert(effective);
        }
    }
    if let Some(configured) = configured {
        return ModelResolution::ProtocolMismatch(configured);
    }
    if disabled_match {
        return ModelResolution::NoMatch;
    }
    provider
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|_| can_convert_from(provider.protocol, protocol))
        .map(|value| ModelResolution::Serve(value.to_string()))
        .unwrap_or(ModelResolution::NoMatch)
}

/// Resolve the actual protocol used by a served mapping. The inbound protocol
/// is deliberately separate: Chat and Responses may be converted to Messages,
/// while native Messages never widens to an OpenAI upstream.
pub(in crate::ai_gateway) fn resolve_upstream_protocol(
    provider: &GatewayUpstreamProvider,
    requested: Option<&str>,
    inbound: UpstreamProtocol,
) -> Option<UpstreamProtocol> {
    let requested = requested.map(str::trim).filter(|value| !value.is_empty());
    if let Some(requested) = requested {
        let mut mismatched = false;
        for mapping in provider.mappings.iter().filter(|mapping| {
            mapping.local_model.trim() == requested && !mapping.upstream_model.trim().is_empty()
        }) {
            if !mapping.enabled || mapping.auto_disabled {
                continue;
            }
            let effective = mapping.effective_protocol(provider.protocol);
            if can_convert_from(effective, inbound) {
                return Some(effective);
            }
            mismatched = true;
        }
        if mismatched {
            return None;
        }
    }
    if provider
        .default_model
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        && can_convert_from(provider.protocol, inbound)
    {
        return Some(provider.protocol);
    }
    None
}

/// Candidate set: enabled and able to serve the request model under the inbound
/// protocol. A provider is not filtered by its own protocol alone, because a
/// mapping row may declare the inbound protocol. The provider-level
/// `auto_disabled` field is legacy and never filters; only an auto-disabled row
/// removes the model it maps.
pub(in crate::ai_gateway) fn candidate_providers<'a>(
    providers: &'a [GatewayUpstreamProvider],
    requested: Option<&str>,
    protocol: UpstreamProtocol,
) -> Vec<&'a GatewayUpstreamProvider> {
    providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && matches!(
                    resolve_model_for_protocol(provider, requested, protocol),
                    ModelResolution::Serve(_)
                )
        })
        .collect()
}
