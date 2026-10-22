//! Provider template snapshot loading and validation.
//!
//! The app ships `provider_templates.json` embedded via `include_str!`; this
//! module parses that snapshot (or a later network payload) into the public
//! [`ProviderTemplate`] model, dropping what the gateway cannot serve instead of
//! failing the whole document. Only structural problems (invalid JSON, an empty
//! or duplicate template id) are fatal.

use super::storage::new_provider_id;
use super::types_config::{
    now_ts, ErrorStrategy, GatewayConfig, GatewayUpstreamProvider, ModelMapping, ModelPrice,
    OffPeakPrice, ProviderTemplate, ProviderTemplateModel, ProviderTemplateState, QuotaStrategy,
    SyncStrategy, UpstreamKey, UpstreamProtocol,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

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
    #[serde(default = "super::types_config::default_true")]
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
        .get_or_init(|| parse_template_snapshot(include_str!("provider_templates.json")))
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

// ---------------------------------------------------------------------------
// Step 3: template query, sync and incremental propagation
// ---------------------------------------------------------------------------

/// A template plus the origin of its current data, returned by the query and
/// sync commands.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ProviderTemplateView {
    pub template: ProviderTemplate,
    /// Last successful sync time, absent while the built-in snapshot is used.
    pub synced_at: Option<u64>,
    /// Human-readable data source (the snapshot's or the last sync's).
    pub source: String,
    /// True while the view falls back to the in-app snapshot.
    pub from_snapshot: bool,
}

/// One provider whose derived mappings a template sync removed: the removed
/// `upstream_model` values in mapping row order without duplicates. Emitted once
/// per bound provider with at least one removal, including a provider whose only
/// removals were already-disabled legacy rows. The provider name is the stored
/// name after propagation; no key value is ever carried.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct ProviderRetirementNotice {
    pub provider_id: String,
    pub provider_name: String,
    pub retired_models: Vec<String>,
}

/// One provider whose template sync added model mappings: the added models in
/// mapping row order without duplicates, named by their trimmed `local_model`
/// when non-empty, otherwise by their `upstream_model`. A provider with no
/// addition yields no notice; no key value is ever carried.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct TemplateAdditionNotice {
    pub provider_name: String,
    pub added_models: Vec<String>,
}

/// Diff the providers bound to `template_id` between the pre-sync `before` and
/// the post-sync `after` configuration, returning one notice per provider that
/// gained at least one mapping. Only a mapping whose `upstream_model` was absent
/// from the same provider before the sync qualifies; a provider not bound to the
/// template is ignored. The provider list and mapping order of `after` define
/// the output order.
pub(in crate::ai_gateway) fn template_sync_addition_notices(
    before: &GatewayConfig,
    after: &GatewayConfig,
    template_id: &str,
) -> Vec<TemplateAdditionNotice> {
    let mut notices = Vec::new();
    for provider in after
        .providers
        .iter()
        .filter(|provider| provider.template_id.as_deref() == Some(template_id))
    {
        let previous_upstream: Vec<&str> = before
            .providers
            .iter()
            .find(|candidate| candidate.id == provider.id)
            .map(|previous| {
                previous
                    .mappings
                    .iter()
                    .map(|mapping| mapping.upstream_model.as_str())
                    .collect()
            })
            .unwrap_or_default();

        let mut added_models: Vec<String> = Vec::new();
        for mapping in &provider.mappings {
            if previous_upstream
                .iter()
                .any(|upstream| *upstream == mapping.upstream_model)
            {
                continue;
            }
            let local_model = mapping.local_model.trim();
            added_models.push(if local_model.is_empty() {
                mapping.upstream_model.clone()
            } else {
                local_model.to_string()
            });
        }

        if !added_models.is_empty() {
            notices.push(TemplateAdditionNotice {
                provider_name: provider.name.clone(),
                added_models,
            });
        }
    }
    notices
}

/// Resolve the effective template data for `template_id`: the last persisted
/// sync result when present, else the built-in snapshot. An unknown id is an
/// actionable error naming the id.
pub(in crate::ai_gateway) fn effective_template(
    config: &GatewayConfig,
    template_id: &str,
) -> Result<ProviderTemplate, String> {
    if config
        .deleted_template_ids
        .iter()
        .any(|id| id == template_id)
    {
        return Err(format!("Template '{template_id}' has been deleted"));
    }
    if let Some(template) = config
        .provider_templates
        .iter()
        .find(|state| state.template_id == template_id)
        .and_then(|state| state.template.clone())
    {
        return Ok(template);
    }
    find_builtin_template(template_id)
}

/// Resolve the effective template for a provider binding, or `None` when the
/// provider is unbound or its stored `template_id` no longer resolves (unknown
/// or deleted). A missing/broken binding yields the generic policy at runtime
/// through the callers' defaults; no URL, name or icon is ever consulted.
pub(in crate::ai_gateway) fn effective_provider_template(
    config: &GatewayConfig,
    provider: &GatewayUpstreamProvider,
) -> Option<ProviderTemplate> {
    provider
        .template_id
        .as_deref()
        .and_then(|template_id| effective_template(config, template_id).ok())
}

/// The effective quota strategy for a provider: its bound template's declared
/// strategy, else `none` for an unbound, unknown or deleted binding.
pub(in crate::ai_gateway) fn effective_quota_strategy(
    config: &GatewayConfig,
    provider: &GatewayUpstreamProvider,
) -> QuotaStrategy {
    effective_provider_template(config, provider)
        .map(|template| template.quota_strategy)
        .unwrap_or_default()
}

/// Build the ordered view of provider templates: persisted sync/custom results
/// win, deleted built-in templates are skipped, and user custom templates are appended.
pub fn provider_template_views(
    config: &GatewayConfig,
) -> Result<Vec<ProviderTemplateView>, String> {
    let mut views = Vec::new();
    let builtins = builtin_templates()?;
    for snapshot in &builtins {
        if config
            .deleted_template_ids
            .iter()
            .any(|id| id == &snapshot.id)
        {
            continue;
        }
        match config
            .provider_templates
            .iter()
            .find(|state| state.template_id == snapshot.id)
            .and_then(|state| {
                state
                    .template
                    .clone()
                    .map(|template| (template, state))
            }) {
            Some((template, state)) => views.push(ProviderTemplateView {
                template,
                synced_at: state.synced_at,
                source: state
                    .source
                    .clone()
                    .filter(|source| !source.trim().is_empty())
                    .unwrap_or_else(|| snapshot.source.clone()),
                from_snapshot: false,
            }),
            None => views.push(ProviderTemplateView {
                template: snapshot.clone(),
                synced_at: None,
                source: snapshot.source.clone(),
                from_snapshot: true,
            }),
        }
    }

    for state in &config.provider_templates {
        if builtins.iter().any(|b| b.id == state.template_id) {
            continue;
        }
        if config
            .deleted_template_ids
            .iter()
            .any(|id| id == &state.template_id)
        {
            continue;
        }
        if let Some(template) = &state.template {
            views.push(ProviderTemplateView {
                template: template.clone(),
                synced_at: state.synced_at,
                source: state
                    .source
                    .clone()
                    .filter(|source| !source.trim().is_empty())
                    .unwrap_or_else(|| template.source.clone()),
                from_snapshot: false,
            });
        }
    }

    Ok(views)
}

/// Transform a raw identifier part into display words: runs of characters
/// outside `[A-Za-z0-9.]` become single spaces, edge dots are trimmed, and a
/// leading lowercase ASCII letter of each word is upper-cased. Keeping `.`
/// inside words leaves version numbers such as `2.1` or `K2.7` intact.
fn transform_identifier_part(raw: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for candidate in raw.chars() {
        if candidate.is_ascii_alphanumeric() || candidate == '.' {
            current.push(candidate);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for word in words {
        let trimmed = word.trim_matches('.');
        if trimmed.is_empty() {
            continue;
        }
        let mut word = trimmed.to_string();
        if let Some(first) = word.chars().next() {
            if first.is_ascii_lowercase() {
                word.replace_range(..first.len_utf8(), &first.to_ascii_uppercase().to_string());
            }
        }
        out.push(word);
    }
    out.join(" ")
}

/// Complete a synced display name so it covers the upstream identifier.
///
/// The compared segment is the substring after the identifier's last `/` (or
/// the whole identifier when that substring is empty); the vendor prefix is
/// never appended. Coverage matches the segment's ASCII alphanumerics in
/// order against the base's, case-insensitively, and the raw remainder
/// starting right after the last consumed character is transformed and
/// appended. A missing or blank base transforms the whole segment; a base
/// that already covers the segment is returned trimmed and verbatim, so
/// applying the same response twice is byte-identical.
fn complete_model_display_name(base: Option<&str>, identifier: &str) -> Option<String> {
    let segment = match identifier.rsplit('/').next() {
        Some(segment) if !segment.is_empty() => segment,
        _ => identifier,
    };
    let base = base.map(str::trim).filter(|base| !base.is_empty());
    let Some(base) = base else {
        let transformed = transform_identifier_part(segment);
        return if transformed.is_empty() {
            None
        } else {
            Some(transformed)
        };
    };
    let wanted: Vec<char> = segment
        .chars()
        .filter(|candidate| candidate.is_ascii_alphanumeric())
        .map(|candidate| candidate.to_ascii_lowercase())
        .collect();
    let mut available = base
        .chars()
        .filter(|candidate| candidate.is_ascii_alphanumeric())
        .map(|candidate| candidate.to_ascii_lowercase());
    let mut consumed = 0usize;
    for wanted in &wanted {
        if available.any(|candidate| candidate == *wanted) {
            consumed += 1;
        } else {
            break;
        }
    }
    if consumed == wanted.len() {
        return Some(base.to_string());
    }
    // Byte offset in the raw segment right after the last consumed
    // alphanumeric character (0 when nothing was consumed).
    let mut seen = 0usize;
    let mut offset = 0usize;
    for (index, candidate) in segment.char_indices() {
        if candidate.is_ascii_alphanumeric() {
            seen += 1;
            if seen > consumed {
                break;
            }
            offset = index + candidate.len_utf8();
        }
    }
    let transformed = transform_identifier_part(&segment[offset..]);
    if transformed.is_empty() {
        return Some(base.to_string());
    }
    Some(format!("{base} {transformed}"))
}

/// Built-in CommandCode template id and its two official model views: the
/// ordinary list stays authoritative for membership, names and endpoints, while
/// the client catalog supplies explicit reasoning-effort declarations.
const COMMANDCODE_TEMPLATE_ID: &str = "commandcode";
const COMMANDCODE_ORDINARY_URL: &str = "https://api.commandcode.ai/provider/v1/models";
const COMMANDCODE_CLIENT_URL: &str = "https://api.commandcode.ai/provider/v1/models?view=client";

/// Provider API reasoning-effort values CommandCode accepts; `off` is a CLI
/// control that must never be exported as an upstream effort.
const COMMANDCODE_PROVIDER_API_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Normalize an explicitly declared reasoning-effort value: `null` and an empty
/// array clear the set; an array of strings is trimmed, blank-dropped and
/// deduplicated in source order; any other shape is malformed.
fn normalize_declared_efforts(value: &Value) -> Result<Vec<String>, String> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => {
            let mut out: Vec<String> = Vec::with_capacity(items.len());
            for item in items {
                let text = item.as_str().ok_or_else(|| {
                    "a declared reasoning-effort field must be an array of strings".to_string()
                })?;
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !out.iter().any(|existing| existing == trimmed) {
                    out.push(trimmed.to_string());
                }
            }
            Ok(out)
        }
        _ => Err(
            "a declared reasoning-effort field must be an array of strings or null".to_string(),
        ),
    }
}

/// The explicitly declared reasoning-effort set of an object entry, when the
/// entry carries one: `reasoningEfforts` wins over a coexisting
/// `reasoning_efforts`, and an absent field is `None` so the previous value is
/// preserved.
fn declared_reasoning_efforts(
    object: &serde_json::Map<String, Value>,
) -> Result<Option<Vec<String>>, String> {
    match object
        .get("reasoningEfforts")
        .or_else(|| object.get("reasoning_efforts"))
    {
        Some(value) => normalize_declared_efforts(value).map(Some),
        None => Ok(None),
    }
}

/// Parse one explicitly declared total-context field. `Ok(None)` is an explicit
/// JSON `null`, `Ok(Some(value))` a positive integer no larger than `u32::MAX`,
/// and any other JSON shape is malformed. The caller supplies the model id and
/// field name so the error names the exact declaration.
fn declared_context_value(
    value: &Value,
    identifier: &str,
    field: &str,
) -> Result<Option<u32>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => match number.as_u64() {
            Some(parsed) if parsed > 0 && parsed <= u32::MAX as u64 => Ok(Some(parsed as u32)),
            _ => Err(format!(
                "model '{identifier}' declares an invalid {field}: expected a positive integer no larger than {}",
                u32::MAX
            )),
        },
        _ => Err(format!(
            "model '{identifier}' declares an invalid {field}: expected a positive integer no larger than {} or null",
            u32::MAX
        )),
    }
}

/// The total-context declaration of one source entry, distinguishing absence
/// from an explicit `null`. Top-level `context_length` and nested `limit.context`
/// are both supported; when both are present they must agree, including `null`.
/// A missing/non-object `limit` parent, or an object without `context`, declares
/// nothing. Any declared malformed value or a disagreement is an error.
fn declared_context_window(
    object: &serde_json::Map<String, Value>,
    identifier: &str,
) -> Result<Option<Option<u32>>, String> {
    let top_level = object
        .get("context_length")
        .map(|value| declared_context_value(value, identifier, "context_length"))
        .transpose()?;
    let nested = match object.get("limit") {
        None | Some(Value::Null) => None,
        Some(Value::Object(limit)) => limit
            .get("context")
            .map(|value| declared_context_value(value, identifier, "limit.context"))
            .transpose()?,
        Some(_) => {
            return Err(format!(
                "model '{identifier}' declares an invalid limit: expected an object or null"
            ))
        }
    };
    match (top_level, nested) {
        (None, None) => Ok(None),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (Some(top), Some(nested)) if top == nested => Ok(Some(top)),
        (Some(top), Some(nested)) => Err(format!(
            "model '{identifier}' declares conflicting context windows: context_length is {} and limit.context is {}",
            top.map_or_else(|| "null".to_string(), |value| value.to_string()),
            nested.map_or_else(|| "null".to_string(), |value| value.to_string()),
        )),
    }
}

/// Whether a source object declares a total context window; called only after
/// the entry has passed the existing identifier, hidden, endpoint and duplicate
/// filters, so a rejected entry's malformed metadata never fails the sync.
fn resolve_source_context_window(
    object: &serde_json::Map<String, Value>,
    identifier: &str,
    previous: Option<&ProviderTemplateModel>,
) -> Result<Option<u32>, String> {
    Ok(match declared_context_window(object, identifier)? {
        Some(declared) => declared,
        None => previous.and_then(|model| model.context_window),
    })
}

/// The property of a source entry that marks it hidden from clients; only an
/// exact boolean `true` skips the entry.
fn source_entry_is_hidden(object: &serde_json::Map<String, Value>) -> bool {
    matches!(object.get("hidden"), Some(Value::Bool(true)))
}

/// Parse a model-list payload and merge it over the template's current models.
///
/// Accepted payload shapes are a `data` array, a `models` array, a root array
/// and string entries. An object entry's identifier is `id` (or `name` for the
/// `models` shape when `id` is absent) and its display name is the trimmed
/// source `name` completed with the identifier segment's uncovered remainder
/// (a blank source name falls back to the previously stored display name, and
/// a missing base transforms the identifier segment on its own). When
/// `supported_endpoints` is an array: Chat Completions wins over Responses,
/// which wins over Anthropic Messages; an array declaring none of those drops
/// the entry. A missing or non-array field keeps the entry with no protocol so
/// it inherits the template protocol. An object entry may also declare
/// `reasoningEfforts` (winning over `reasoning_efforts`): an array replaces the
/// set, `null`/`[]` clears it and an absent field preserves the previous value,
/// while any other declared shape is malformed. Existing models keep their local
/// `enabled` flag and any source-omitted display name/protocol; new models start
/// enabled. An entry whose `hidden` is exactly `true` is skipped.
/// Duplicate identifiers keep the first. A missing model array, an entry
/// without an identifier and an empty effective set are fatal.
fn parse_model_list_source(
    raw: &str,
    previous: &ProviderTemplate,
) -> Result<Vec<ProviderTemplateModel>, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("response is not valid JSON: {error}"))?;

    let (entries, allow_name_identifier) =
        if let Some(data) = value.get("data").and_then(Value::as_array) {
            (data, false)
        } else if let Some(models) = value.get("models").and_then(Value::as_array) {
            (models, true)
        } else if let Some(root) = value.as_array() {
            (root, false)
        } else {
            return Err("response is missing the model list".to_string());
        };

    let mut merged: Vec<ProviderTemplateModel> = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(raw_identifier) = entry.as_str() {
            let identifier = raw_identifier.trim();
            if identifier.is_empty() {
                return Err(
                    "response contains a model entry without an identifier".to_string()
                );
            }
            if merged
                .iter()
                .any(|model| model.upstream_model == identifier)
            {
                continue;
            }
            let previous_model = previous
                .models
                .iter()
                .find(|model| model.upstream_model == identifier);
            let display_name =
                complete_model_display_name(previous_model.and_then(|model| model.display_name.as_deref()), identifier);
            merged.push(ProviderTemplateModel {
                upstream_model: identifier.to_string(),
                local_model: previous_model.and_then(|model| model.local_model.clone()),
                display_name,
                protocol: previous_model.and_then(|model| model.protocol),
                enabled: previous_model.map(|model| model.enabled).unwrap_or(true),
                input: previous_model.map(|model| model.input).unwrap_or(0.0),
                cache_read: previous_model.map(|model| model.cache_read).unwrap_or(0.0),
                cache_write: previous_model.map(|model| model.cache_write).unwrap_or(0.0),
                output: previous_model.map(|model| model.output).unwrap_or(0.0),
                off_peaks: previous_model.map(|model| model.off_peaks.clone()).unwrap_or_default(),
                reasoning_efforts: previous_model.map(|model| model.reasoning_efforts.clone()).unwrap_or_default(),
                context_window: previous_model.and_then(|model| model.context_window),
            });
            continue;
        }

        let object = entry.as_object().ok_or_else(|| {
            "response contains a model entry without an identifier".to_string()
        })?;
        if source_entry_is_hidden(object) {
            continue;
        }
        let declared_efforts = declared_reasoning_efforts(object)?;
        let identifier = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                if allow_name_identifier {
                    object
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                } else {
                    None
                }
            })
            .ok_or_else(|| "response contains a model entry without an identifier".to_string())?
            .to_string();

        let protocol = match object.get("supported_endpoints") {
            Some(Value::Array(endpoints)) => {
                let has = |needle: &str| {
                    endpoints
                        .iter()
                        .any(|endpoint| endpoint.as_str() == Some(needle))
                };
                if has("/chat/completions") {
                    Some(UpstreamProtocol::ChatCompletions)
                } else if has("/responses") {
                    Some(UpstreamProtocol::Responses)
                } else if has("/messages") || has("/v1/messages") {
                    Some(UpstreamProtocol::AnthropicMessages)
                } else {
                    continue;
                }
            }
            _ => None,
        };

        if merged
            .iter()
            .any(|model| model.upstream_model == identifier)
        {
            continue;
        }
        let previous_model = previous
            .models
            .iter()
            .find(|model| model.upstream_model == identifier);
        let source_name = object
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let display_name = complete_model_display_name(
            source_name.or_else(|| previous_model.and_then(|model| model.display_name.as_deref())),
            &identifier,
        );
        let reasoning_efforts = match declared_efforts {
            Some(efforts) => efforts,
            None => previous_model
                .map(|model| model.reasoning_efforts.clone())
                .unwrap_or_default(),
        };
        let context_window =
            resolve_source_context_window(object, &identifier, previous_model)?;
        merged.push(ProviderTemplateModel {
            upstream_model: identifier,
            local_model: previous_model.and_then(|model| model.local_model.clone()),
            display_name,
            protocol: protocol.or_else(|| previous_model.and_then(|model| model.protocol)),
            enabled: previous_model.map(|model| model.enabled).unwrap_or(true),
            input: previous_model.map(|model| model.input).unwrap_or(0.0),
            cache_read: previous_model.map(|model| model.cache_read).unwrap_or(0.0),
            cache_write: previous_model.map(|model| model.cache_write).unwrap_or(0.0),
            output: previous_model.map(|model| model.output).unwrap_or(0.0),
            off_peaks: previous_model.map(|model| model.off_peaks.clone()).unwrap_or_default(),
            reasoning_efforts,
            context_window,
        });
    }

    if previous.sync_strategy == SyncStrategy::OpencodeGo {
        // A Go sync is declaration-aware: only an id the effective template
        // explicitly declares as Chat Completions, Responses or Anthropic
        // Messages — or one the source itself declares with a supported endpoint
        // — is compatible. An undeclared, metadata-less id is never guessed.
        let declared: Vec<(&str, UpstreamProtocol)> = previous
            .models
            .iter()
            .filter_map(|model| {
                model
                    .protocol
                    .map(|protocol| (model.upstream_model.as_str(), protocol))
            })
            .collect();
        let mut compatible: Vec<ProviderTemplateModel> = Vec::with_capacity(merged.len());
        for mut model in merged {
            if let Some((_, declared_protocol)) = declared
                .iter()
                .find(|(identifier, _)| *identifier == model.upstream_model)
            {
                model.protocol = Some(*declared_protocol);
                compatible.push(model);
            } else if model.protocol.is_some() {
                compatible.push(model);
            }
        }
        merged = compatible;
    }

    if merged.is_empty() {
        return Err("response carries an empty model set".to_string());
    }

    Ok(merged)
}

/// Replace (or insert) the persisted state for one template after a sync.
fn upsert_template_state(
    config: &mut GatewayConfig,
    template_id: &str,
    template: ProviderTemplate,
    synced_at: u64,
    source: String,
) {
    let state = ProviderTemplateState {
        template_id: template_id.to_string(),
        template: Some(template),
        synced_at: Some(synced_at),
        source: Some(source),
    };
    if let Some(existing) = config
        .provider_templates
        .iter_mut()
        .find(|existing| existing.template_id == template_id)
    {
        *existing = state;
    } else {
        config.provider_templates.push(state);
    }
}

/// The set form of a declared reasoning-effort list: every token trimmed, blank
/// tokens dropped, duplicates removed and the remainder sorted so two lists
/// compare equal regardless of order, duplication or surrounding whitespace.
fn normalized_effort_set(efforts: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::with_capacity(efforts.len());
    for effort in efforts {
        let trimmed = effort.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !out.iter().any(|existing| *existing == trimmed) {
            out.push(trimmed);
        }
    }
    out.sort_unstable();
    out
}

/// Propagate a template update to every derived provider whose `template_id`
/// matches: provider name/base_url/protocol update only while they still equal
/// the previous template values, and only enabled template models add new
/// mappings. A mapping is deleted when its upstream model left the template
/// (whatever its enabled flag) or when it was already disabled and its model the
/// new template does not list; a retired model is appended once to the
/// provider's ignored set so a later sync cannot resurrect it, its
/// provider-scoped price row is deleted, and a `default_model` naming it is
/// cleared. An enabled mapping the sync did not retire and a user-disabled
/// mapping whose model the new template still lists stay untouched. Mapping
/// display name/protocol update only while they equal the previous template
/// values (`local_model` is never touched); a mapping whose effective protocol
/// equals the provider protocol is stored as `None` (follow the provider)
/// instead of an explicit value. Every existing matching mapping whose
/// normalized effort set still equals the previous template model's set — an
/// alias, a user-disabled or auto-disabled row, or a disabled template model —
/// follows the new template capabilities, while an independently customized set
/// is preserved; enablement and health flags are never changed. Ignored models
/// and disabled template models never create a new mapping.
///
/// Returns one [`ProviderRetirementNotice`] for every bound provider with at
/// least one removal in this sync, in provider order. Each notice lists the
/// provider's removed models in mapping row order without duplicates,
/// including a provider whose only removals were already-disabled legacy rows.
/// A provider with no removal yields no notice.
fn propagate_to_derived(
    config: &mut GatewayConfig,
    template_id: &str,
    previous: &ProviderTemplate,
    new_template: &ProviderTemplate,
) -> Vec<ProviderRetirementNotice> {
    // Models the previous template carried that the new template no longer lists
    // are retired regardless of their enabled flag in either template.
    let retired_models: Vec<&str> = previous
        .models
        .iter()
        .filter(|model| {
            !new_template
                .models
                .iter()
                .any(|candidate| candidate.upstream_model == model.upstream_model)
        })
        .map(|model| model.upstream_model.as_str())
        .collect();

    let mut notices: Vec<ProviderRetirementNotice> = Vec::new();
    // Removed models per provider id, in mapping row order without duplicates,
    // collected while the provider loop holds the `config.providers` borrow so
    // the staged price rows can be pruned right after it.
    let mut removed_by_provider: Vec<(String, Vec<String>)> = Vec::new();
    let providers = &mut config.providers;

    for provider in providers
        .iter_mut()
        .filter(|provider| provider.template_id.as_deref() == Some(template_id))
    {
        if provider.name == previous.name {
            provider.name = new_template.name.clone();
        }
        if provider.base_url == previous.base_url {
            provider.base_url = new_template.base_url.clone();
        }
        let provider_protocol_before = provider.protocol;
        if provider.protocol == previous.protocol {
            provider.protocol = new_template.protocol;
        }
        let provider_protocol = provider.protocol;

        // Delete every mapping this sync retires: one whose upstream model the
        // previous template carried but the new template dropped (whatever its
        // enabled flag), or an already-disabled row whose model the new template
        // does not list. An enabled manual extra the sync never retired stays.
        let mut removed: Vec<String> = Vec::new();
        provider.mappings.retain(|mapping| {
            let retired_by_sync = retired_models
                .iter()
                .any(|retired| *retired == mapping.upstream_model);
            let legacy_retired = !mapping.enabled
                && !new_template
                    .models
                    .iter()
                    .any(|model| model.upstream_model == mapping.upstream_model);
            if retired_by_sync || legacy_retired {
                if !removed
                    .iter()
                    .any(|existing| existing == &mapping.upstream_model)
                {
                    removed.push(mapping.upstream_model.clone());
                }
                false
            } else {
                true
            }
        });

        if !removed.is_empty() {
            // Each removed model is ignored once so a later sync cannot
            // resurrect it, whichever row first carried it.
            for model in &removed {
                if !provider
                    .ignored_models
                    .iter()
                    .any(|ignored| ignored == model)
                {
                    provider.ignored_models.push(model.clone());
                }
            }
            // A `default_model` naming a removed model is cleared; a trimmed
            // comparison keeps whitespace-padded values matching.
            let clears_default_model = provider
                .default_model
                .as_deref()
                .map(str::trim)
                .is_some_and(|default_model| {
                    removed.iter().any(|model| model.as_str() == default_model)
                });
            if clears_default_model {
                provider.default_model = None;
            }
            notices.push(ProviderRetirementNotice {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                retired_models: removed.clone(),
            });
            removed_by_provider.push((provider.id.clone(), removed));
        }

        // Declared context is owned by the latest template for every remaining
        // row: aliases, user-disabled rows and auto-disabled rows follow the
        // first template model with the same trimmed upstream id, and a row the
        // template no longer lists becomes unspecified. Mapping health and
        // enablement are never changed here.
        for mapping in &mut provider.mappings {
            mapping.context_window = new_template
                .models
                .iter()
                .find(|model| model.upstream_model.trim() == mapping.upstream_model.trim())
                .and_then(|model| model.context_window);
        }

        // Explicit capability maintenance runs before creation and on existing
        // rows only, so it never creates a mapping and never resurrects an
        // ignored model. A row whose set still equals the previous template
        // model's set is inherited and follows the new set; any other set is an
        // independent override and is preserved.
        for model in &new_template.models {
            if provider
                .ignored_models
                .iter()
                .any(|ignored| ignored == &model.upstream_model)
            {
                continue;
            }
            let Some(previous_model) = previous
                .models
                .iter()
                .find(|candidate| candidate.upstream_model == model.upstream_model)
            else {
                continue;
            };
            let inherited_set = normalized_effort_set(&previous_model.reasoning_efforts);
            for mapping in provider
                .mappings
                .iter_mut()
                .filter(|mapping| mapping.upstream_model == model.upstream_model)
            {
                if normalized_effort_set(&mapping.reasoning_efforts) == inherited_set {
                    mapping.reasoning_efforts = model.reasoning_efforts.clone();
                }
            }
        }

        for model in new_template.models.iter().filter(|model| model.enabled) {
            if provider
                .ignored_models
                .iter()
                .any(|ignored| ignored == &model.upstream_model)
            {
                continue;
            }
            let previous_model = previous
                .models
                .iter()
                .find(|candidate| candidate.upstream_model == model.upstream_model);
            let effective_protocol = model.protocol.unwrap_or(new_template.protocol);
            let desired_protocol = if effective_protocol == provider_protocol {
                None
            } else {
                Some(effective_protocol)
            };

            if let Some(mapping) = provider
                .mappings
                .iter_mut()
                .find(|mapping| mapping.upstream_model == model.upstream_model)
            {
                let Some(previous_model) = previous_model else {
                    continue;
                };
                if mapping.display_name == previous_model.display_name {
                    mapping.display_name = model.display_name.clone();
                }
                let previous_effective =
                    previous_model.protocol.unwrap_or(previous.protocol);
                if mapping.effective_protocol(provider_protocol_before) == previous_effective
                {
                    mapping.protocol = desired_protocol;
                }
            } else {
                provider.mappings.push(ModelMapping {
                    local_model: model
                        .local_model
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&model.upstream_model)
                        .to_string(),
                    upstream_model: model.upstream_model.clone(),
                    enabled: true,
                    protocol: desired_protocol,
                    display_name: model
                        .display_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                        .or_else(|| {
                            complete_model_display_name(None, &model.upstream_model)
                        }),
                    reasoning_efforts: model.reasoning_efforts.clone(),
                    context_window: model.context_window,
                    ..Default::default()
                });
            }
        }
    }

    // Drop exactly the staged price rows of each affected provider and removed
    // model; rows of other providers or other models stay.
    config.model_prices.retain(|row| {
        !removed_by_provider.iter().any(|(provider_id, removed)| {
            row.provider_id.as_deref() == Some(provider_id.as_str())
                && removed.iter().any(|model| model == &row.upstream_model)
        })
    });

    notices
}

/// Apply one template sync with injectable fetch and persistence seams, returning
/// the retirement notices alongside the view.
///
/// The template's model list is replaced wholesale from its `models_url`; a
/// source-provided display name completed with the identifier's uncovered
/// remainder (and the derived protocol) wins while a locally owned
/// `enabled` flag and any omitted value are kept for models the new list still
/// carries. A sync deletes the derived mapping of every model the previous
/// template carried but the new list removed (whatever the mapping's enabled
/// flag) and of every already-disabled mapping whose model the new list no
/// longer has; each removed model is recorded once in the provider's ignored
/// set, its provider-scoped price row is deleted, and a `default_model` naming
/// it is cleared. A blank URL and every fatal source problem (fetch error,
/// non-JSON, missing model array, entry without an identifier, empty effective
/// set) write nothing. The new template and every derived provider update are
/// staged on a clone, handed to `persist`, and only committed to `config` once
/// persistence succeeds. The notices are computed from the staged clone and
/// returned only after `persist` succeeds and `config` is updated; any error
/// returns no notices.
pub(in crate::ai_gateway) fn apply_template_sync_with_notices(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<(ProviderTemplateView, Vec<ProviderRetirementNotice>), String> {
    let previous = effective_template(config, template_id)?;
    let url = previous
        .models_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| format!("template '{template_id}' has no model-list URL configured"))?
        .to_string();

    let raw = fetch(&previous).map_err(|reason| format!("failed to fetch {url}: {reason}"))?;
    let models = parse_model_list_source(&raw, &previous)
        .map_err(|reason| format!("failed to parse {url}: {reason}"))?;

    let mut next_template = previous.clone();
    next_template.models = models;

    let mut next = config.clone();
    let notices = propagate_to_derived(&mut next, template_id, &previous, &next_template);
    let synced_at = now_ts();
    let source = next_template.source.clone();
    upsert_template_state(
        &mut next,
        template_id,
        next_template.clone(),
        synced_at,
        source.clone(),
    );

    persist(&next)?;
    *config = next;

    Ok((
        ProviderTemplateView {
            template: next_template,
            synced_at: Some(synced_at),
            source,
            from_snapshot: false,
        },
        notices,
    ))
}

/// Apply one template sync with injectable fetch and persistence seams, dropping
/// the retirement notices.
///
/// Test-only wrapper; production syncs go through
/// [`apply_template_sync_with_notices`], which carries the same behavior plus
/// the notices.
#[cfg(test)]
pub fn apply_template_sync_with(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<ProviderTemplateView, String> {
    apply_template_sync_with_notices(config, template_id, fetch, persist)
        .map(|(view, _notices)| view)
}

/// Apply one template sync from an already-fetched model-list body, with no
/// network call and no nested persistence.
///
/// The caller performs the HTTP fetch outside the configuration write lock and
/// then runs this inside the serialized mutation: the body is parsed against
/// the latest persisted template exactly like [`apply_template_sync_with`], and
/// every template/derived-provider change is committed to `config`. A parse
/// failure writes nothing. The retirement notices computed by the sync are
/// returned so the caller can record them once the write stands.
pub(in crate::ai_gateway) fn apply_template_sync_from_body(
    config: &mut GatewayConfig,
    template_id: &str,
    raw: &str,
) -> Result<(ProviderTemplateView, Vec<ProviderRetirementNotice>), String> {
    apply_template_sync_with_notices(config, template_id, |_| Ok(raw.to_string()), |_| Ok(()))
}

/// Fetch a template's model list through an injectable request seam and, for the
/// canonical CommandCode template, enrich the ordinary list with the official
/// client catalog's capabilities.
///
/// The ordinary list stays authoritative for membership, names and endpoints: a
/// second credential-free request reads the client view and only joins matching
/// declared efforts by exact upstream id, so a client-only entry never adds a
/// model. The seam returns the final ordinary-list JSON body. Any client failure
/// (request, unsupported `schemaVersion`, invalid `data`, missing or duplicate
/// id, malformed declared effort) fails the whole operation naming the client
/// URL. Every other URL is fetched once as configured.
pub(in crate::ai_gateway) async fn fetch_template_models_with<F, Fut>(
    template: &ProviderTemplate,
    mut fetch: F,
) -> Result<String, String>
where
    F: FnMut(String) -> Fut + Send,
    Fut: core::future::Future<Output = Result<String, String>> + Send,
{
    let url = template
        .models_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "template has no model-list URL configured".to_string())?
        .to_string();

    if template.id.trim() != COMMANDCODE_TEMPLATE_ID || url != COMMANDCODE_ORDINARY_URL {
        return fetch(url.clone())
            .await
            .map_err(|reason| format!("failed to fetch {url}: {reason}"));
    }

    let ordinary = fetch(COMMANDCODE_ORDINARY_URL.to_string())
        .await
        .map_err(|reason| format!("failed to fetch {COMMANDCODE_ORDINARY_URL}: {reason}"))?;
    let client = fetch(COMMANDCODE_CLIENT_URL.to_string())
        .await
        .map_err(|reason| format!("failed to fetch {COMMANDCODE_CLIENT_URL}: {reason}"))?;
    join_commandcode_client_capabilities(&ordinary, &client)
}

/// The client-catalog failure message, always naming the client URL.
fn commandcode_client_error(reason: impl std::fmt::Display) -> String {
    format!("failed to parse {COMMANDCODE_CLIENT_URL}: {reason}")
}

/// A mutable reference to the model-entry array of an accepted source shape:
/// `data`, `models`, or the root array.
fn model_entries_mut(value: &mut Value) -> Option<&mut Vec<Value>> {
    match value {
        Value::Array(entries) => Some(entries),
        Value::Object(map) => {
            if map.get("data").is_some_and(Value::is_array) {
                map.get_mut("data").and_then(Value::as_array_mut)
            } else if map.get("models").is_some_and(Value::is_array) {
                map.get_mut("models").and_then(Value::as_array_mut)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Enrich an ordinary CommandCode model list with the official client catalog's
/// declared reasoning efforts, joined by exact upstream id. Only capabilities
/// are joined: the ordinary entries, their order, names and endpoints are
/// untouched, and a client-only or hidden catalog entry never adds a model.
/// Effort values are normalized, then filtered to the CommandCode Provider API
/// set so a CLI-only `off` is never exported.
fn join_commandcode_client_capabilities(
    ordinary: &str,
    client: &str,
) -> Result<String, String> {
    let client_value: Value = serde_json::from_str(client)
        .map_err(|error| commandcode_client_error(format!("not valid JSON: {error}")))?;
    if client_value.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err(commandcode_client_error("unsupported catalog schemaVersion"));
    }
    let client_entries = client_value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| commandcode_client_error("the catalog is missing its model data array"))?;

    let mut declared: Vec<(String, Option<Vec<String>>)> =
        Vec::with_capacity(client_entries.len());
    for entry in client_entries {
        let object = entry
            .as_object()
            .ok_or_else(|| commandcode_client_error("the catalog contains a non-object entry"))?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| commandcode_client_error("the catalog contains an entry without an id"))?
            .to_string();
        if declared.iter().any(|(existing, _)| existing == &id) {
            return Err(commandcode_client_error(format!(
                "the catalog declares the duplicate id {id}"
            )));
        }
        let efforts = declared_reasoning_efforts(object)
            .map_err(commandcode_client_error)?
            .map(|efforts| {
                efforts
                    .into_iter()
                    .filter(|effort| COMMANDCODE_PROVIDER_API_EFFORTS.contains(&effort.as_str()))
                    .collect::<Vec<String>>()
            });
        declared.push((id, efforts));
    }

    let mut ordinary_value: Value = serde_json::from_str(ordinary)
        .map_err(|error| format!("response is not valid JSON: {error}"))?;
    let entries = model_entries_mut(&mut ordinary_value)
        .ok_or_else(|| "response is missing the model list".to_string())?;

    for entry in entries.iter_mut() {
        let Some(object) = entry.as_object_mut() else {
            continue;
        };
        let identifier = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                object
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .map(str::to_string);
        let Some(identifier) = identifier else {
            continue;
        };
        let Some((_, Some(efforts))) = declared.iter().find(|(id, _)| id == &identifier) else {
            continue;
        };
        object.insert(
            "reasoningEfforts".to_string(),
            Value::Array(efforts.iter().cloned().map(Value::String).collect()),
        );
    }

    serde_json::to_string(&ordinary_value)
        .map_err(|error| format!("failed to encode the enriched model list: {error}"))
}

static TEMPLATE_MODELS_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

/// Reuse one HTTP client for template model-list requests. The policy stays the
/// existing 15-second timeout with the default redirect handling; only the
/// client construction is shared.
fn template_models_client() -> Result<reqwest::Client, String> {
    TEMPLATE_MODELS_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(|error| format!("failed to prepare a model-list request: {error}"))
}

/// Fetch a template's model list from its configured `models_url` through
/// `reqwest` with a 15-second timeout and no credentials, delegating the
/// CommandCode client-catalog join to [`fetch_template_models_with`]. A blank
/// URL, non-2xx status, timeout and network error return an actionable error
/// naming the URL and the reason.
pub(in crate::ai_gateway) async fn fetch_template_models(
    template: &ProviderTemplate,
) -> Result<String, String> {
    let client = template_models_client()?;
    fetch_template_models_with(template, move |url| {
        let client = client.clone();
        async move {
            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|error| error.to_string())?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("HTTP {status}"));
            }
            response.text().await.map_err(|error| error.to_string())
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// Step 4: create-from-template and model maintenance
// ---------------------------------------------------------------------------

/// Build the mapping a template model is created with: `local_model` equals
/// `upstream_model` (or the template's trimmed `local_model` when present),
/// carrying the model's enabled flag, the template's display name verbatim and
/// `None` (follow the provider protocol) when the model's effective protocol
/// equals the provider protocol. A missing or blank template display name
/// falls back to the identifier segment transformed into display words so the
/// new mapping never stays empty.
fn mapping_from_template(
    template: &ProviderTemplate,
    model: &ProviderTemplateModel,
    provider_protocol: UpstreamProtocol,
) -> ModelMapping {
    let effective = model.protocol.unwrap_or(template.protocol);
    ModelMapping {
        local_model: model
            .local_model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&model.upstream_model)
            .to_string(),
        upstream_model: model.upstream_model.clone(),
        enabled: model.enabled,
        protocol: if effective == provider_protocol {
            None
        } else {
            Some(effective)
        },
        display_name: model
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| complete_model_display_name(None, &model.upstream_model)),
        reasoning_efforts: model.reasoning_efforts.clone(),
        context_window: model.context_window,
        ..Default::default()
    }
}

/// Create an upstream provider from a template with an injectable persistence
/// seam.
///
/// A blank API key is rejected before anything is staged, so the config and the
/// persist seam stay untouched. A blank `name`/`base_url` falls back to the
/// template's value; the protocol argument always wins. The new provider binds
/// the template, leaves `default_model` empty, receives one mapping per enabled
/// template model (`local_model` = `upstream_model`, official display name,
/// `None` i.e. follow-the-provider when the effective protocol equals the
/// provider protocol) and writes prices if defined on the template models.
/// The staged clone is handed to `persist` and only committed to `config` once persistence succeeds.
pub fn apply_create_provider_from_template(
    config: &mut GatewayConfig,
    template_id: &str,
    name: &str,
    base_url: &str,
    protocol: UpstreamProtocol,
    api_key: &str,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<GatewayUpstreamProvider, String> {
    if api_key.trim().is_empty() {
        return Err("an API key is required to create a provider from a template".to_string());
    }
    let template = effective_template(config, template_id)?;

    let provider_id = new_provider_id();
    let provider = GatewayUpstreamProvider {
        id: provider_id.clone(),
        name: if name.trim().is_empty() {
            template.name.clone()
        } else {
            name.to_string()
        },
        base_url: if base_url.trim().is_empty() {
            template.base_url.clone()
        } else {
            base_url.to_string()
        },
        keys: vec![UpstreamKey {
            id: "default".to_string(),
            name: "Default".to_string(),
            value: api_key.to_string(),
            enabled: true,
            ..UpstreamKey::default()
        }],
        template_id: Some(template_id.to_string()),
        protocol,
        mappings: template
            .models
            .iter()
            .filter(|model| model.enabled)
            .map(|model| mapping_from_template(&template, model, protocol))
            .collect(),
        ignored_models: Vec::new(),
        ..GatewayUpstreamProvider::default()
    };

    let mut next = config.clone();
    next.providers.push(provider.clone());
    for model in &template.models {
        if model.enabled
            && (model.input > 0.0
                || model.output > 0.0
                || model.cache_read > 0.0
                || model.cache_write > 0.0
                || !model.off_peaks.is_empty())
        {
            next.model_prices.push(ModelPrice {
                provider_id: Some(provider_id.clone()),
                upstream_model: model.upstream_model.clone(),
                input: model.input,
                cache_read: model.cache_read,
                cache_write: model.cache_write,
                output: model.output,
                off_peaks: model.off_peaks.clone(),
            });
        }
    }

    persist(&next)?;
    *config = next;
    Ok(provider)
}

/// Delete one upstream model from a provider with an injectable persistence
/// seam.
///
/// An unknown provider is an actionable error that writes nothing. The mapping
/// and the provider-scoped price row for that model are removed on both a
/// template-bound and a manual provider. A template-bound provider additionally
/// records the model in its ignored set exactly once so a later sync cannot
/// resurrect it; a manual provider writes no ignored record.
pub fn apply_delete_provider_model(
    config: &mut GatewayConfig,
    provider_id: &str,
    upstream_model: &str,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<(), String> {
    if !config.providers.iter().any(|provider| provider.id == provider_id) {
        return Err(format!("provider not found: {provider_id}"));
    }

    let mut next = config.clone();
    {
        let provider = next
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
            .expect("the provider was checked above");
        provider
            .mappings
            .retain(|mapping| mapping.upstream_model != upstream_model);
        if provider.template_id.is_some()
            && !provider
                .ignored_models
                .iter()
                .any(|ignored| ignored == upstream_model)
        {
            provider.ignored_models.push(upstream_model.to_string());
        }
    }
    next.model_prices.retain(|row| {
        !(row.provider_id.as_deref() == Some(provider_id)
            && row.upstream_model == upstream_model)
    });

    persist(&next)?;
    *config = next;
    Ok(())
}

/// Restore one previously ignored model from the template's current data with
/// an injectable persistence seam.
///
/// Only a model present in the provider's `ignored_models` can be restored;
/// anything else is an actionable error that writes nothing. The mapping is
/// rebuilt from the persisted template state when one exists, else from the
/// built-in snapshot, and the model leaves the ignored set; no price row is
/// written or modified. A model the template no longer carries reports an error
/// naming it and creates no mapping.
pub fn apply_restore_provider_model(
    config: &mut GatewayConfig,
    provider_id: &str,
    upstream_model: &str,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<(), String> {
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("provider not found: {provider_id}"))?;
    if !provider
        .ignored_models
        .iter()
        .any(|ignored| ignored == upstream_model)
    {
        return Err(format!(
            "model '{upstream_model}' is not ignored and cannot be restored"
        ));
    }
    let template_id = provider.template_id.clone().ok_or_else(|| {
        format!("provider {provider_id} is not bound to a provider template")
    })?;

    let template = effective_template(config, &template_id)?;
    let model = template
        .models
        .iter()
        .find(|model| model.upstream_model == upstream_model)
        .cloned()
        .ok_or_else(|| {
            format!("model '{upstream_model}' is not present in template '{template_id}'")
        })?;
    let provider_protocol = provider.protocol;

    let mut next = config.clone();
    {
        let provider = next
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
            .expect("the provider was checked above");
        provider
            .ignored_models
            .retain(|ignored| ignored != upstream_model);
        provider
            .mappings
            .retain(|mapping| mapping.upstream_model != upstream_model);
        provider
            .mappings
            .push(mapping_from_template(&template, &model, provider_protocol));
    }

    persist(&next)?;
    *config = next;
    Ok(())
}

/// Save an updated or new provider template into persisted state.
pub fn apply_upsert_provider_template<W>(
    config: &mut GatewayConfig,
    mut template: ProviderTemplate,
    write: W,
) -> Result<Vec<ProviderTemplateView>, String>
where
    W: FnOnce(&GatewayConfig) -> Result<(), String>,
{
    template.id = template.id.trim().to_string();
    if template.id.is_empty() {
        template.id = format!("tpl-{}", new_provider_id());
    }
    template.name = template.name.trim().to_string();
    if template.name.is_empty() {
        return Err("Template name is required".to_string());
    }
    template.base_url = template.base_url.trim().to_string();
    if template.base_url.is_empty() {
        return Err("Template base URL is required".to_string());
    }
    template.icon = template
        .icon
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    for model in &template.models {
        if model.context_window == Some(0) {
            return Err(format!(
                "model '{}' declares an invalid context window: expected a positive integer",
                model.upstream_model
            ));
        }
    }

    let saved_models = template.models.clone();
    let mut next = config.clone();
    next.deleted_template_ids.retain(|id| id != &template.id);

    let template_id = template.id.clone();
    let source = if template.source.trim().is_empty() {
        None
    } else {
        Some(template.source.clone())
    };

    if let Some(state) = next
        .provider_templates
        .iter_mut()
        .find(|s| s.template_id == template_id)
    {
        state.template = Some(template);
        if source.is_some() {
            state.source = source;
        }
    } else {
        next.provider_templates.push(ProviderTemplateState {
            template_id: template_id.clone(),
            template: Some(template),
            synced_at: None,
            source,
        });
    }

    // A manual template save owns the inherited context of every bound mapping,
    // including aliases and disabled rows. A mapping whose trimmed upstream id
    // the template no longer lists becomes unspecified without retiring the row.
    for provider in next
        .providers
        .iter_mut()
        .filter(|provider| provider.template_id.as_deref() == Some(template_id.as_str()))
    {
        for mapping in &mut provider.mappings {
            mapping.context_window = saved_models
                .iter()
                .find(|model| model.upstream_model.trim() == mapping.upstream_model.trim())
                .and_then(|model| model.context_window);
        }
    }

    write(&next)?;
    *config = next;
    provider_template_views(config)
}

/// Delete a provider template. Fails if the template is currently in use by any upstream provider.
pub fn apply_delete_provider_template<W>(
    config: &mut GatewayConfig,
    template_id: &str,
    write: W,
) -> Result<Vec<ProviderTemplateView>, String>
where
    W: FnOnce(&GatewayConfig) -> Result<(), String>,
{
    let tid = template_id.trim();
    if tid.is_empty() {
        return Err("Template ID is required".to_string());
    }

    let used_by = config
        .providers
        .iter()
        .find(|p| p.template_id.as_deref() == Some(tid));
    if let Some(provider) = used_by {
        return Err(format!(
            "Cannot delete template '{tid}': it is currently used by upstream provider '{}'",
            provider.name
        ));
    }

    config.provider_templates.retain(|s| s.template_id != tid);
    if !config.deleted_template_ids.iter().any(|id| id == tid) {
        config.deleted_template_ids.push(tid.to_string());
    }

    write(config)?;
    provider_template_views(config)
}
