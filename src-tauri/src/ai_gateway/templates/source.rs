//! Model-list source parsing and template model fetching.

use super::super::types_config::{
    ProviderTemplate, ProviderTemplateModel, SyncStrategy, UpstreamProtocol,
};
use serde_json::Value;
use std::time::Duration;

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
pub(super) fn complete_model_display_name(base: Option<&str>, identifier: &str) -> Option<String> {
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
pub(super) fn parse_model_list_source(
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

/// Build a template model-list client for the current global proxy generation.
/// The policy stays the existing 15-second timeout with the default redirect
/// handling; transport is resolved anew for every model-list operation.
fn template_models_client() -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder().timeout(Duration::from_secs(15));
    crate::proxy::apply_current_proxy_to_builder(builder)?
        .build()
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

#[cfg(test)]
mod proxy_switch_named {
    #[tokio::test]
    async fn gateway_template_models_proxy_switch() {
        use crate::ai_gateway::types_config::ProviderTemplate;
        use crate::proxy::test_support::{HttpProxyRecorder, SERIAL_GUARD};
        let _g = SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let proxy_a = HttpProxyRecorder::new("[]").await;
        let proxy_b = HttpProxyRecorder::new("[]").await;
        fn port_of(url: &str) -> u16 {
            url.trim_start_matches("http://").split(':').nth(1).unwrap().parse().unwrap()
        }
        let manager = crate::proxy::PROXY_MANAGER.get_or_init(|| std::sync::Arc::new(crate::proxy::ProxyManager::new()));
        manager.publish(manager.prepare(Some(crate::config::ProxyConfig {
            proxy_enabled: true,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port_of(&proxy_a.url()),
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        })).unwrap());

        let template = ProviderTemplate {
            id: "t".to_string(),
            name: "T".to_string(),
            description: String::new(),
            base_url: String::new(),
            protocol: crate::ai_gateway::UpstreamProtocol::ChatCompletions,
            sync_strategy: Default::default(),
            quota_strategy: Default::default(),
            error_strategy: Default::default(),
            transport_strategy: Default::default(),
            icon: None,
            source: String::new(),
            models_url: Some("http://template.example.test/models".to_string()),
            models: vec![],
        };
        let _ = super::fetch_template_models(&template).await;
        assert_eq!(proxy_a.targets(), vec!["http://template.example.test/models".to_string()]);

        manager.publish(manager.prepare(Some(crate::config::ProxyConfig {
            proxy_enabled: true,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port_of(&proxy_b.url()),
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        })).unwrap());

        let _ = super::fetch_template_models(&template).await;
        assert_eq!(proxy_b.targets(), vec!["http://template.example.test/models".to_string()]);
    }
}
