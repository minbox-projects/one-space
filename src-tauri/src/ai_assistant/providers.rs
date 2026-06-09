#[derive(Debug)]
struct ProviderCatalogFetchError {
    message: String,
    unsupported_catalog_endpoint: bool,
}

fn is_unsupported_model_catalog_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 404 | 405 | 501)
}

fn catalog_tags_from_model_id(model_id: &str) -> Vec<String> {
    let lower = model_id.to_lowercase();
    let mut tags = Vec::new();
    if lower.contains("mini") || lower.contains("small") {
        tags.push("light".to_string());
    }
    if lower.contains("reason") || lower.contains("o1") || lower.contains("o3") {
        tags.push("reasoning".to_string());
    }
    if lower.contains("vision") || lower.contains("vl") {
        tags.push("vision".to_string());
    }
    tags
}

fn parse_provider_model_catalog(
    provider: &AiAssistantProvider,
    payload: &Value,
) -> Vec<ModelCatalogItem> {
    let now = now_ts();
    let items = payload
        .get("data")
        .and_then(|value| value.as_array())
        .cloned()
        .or_else(|| {
            payload
                .get("data")
                .and_then(|value| value.get("models"))
                .and_then(|value| value.as_array())
                .cloned()
        })
        .or_else(|| {
            payload
                .get("models")
                .and_then(|value| value.as_array())
                .cloned()
        })
        .or_else(|| {
            payload
                .get("result")
                .and_then(|value| value.as_array())
                .cloned()
        })
        .unwrap_or_default();

    let mut seen = HashSet::new();
    let mut catalog = Vec::new();
    for item in items {
        let raw_id = item
            .get("id")
            .and_then(|value| value.as_str())
            .or_else(|| item.get("name").and_then(|value| value.as_str()))
            .unwrap_or("")
            .trim()
            .trim_start_matches("models/");
        if raw_id.is_empty() {
            continue;
        }
        let id = catalog_model_id(&provider.id, raw_id);
        if !seen.insert(id.clone()) {
            continue;
        }
        let label = item
            .get("display_name")
            .and_then(|value| value.as_str())
            .or_else(|| item.get("displayName").and_then(|value| value.as_str()))
            .unwrap_or(raw_id)
            .trim()
            .to_string();
        catalog.push(ModelCatalogItem {
            id,
            provider_id: provider.id.clone(),
            model_id: raw_id.to_string(),
            label,
            description: item
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            enabled: true,
            tags: catalog_tags_from_model_id(raw_id),
            supports_reasoning: provider.capabilities.supports_reasoning,
            supports_streaming: provider.capabilities.supports_streaming,
            supports_web_search: provider.capabilities.supports_web_search,
            created_at: now,
            updated_at: now,
        });
    }
    catalog
}

async fn fetch_provider_model_catalog_detailed(
    provider: &AiAssistantProvider,
) -> Result<Vec<ModelCatalogItem>, ProviderCatalogFetchError> {
    if provider.api_key.trim().is_empty() {
        return Err(ProviderCatalogFetchError {
            message: "Provider API key is empty".to_string(),
            unsupported_catalog_endpoint: false,
        });
    }
    let client = build_reqwest_client(Some(12)).map_err(|message| ProviderCatalogFetchError {
        message,
        unsupported_catalog_endpoint: false,
    })?;
    let endpoint = resolve_provider_endpoint(provider, "models");
    let mut request = client.get(endpoint);
    if provider.protocol == "anthropic-messages" {
        request = request.header("anthropic-version", "2023-06-01");
    }
    let request =
        apply_provider_headers(request, provider).map_err(|message| ProviderCatalogFetchError {
            message,
            unsupported_catalog_endpoint: false,
        })?;
    let response = request
        .send()
        .await
        .map_err(|e| ProviderCatalogFetchError {
            message: e.to_string(),
            unsupported_catalog_endpoint: false,
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let details = body.trim();
        let unsupported_catalog_endpoint = is_unsupported_model_catalog_status(status);
        let message = if details.is_empty() {
            format!("Provider model fetch failed: {}", status)
        } else {
            format!("Provider model fetch failed: {} - {}", status, details)
        };
        let message = if unsupported_catalog_endpoint {
            format!(
                "{}. This provider does not expose a standard model catalog endpoint.",
                message
            )
        } else {
            message
        };
        return Err(ProviderCatalogFetchError {
            message,
            unsupported_catalog_endpoint,
        });
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|e| ProviderCatalogFetchError {
            message: e.to_string(),
            unsupported_catalog_endpoint: false,
        })?;
    let catalog = parse_provider_model_catalog(provider, &payload);
    if catalog.is_empty() {
        return Err(ProviderCatalogFetchError {
            message: "Provider returned no models".to_string(),
            unsupported_catalog_endpoint: false,
        });
    }
    Ok(catalog)
}

async fn fetch_provider_model_catalog(
    provider: &AiAssistantProvider,
) -> Result<Vec<ModelCatalogItem>, String> {
    fetch_provider_model_catalog_detailed(provider)
        .await
        .map_err(|error| error.message)
}

fn text_from_openai_message(message: &Value) -> String {
    if let Some(text) = message.get("content").and_then(|content| content.as_str()) {
        return text.to_string();
    }
    if let Some(items) = message
        .get("content")
        .and_then(|content| content.as_array())
    {
        return items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("content").and_then(|value| value.as_str()))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

fn reasoning_from_openai_message(message: &Value) -> Option<String> {
    message
        .get("reasoning")
        .and_then(value_to_text)
        .or_else(|| message.get("reasoning_content").and_then(value_to_text))
        .or_else(|| message.get("reasoning_summary").and_then(value_to_text))
}

fn value_to_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(items) = value.as_array() {
        let joined = items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|val| val.as_str())
                    .or_else(|| item.get("content").and_then(|val| val.as_str()))
                    .or_else(|| item.as_str())
                    .map(|text| text.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }
    None
}
