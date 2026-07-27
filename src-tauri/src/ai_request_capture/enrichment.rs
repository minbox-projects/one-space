use super::{AiRequestCaptureHeader, CapturedBody};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CaptureEnrichment {
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) total_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BodyRepresentation {
    pub(crate) data: String,
    pub(crate) encoding: Option<String>,
}

pub(crate) fn enrich(
    upstream_url: &str,
    request_body: &[u8],
    response_body: &[u8],
) -> CaptureEnrichment {
    let provider = detect_provider(upstream_url);
    let Some(provider) = provider else {
        return CaptureEnrichment::default();
    };

    let request_values = json_values(request_body);
    let response_values = json_values(response_body);
    let mut model = request_values.iter().find_map(model_from_value);
    let mut input_tokens = None;
    let mut output_tokens = None;
    let mut total_tokens = None;
    for value in response_values {
        model = model.or_else(|| model_from_value(&value));
        let usage = usage_from_value(&provider, &value);
        input_tokens = usage.input_tokens.or(input_tokens);
        output_tokens = usage.output_tokens.or(output_tokens);
        total_tokens = usage.total_tokens.or(total_tokens);
    }
    model = model.or_else(|| model_from_gemini_path(upstream_url, &provider));
    total_tokens = total_tokens.or_else(|| input_tokens.zip(output_tokens).map(|(a, b)| a + b));

    CaptureEnrichment {
        provider: Some(provider),
        model,
        input_tokens,
        output_tokens,
        total_tokens,
    }
}

pub(crate) fn body_representation(
    headers: &[AiRequestCaptureHeader],
    body: &CapturedBody,
) -> BodyRepresentation {
    let content_type = headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .and_then(|header| header.values.first())
        .map(|value| value.to_ascii_lowercase());
    let valid_utf8 = std::str::from_utf8(&body.data).ok();
    if valid_utf8.is_some() && content_type.as_deref().map_or(true, is_text_content_type) {
        BodyRepresentation {
            data: valid_utf8.unwrap_or_default().to_string(),
            encoding: None,
        }
    } else {
        BodyRepresentation {
            data: BASE64.encode(&body.data),
            encoding: Some("base64".to_string()),
        }
    }
}

fn detect_provider(upstream_url: &str) -> Option<String> {
    let url = url::Url::parse(upstream_url).ok()?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host.contains("openai") {
        Some("openai".to_string())
    } else if host.contains("anthropic") {
        Some("anthropic".to_string())
    } else if host.contains("generativelanguage") {
        Some("gemini".to_string())
    } else {
        None
    }
}

fn json_values(body: &[u8]) -> Vec<Value> {
    let mut values = serde_json::from_slice(body)
        .ok()
        .into_iter()
        .collect::<Vec<_>>();
    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return values,
    };
    for line in text.lines() {
        let value = line
            .trim()
            .strip_prefix("data:")
            .unwrap_or(line.trim())
            .trim();
        if value.is_empty() || value == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str(value) {
            values.push(value);
        }
    }
    values
}

fn model_from_value(value: &Value) -> Option<String> {
    ["model", "modelVersion"]
        .iter()
        .find_map(|key| value.get(key).and_then(Value::as_str).map(str::to_string))
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("model"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

#[derive(Default)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

fn usage_from_value(provider: &str, value: &Value) -> Usage {
    let usage = match provider {
        "openai" => value.get("usage"),
        "anthropic" => value.get("usage").or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("usage"))
        }),
        "gemini" => value.get("usageMetadata"),
        _ => None,
    };
    let Some(usage) = usage else {
        return Usage::default();
    };
    match provider {
        "openai" => Usage {
            input_tokens: number(usage, "prompt_tokens").or_else(|| number(usage, "input_tokens")),
            output_tokens: number(usage, "completion_tokens")
                .or_else(|| number(usage, "output_tokens")),
            total_tokens: number(usage, "total_tokens"),
        },
        "anthropic" => Usage {
            input_tokens: number(usage, "input_tokens"),
            output_tokens: number(usage, "output_tokens"),
            total_tokens: number(usage, "total_tokens"),
        },
        "gemini" => Usage {
            input_tokens: number(usage, "promptTokenCount"),
            output_tokens: number(usage, "candidatesTokenCount"),
            total_tokens: number(usage, "totalTokenCount"),
        },
        _ => Usage::default(),
    }
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn model_from_gemini_path(upstream_url: &str, provider: &str) -> Option<String> {
    if provider != "gemini" {
        return None;
    }
    let path = url::Url::parse(upstream_url).ok()?.path().to_string();
    let (_, model) = path.split_once("/models/")?;
    Some(model.split(':').next()?.to_string()).filter(|model| !model.is_empty())
}

fn is_text_content_type(value: &str) -> bool {
    let mime = value.split(';').next().unwrap_or_default().trim();
    mime.starts_with("text/")
        || mime.ends_with("+json")
        || matches!(
            mime,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/x-www-form-urlencoded"
        )
}
