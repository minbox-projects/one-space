/// Fetch available models from an upstream API for a service provider.
/// Supports both Anthropic Models API and OpenAI-compatible /models endpoint.
fn openai_models_url(base_url: &str) -> String {
    if base_url.is_empty() {
        return "https://api.openai.com/v1/models".to_string();
    }

    let suffixes = [
        "chat/completions",
        "responses",
        "completions",
        "embeddings",
        "audio/speech",
        "audio/transcriptions",
    ];
    let normalized = suffixes
        .iter()
        .find_map(|s| base_url.strip_suffix(s))
        .map(|prefix| prefix.trim_end_matches('/'))
        .unwrap_or(base_url);

    if normalized.ends_with("/models") {
        normalized.to_string()
    } else if normalized.ends_with("/v1") {
        format!("{}/models", normalized)
    } else if normalized.contains("/v1") {
        format!("{}/models", normalized)
    } else {
        format!("{}/v1/models", normalized)
    }
}

#[tauri::command]
pub async fn service_provider_fetch_models(
    provider: serde_json::Value,
) -> Result<Vec<String>, String> {
    let obj = provider
        .as_object()
        .ok_or("provider must be a JSON object")?;

    let tool = obj.get("tool").and_then(|v| v.as_str()).unwrap_or("");

    let default_api_format = if tool == "claude" {
        "anthropic_messages"
    } else {
        "open_ai_chat"
    };

    let api_format = obj
        .get("claude_api_format")
        .and_then(|v| v.as_str())
        .unwrap_or(default_api_format);

    let base_url = obj
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches('/');

    let api_key = obj.get("api_key").and_then(|v| v.as_str()).unwrap_or("");

    if api_key.is_empty() {
        return Err("api_key is required to fetch models".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let models = match api_format {
        "anthropic_messages" => {
            let url = if base_url.is_empty() {
                "https://api.anthropic.com/v1/models".to_string()
            } else {
                format!("{}/v1/models", base_url)
            };
            let resp = client
                .get(&url)
                .header("anthropic-version", "2023-06-01")
                .header("x-api-key", api_key)
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let status = resp.status();
            let body = resp
                .text()
                .await
                .map_err(|e| format!("Failed to read response: {}", e))?;
            if !status.is_success() {
                return Err(format!("API error {}: {}", status.as_u16(), body));
            }
            let json: serde_json::Value =
                serde_json::from_str(&body).map_err(|e| format!("Failed to parse JSON: {}", e))?;
            json.get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            item.get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        "open_ai_chat" | "open_ai_responses" => {
            let url = openai_models_url(base_url);
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await
                .map_err(|e| format!("Request failed: {}", e))?;
            let status = resp.status();
            let body = resp
                .text()
                .await
                .map_err(|e| format!("Failed to read response: {}", e))?;
            if !status.is_success() {
                return Err(format!("API error {}: {}", status.as_u16(), body));
            }
            let json: serde_json::Value =
                serde_json::from_str(&body).map_err(|e| format!("Failed to parse JSON: {}", e))?;
            json.get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            item.get("id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => return Err(format!("Unsupported api format: {}", api_format)),
    };

    Ok(models)
}
