//! Raw provider-template snapshot shape and tolerant parsing.

use super::super::types_config::{
    ErrorStrategy, OffPeakPrice, ProviderTemplate, ProviderTemplateModel, QuotaStrategy,
    SyncStrategy, TransportStrategy, UpstreamProtocol,
};
use serde::Deserialize;
use std::sync::OnceLock;

/// Raw template document shape, kept lenient so unknown protocol strings can be
/// dropped instead of aborting. Removed legacy fields (prices, off-peak windows,
/// reasoning efforts, snapshot version) are ignored as unknown fields.
#[derive(Deserialize)]
struct RawTemplate {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    sync_strategy: Option<String>,
    #[serde(default)]
    quota_strategy: Option<String>,
    #[serde(default)]
    error_strategy: Option<String>,
    #[serde(default)]
    transport_strategy: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub models_url: Option<String>,
    #[serde(default)]
    pub models: Vec<RawTemplateModel>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Deserialize)]
struct RawTemplateModel {
    #[serde(default)]
    upstream_model: String,
    #[serde(default)]
    local_model: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default = "crate::ai_gateway::types_config::default_true")]
    enabled: bool,
    #[serde(default)]
    input: f64,
    #[serde(default)]
    cache_read: f64,
    #[serde(default)]
    cache_write: f64,
    #[serde(default)]
    output: f64,
    #[serde(default)]
    off_peaks: Vec<OffPeakPrice>,
    #[serde(default)]
    reasoning_efforts: Vec<String>,
    #[serde(default)]
    context_window: Option<u32>,
}

fn parse_protocol(raw: &str) -> Option<UpstreamProtocol> {
    match raw {
        "chat_completions" => Some(UpstreamProtocol::ChatCompletions),
        "responses" => Some(UpstreamProtocol::Responses),
        "anthropic_messages" => Some(UpstreamProtocol::AnthropicMessages),
        _ => None,
    }
}

/// Tolerant read of a persisted sync strategy: only the closed known values are
/// honored and anything else (missing, unknown, legacy) resolves to `generic`.
fn parse_sync_strategy(raw: Option<&str>) -> SyncStrategy {
    match raw {
        Some("opencode_go") => SyncStrategy::OpencodeGo,
        _ => SyncStrategy::Generic,
    }
}

/// Tolerant read of a persisted quota strategy: only the closed known values are
/// honored and anything else resolves to `none`.
fn parse_quota_strategy(raw: Option<&str>) -> QuotaStrategy {
    match raw {
        Some("commandcode") => QuotaStrategy::Commandcode,
        Some("opencode_go") => QuotaStrategy::OpencodeGo,
        _ => QuotaStrategy::NoQuota,
    }
}

/// Tolerant read of a persisted error strategy: only `generic` exists today and
/// anything else resolves to it.
fn parse_error_strategy(_raw: Option<&str>) -> ErrorStrategy {
    ErrorStrategy::Generic
}

/// Tolerant read of a persisted transport strategy: only the closed known values
/// are honored and anything else (missing, unknown, legacy) resolves to direct.
fn parse_transport_strategy(raw: Option<&str>) -> TransportStrategy {
    match raw {
        Some("opencode_zen_sidecar") => TransportStrategy::OpencodeZenSidecar,
        _ => TransportStrategy::Direct,
    }
}

/// Parse a top-level JSON array of templates into the validated template model.
///
/// Structural failures (invalid JSON, an empty id or a duplicate template id)
/// return a readable error. Model-level problems (an empty identifier, an
/// unknown protocol string, a duplicate upstream name) are non-fatal: the model
/// is dropped, keeping the first occurrence.
pub fn parse_template_snapshot(json: &str) -> Result<Vec<ProviderTemplate>, String> {
    let raw: Vec<RawTemplate> = serde_json::from_str(json).map_err(|error| {
        format!("provider template snapshot is not a valid template array: {error}")
    })?;

    let mut templates: Vec<ProviderTemplate> = Vec::with_capacity(raw.len());
    let mut seen_ids: Vec<String> = Vec::with_capacity(raw.len());
    for item in raw {
        let id = item.id.trim().to_string();
        if id.is_empty() {
            return Err(
                "provider template snapshot contains a template with an empty id".to_string(),
            );
        }
        if seen_ids.iter().any(|existing| existing == &id) {
            return Err(format!(
                "provider template snapshot contains a duplicate template id: {id}"
            ));
        }
        seen_ids.push(id.clone());

        let mut models: Vec<ProviderTemplateModel> = Vec::with_capacity(item.models.len());
        for raw_model in item.models {
            let upstream_model = raw_model.upstream_model;
            if upstream_model.trim().is_empty() {
                continue;
            }
            let protocol = match raw_model.protocol.as_deref() {
                None => None,
                Some(raw) => match parse_protocol(raw) {
                    Some(protocol) => Some(protocol),
                    None => continue,
                },
            };
            if models
                .iter()
                .any(|existing| existing.upstream_model == upstream_model)
            {
                continue;
            }
            models.push(ProviderTemplateModel {
                upstream_model,
                local_model: raw_model.local_model,
                display_name: raw_model.display_name,
                protocol,
                enabled: raw_model.enabled,
                input: raw_model.input,
                cache_read: raw_model.cache_read,
                cache_write: raw_model.cache_write,
                output: raw_model.output,
                off_peaks: raw_model.off_peaks,
                reasoning_efforts: raw_model.reasoning_efforts,
                context_window: raw_model.context_window,
            });
        }

        let protocol = match item.protocol.as_deref() {
            Some(raw) => parse_protocol(raw).unwrap_or_default(),
            None => UpstreamProtocol::ChatCompletions,
        };

        let models_url = item
            .models_url
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let icon = item
            .icon
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        templates.push(ProviderTemplate {
            id,
            name: item.name,
            description: item.description,
            base_url: item.base_url,
            protocol,
            sync_strategy: parse_sync_strategy(item.sync_strategy.as_deref()),
            quota_strategy: parse_quota_strategy(item.quota_strategy.as_deref()),
            error_strategy: parse_error_strategy(item.error_strategy.as_deref()),
            transport_strategy: parse_transport_strategy(item.transport_strategy.as_deref()),
            source: item.source,
            models_url,
            models,
            icon,
        });
    }

    Ok(templates)
}

/// Parse the in-app snapshot embedded at compile time once per process. The
/// embedded bytes are immutable, so every caller receives an independent clone
/// of the same validated template list. Persisted template snapshots still take
/// precedence wherever the effective template is resolved.
pub fn builtin_templates() -> Result<Vec<ProviderTemplate>, String> {
    static BUILTIN_TEMPLATES: OnceLock<Result<Vec<ProviderTemplate>, String>> = OnceLock::new();
    BUILTIN_TEMPLATES
        .get_or_init(|| parse_template_snapshot(include_str!("../provider_templates.json")))
        .clone()
}

/// Resolve one built-in template by its id, naming the id in an actionable
/// error when it is unknown.
pub fn find_builtin_template(template_id: &str) -> Result<ProviderTemplate, String> {
    builtin_templates()?
        .into_iter()
        .find(|template| template.id == template_id)
        .ok_or_else(|| format!("unknown provider template id: {template_id}"))
}
