//! Provider template snapshot loading and validation.
//!
//! The app ships `provider_templates.json` embedded via `include_str!`; this
//! module parses that snapshot (or a later network payload) into the public
//! [`ProviderTemplate`] model, dropping what the gateway cannot serve instead of
//! failing the whole document. Only structural problems (invalid JSON, an empty
//! or duplicate template id) are fatal.

use super::types_config::{OffPeakPrice, ProviderTemplate, ProviderTemplateModel, UpstreamProtocol};
use serde::Deserialize;

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

/// Rewrite numeric literals that overflow `f64` (for example `1e999`) to `0`,
/// leaving strings and every in-range number untouched.
///
/// `serde_json` rejects such a literal as "number out of range" before any
/// value can be normalized, so an out-of-range price would otherwise fail the
/// whole snapshot. This sanitizer runs only as a fallback after a strict parse
/// failed, so valid documents keep their exact numeric representation.
fn sanitize_out_of_range_numbers(json: &str) -> String {
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
                    _ => out.push('0'),
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
            let sanitized = sanitize_out_of_range_numbers(json);
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
