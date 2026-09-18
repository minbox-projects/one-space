//! Provider template snapshot loading and validation.
//!
//! The app ships `provider_templates.json` embedded via `include_str!`; this
//! module parses that snapshot (or a later network payload) into the public
//! [`ProviderTemplate`] model, dropping what the gateway cannot serve instead of
//! failing the whole document. Only structural problems (invalid JSON, an empty
//! or duplicate template id) are fatal.

use super::types_config::{
    now_ts, GatewayConfig, ModelMapping, ModelPrice, OffPeakPrice, ProviderTemplate,
    ProviderTemplateModel, ProviderTemplateState, UpstreamProtocol,
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// Raw template document shape, kept lenient so unknown protocol strings and
/// out-of-range numbers can be dropped or normalized instead of aborting.
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
    source: String,
    #[serde(default)]
    snapshot_version: String,
    #[serde(default)]
    models: Vec<RawTemplateModel>,
}

#[derive(Deserialize)]
struct RawTemplateModel {
    #[serde(default)]
    upstream_model: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    protocol: Option<String>,
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
}

fn parse_protocol(raw: &str) -> Option<UpstreamProtocol> {
    match raw {
        "chat_completions" => Some(UpstreamProtocol::ChatCompletions),
        "responses" => Some(UpstreamProtocol::Responses),
        _ => None,
    }
}

/// Negative, `NaN` and infinite prices are treated as missing (`0.0`).
fn normalize_price(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

/// Rewrite numeric literals that overflow `f64` (for example `1e999`) to
/// `replacement`, leaving strings and every in-range number untouched.
///
/// `serde_json` rejects such a literal as "number out of range" before any
/// value can be normalized, so an out-of-range price would otherwise fail the
/// whole document. This sanitizer runs only as a fallback after a strict parse
/// failed, so valid documents keep their exact numeric representation. The
/// snapshot parser substitutes `0` (missing price); the sync parser substitutes
/// `null` so an out-of-range source value is treated as not provided.
fn sanitize_out_of_range_numbers(json: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(json.len());
    let mut chars = json.char_indices().peekable();
    let mut in_string = false;
    while let Some((idx, c)) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some((_, escaped)) = chars.next() {
                    out.push(escaped);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '-' | '0'..='9' => {
                let start = idx;
                let mut end = idx + c.len_utf8();
                while let Some(&(next_idx, next)) = chars.peek() {
                    if matches!(next, '0'..='9' | '.' | 'e' | 'E' | '+' | '-') {
                        end = next_idx + next.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                let token = &json[start..end];
                match token.parse::<f64>() {
                    Ok(value) if value.is_finite() => out.push_str(token),
                    _ => out.push_str(replacement),
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Parse a top-level JSON array of templates into the validated template model.
///
/// Structural failures (invalid JSON, an empty id or a duplicate template id)
/// return a readable error. Model-level problems (an empty identifier, an
/// unknown protocol, invalid prices and out-of-range weekdays) are non-fatal:
/// the model is dropped or its values normalized.
pub fn parse_template_snapshot(json: &str) -> Result<Vec<ProviderTemplate>, String> {
    let raw: Vec<RawTemplate> = match serde_json::from_str(json) {
        Ok(raw) => raw,
        Err(error) => {
            let sanitized = sanitize_out_of_range_numbers(json, "0");
            if sanitized == json {
                return Err(format!(
                    "provider template snapshot is not a valid template array: {error}"
                ));
            }
            serde_json::from_str(&sanitized).map_err(|retry| {
                format!("provider template snapshot is not a valid template array: {retry}")
            })?
        }
    };

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
                display_name: raw_model.display_name,
                protocol,
                input: normalize_price(raw_model.input),
                cache_read: normalize_price(raw_model.cache_read),
                cache_write: normalize_price(raw_model.cache_write),
                output: normalize_price(raw_model.output),
                off_peaks: raw_model.off_peaks,
                reasoning_efforts: raw_model.reasoning_efforts,
            });
        }

        let protocol = match item.protocol.as_deref() {
            Some(raw) => parse_protocol(raw).unwrap_or_default(),
            None => UpstreamProtocol::ChatCompletions,
        };

        templates.push(ProviderTemplate {
            id,
            name: item.name,
            description: item.description,
            base_url: item.base_url,
            protocol,
            source: item.source,
            snapshot_version: item.snapshot_version,
            models,
        });
    }

    Ok(templates)
}

/// Parse the in-app snapshot embedded at compile time.
pub fn builtin_templates() -> Result<Vec<ProviderTemplate>, String> {
    parse_template_snapshot(include_str!("provider_templates.json"))
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

/// Resolve the effective template data for `template_id`: the last persisted
/// sync result when present, else the built-in snapshot. An unknown id is an
/// actionable error naming the id.
pub(in crate::api_gateway) fn effective_template(
    config: &GatewayConfig,
    template_id: &str,
) -> Result<ProviderTemplate, String> {
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

/// Build the ordered view of both built-in templates: persisted sync results
/// win, otherwise the built-in snapshot is reported with `from_snapshot`.
/// States whose id does not match a built-in template are ignored.
pub fn provider_template_views(
    config: &GatewayConfig,
) -> Result<Vec<ProviderTemplateView>, String> {
    let mut views = Vec::new();
    for snapshot in builtin_templates()? {
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
    Ok(views)
}

/// Which public source a template is refreshed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemplateSourceKind {
    ModelsDev,
    CommandCode,
}

fn template_source_kind(template: &ProviderTemplate) -> TemplateSourceKind {
    if template.id == "commandcode" || template.source.contains("commandcode") {
        TemplateSourceKind::CommandCode
    } else {
        TemplateSourceKind::ModelsDev
    }
}

/// Host label used in actionable sync errors (`models.dev` / `commandcode`).
fn template_source_label(template: &ProviderTemplate) -> String {
    match template_source_kind(template) {
        TemplateSourceKind::ModelsDev => "models.dev".to_string(),
        TemplateSourceKind::CommandCode => "commandcode".to_string(),
    }
}

/// Parse a public payload, tolerating numeric literals that overflow `f64`
/// (for example `1e999`) by turning them into `null`, i.e. "not provided".
fn parse_source_value(raw: &str) -> Result<Value, String> {
    match serde_json::from_str(raw) {
        Ok(value) => Ok(value),
        Err(error) => {
            let sanitized = sanitize_out_of_range_numbers(raw, "null");
            if sanitized == raw {
                return Err(error.to_string());
            }
            serde_json::from_str(&sanitized).map_err(|retry| retry.to_string())
        }
    }
}

/// A validated slice of a models.dev payload; model fields absent from the
/// source stay `None` so the merge can keep the template's current value.
struct ModelsDevSource {
    name: Option<String>,
    base_url: Option<String>,
    models: Vec<ProviderTemplateModel>,
}

fn non_empty_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Read one price field from a source `cost` object; negative, non-finite or
/// missing values fall back to the template's current value.
fn merge_source_price(cost: Option<&Value>, key: &str, fallback: f64) -> f64 {
    cost.and_then(|cost| cost.get(key))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

/// Collect reasoning-effort values from a models.dev `reasoning_options` list,
/// keeping only `effort` entries, preserving order and dropping duplicates.
fn reasoning_efforts_from_options(options: &[Value]) -> Vec<String> {
    let mut efforts: Vec<String> = Vec::new();
    for option in options {
        match option.get("type").and_then(Value::as_str) {
            Some("effort") | None => {}
            Some(_) => continue,
        }
        let Some(values) = option.get("values").and_then(Value::as_array) else {
            continue;
        };
        for value in values {
            let Some(value) = value.as_str() else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() || efforts.iter().any(|existing| existing == value) {
                continue;
            }
            efforts.push(value.to_string());
        }
    }
    efforts
}

/// Parse the single-provider models.dev payload (`{"id","name","api","models"}`)
/// and merge each source model over the template's current data. Structural
/// problems (invalid JSON, a missing model array, an entry without an
/// identifier) and an empty effective model set are fatal.
fn parse_models_dev_source(
    raw: &str,
    current: &ProviderTemplate,
) -> Result<ModelsDevSource, String> {
    let value =
        parse_source_value(raw).map_err(|error| format!("response is not valid JSON: {error}"))?;
    let models = value
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| "response is missing the model list".to_string())?;
    if models.is_empty() {
        return Err("response carries an empty model set".to_string());
    }

    let mut merged: Vec<ProviderTemplateModel> = Vec::with_capacity(models.len());
    for (upstream_model, entry) in models {
        if upstream_model.trim().is_empty() {
            return Err("response contains a model entry without an identifier".to_string());
        }
        if merged
            .iter()
            .any(|model| model.upstream_model == upstream_model.as_str())
        {
            continue;
        }
        let previous = current
            .models
            .iter()
            .find(|model| model.upstream_model == upstream_model.as_str());
        let cost = entry.get("cost");
        let reasoning_efforts = match entry
            .get("reasoning_options")
            .and_then(Value::as_array)
        {
            Some(options) => reasoning_efforts_from_options(options),
            None => previous
                .map(|model| model.reasoning_efforts.clone())
                .unwrap_or_default(),
        };
        merged.push(ProviderTemplateModel {
            upstream_model: upstream_model.clone(),
            display_name: entry
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| previous.and_then(|model| model.display_name.clone())),
            protocol: previous.and_then(|model| model.protocol),
            input: merge_source_price(cost, "input", previous.map(|m| m.input).unwrap_or(0.0)),
            cache_read: merge_source_price(
                cost,
                "cache_read",
                previous.map(|m| m.cache_read).unwrap_or(0.0),
            ),
            cache_write: merge_source_price(
                cost,
                "cache_write",
                previous.map(|m| m.cache_write).unwrap_or(0.0),
            ),
            output: merge_source_price(cost, "output", previous.map(|m| m.output).unwrap_or(0.0)),
            off_peaks: previous
                .map(|model| model.off_peaks.clone())
                .unwrap_or_default(),
            reasoning_efforts,
        });
    }

    if merged.is_empty() {
        return Err("response carries an empty model set".to_string());
    }

    Ok(ModelsDevSource {
        name: non_empty_string(&value, "name"),
        base_url: non_empty_string(&value, "api"),
        models: merged,
    })
}

/// Derive the gateway protocol CommandCode can serve for one entry; a model
/// without `/chat/completions` or `/responses` (for example only `/messages`)
/// is dropped.
fn commandcode_protocol(entry: &Value) -> Option<UpstreamProtocol> {
    let endpoints = entry
        .get("supported_endpoints")
        .and_then(Value::as_array)?;
    let has = |needle: &str| endpoints.iter().any(|endpoint| endpoint.as_str() == Some(needle));
    if has("/chat/completions") {
        Some(UpstreamProtocol::ChatCompletions)
    } else if has("/responses") {
        Some(UpstreamProtocol::Responses)
    } else {
        None
    }
}

/// Parse the CommandCode list payload (`{"object","data":[...]}`) and merge it
/// over the template's current data. The public source only provides the model
/// list, display name and protocol, so curated prices, off-peak windows and
/// reasoning efforts of an existing model are kept.
fn parse_commandcode_source(
    raw: &str,
    current: &ProviderTemplate,
) -> Result<Vec<ProviderTemplateModel>, String> {
    let value =
        parse_source_value(raw).map_err(|error| format!("response is not valid JSON: {error}"))?;
    let entries = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "response is missing the model list".to_string())?;

    let mut merged: Vec<ProviderTemplateModel> = Vec::with_capacity(entries.len());
    for entry in entries {
        let upstream_model = entry
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "response contains a model entry without an identifier".to_string())?
            .to_string();
        let Some(protocol) = commandcode_protocol(entry) else {
            continue;
        };
        if merged
            .iter()
            .any(|model| model.upstream_model == upstream_model)
        {
            continue;
        }
        let previous = current
            .models
            .iter()
            .find(|model| model.upstream_model == upstream_model);
        merged.push(ProviderTemplateModel {
            upstream_model,
            display_name: entry
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| previous.and_then(|model| model.display_name.clone())),
            protocol: Some(protocol),
            input: previous.map(|model| model.input).unwrap_or(0.0),
            cache_read: previous.map(|model| model.cache_read).unwrap_or(0.0),
            cache_write: previous.map(|model| model.cache_write).unwrap_or(0.0),
            output: previous.map(|model| model.output).unwrap_or(0.0),
            off_peaks: previous
                .map(|model| model.off_peaks.clone())
                .unwrap_or_default(),
            reasoning_efforts: previous
                .map(|model| model.reasoning_efforts.clone())
                .unwrap_or_default(),
        });
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

/// Propagate a template update to every derived provider whose `template_id`
/// matches: provider name/base_url/protocol update only while they still equal
/// the previous template values, mapping display name/protocol/reasoning
/// efforts update only while they equal the previous template values (`enabled`
/// and `local_model` are never touched), new official models are added (unless
/// ignored), retired models keep their mapping, and provider-scoped price rows
/// follow the same "only if untouched" rule. Other providers and rows are not
/// touched.
fn propagate_to_derived(
    config: &mut GatewayConfig,
    template_id: &str,
    previous: &ProviderTemplate,
    new_template: &ProviderTemplate,
) {
    let GatewayConfig {
        providers,
        model_prices,
        ..
    } = config;

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
        if provider.protocol == previous.protocol {
            provider.protocol = new_template.protocol;
        }

        for model in &new_template.models {
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
                if mapping.protocol
                    == Some(previous_model.protocol.unwrap_or(previous.protocol))
                {
                    mapping.protocol = Some(effective_protocol);
                }
                if mapping.reasoning_efforts == previous_model.reasoning_efforts {
                    mapping.reasoning_efforts = model.reasoning_efforts.clone();
                }
            } else {
                provider.mappings.push(ModelMapping {
                    local_model: model.upstream_model.clone(),
                    upstream_model: model.upstream_model.clone(),
                    enabled: true,
                    protocol: Some(effective_protocol),
                    display_name: model.display_name.clone(),
                    reasoning_efforts: model.reasoning_efforts.clone(),
                });
            }
        }

        let provider_id = provider.id.clone();
        for model in &new_template.models {
            let previous_model = previous
                .models
                .iter()
                .find(|candidate| candidate.upstream_model == model.upstream_model);
            if let Some(row) = model_prices.iter_mut().find(|row| {
                row.provider_id.as_deref() == Some(provider_id.as_str())
                    && row.upstream_model == model.upstream_model
            }) {
                let untouched = previous_model.is_some_and(|previous_model| {
                    row.input == previous_model.input
                        && row.cache_read == previous_model.cache_read
                        && row.cache_write == previous_model.cache_write
                        && row.output == previous_model.output
                        && row.off_peaks == previous_model.off_peaks
                        && row.off_peak.is_none()
                });
                if untouched {
                    row.input = model.input;
                    row.cache_read = model.cache_read;
                    row.cache_write = model.cache_write;
                    row.output = model.output;
                    row.off_peaks = model.off_peaks.clone();
                    row.off_peak = None;
                }
            } else {
                model_prices.push(ModelPrice {
                    provider_id: Some(provider_id.clone()),
                    upstream_model: model.upstream_model.clone(),
                    input: model.input,
                    cache_read: model.cache_read,
                    cache_write: model.cache_write,
                    output: model.output,
                    off_peaks: model.off_peaks.clone(),
                    off_peak: None,
                });
            }
        }
    }
}

/// Apply one template sync with injectable fetch and persistence seams.
///
/// The public source fields win; fields the source does not provide keep the
/// template's current value. Structural problems and an empty model set are
/// fatal and write nothing. The new template and every derived provider update
/// are staged on a clone, handed to `persist`, and only committed to `config`
/// once persistence succeeds, so a failed write is atomic.
pub fn apply_template_sync_with(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<ProviderTemplateView, String> {
    let previous = effective_template(config, template_id)?;
    let kind = template_source_kind(&previous);
    let label = template_source_label(&previous);

    let raw = fetch(&previous).map_err(|reason| format!("{label}: {reason}"))?;

    let mut next_template = previous.clone();
    match kind {
        TemplateSourceKind::ModelsDev => {
            let source = parse_models_dev_source(&raw, &previous)
                .map_err(|reason| format!("{label}: {reason}"))?;
            next_template.models = source.models;
            if let Some(name) = source.name {
                next_template.name = name;
            }
            if let Some(base_url) = source.base_url {
                next_template.base_url = base_url;
            }
        }
        TemplateSourceKind::CommandCode => {
            next_template.models = parse_commandcode_source(&raw, &previous)
                .map_err(|reason| format!("{label}: {reason}"))?;
        }
    }

    let mut next = config.clone();
    propagate_to_derived(&mut next, template_id, &previous, &next_template);
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

    Ok(ProviderTemplateView {
        template: next_template,
        synced_at: Some(synced_at),
        source,
        from_snapshot: false,
    })
}

/// Fetch the public payload for a template: models.dev's `api.json` for
/// OpenCode Zen, CommandCode's model list for CommandCode. A 15-second timeout
/// and no credentials are used; non-2xx, timeout and network errors name the
/// URL and the reason.
pub(in crate::api_gateway) async fn fetch_template_source(
    template: &ProviderTemplate,
) -> Result<String, String> {
    let url = match template_source_kind(template) {
        TemplateSourceKind::ModelsDev => "https://models.dev/api.json",
        TemplateSourceKind::CommandCode => "https://api.commandcode.ai/provider/v1/models",
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| format!("failed to prepare a request for {url}: {error}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("failed to fetch {url}: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("failed to fetch {url}: HTTP {status}"));
    }
    response
        .text()
        .await
        .map_err(|error| format!("failed to read {url}: {error}"))
}
