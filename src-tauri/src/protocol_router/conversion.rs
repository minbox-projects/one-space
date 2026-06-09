fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn anthropic_to_openai_chat(input: &Value, model: &str) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = input.get("system") {
        if let Some(text) = content_to_text(system) {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }
    if let Some(items) = input.get("messages").and_then(|v| v.as_array()) {
        for item in items {
            let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = item
                .get("content")
                .and_then(content_to_text)
                .unwrap_or_default();
            messages.push(json!({ "role": role, "content": content }));
        }
    }
    let mut body = Map::new();
    body.insert("model".to_string(), Value::String(model.to_string()));
    body.insert("messages".to_string(), Value::Array(messages));
    if let Some(max_tokens) = input.get("max_tokens").cloned() {
        body.insert("max_tokens".to_string(), max_tokens);
    }
    if let Some(temperature) = input.get("temperature").cloned() {
        body.insert("temperature".to_string(), temperature);
    }
    if let Some(stream) = input.get("stream").cloned() {
        body.insert("stream".to_string(), stream);
    }
    if let Some(tools) = anthropic_tools_to_openai(input.get("tools")) {
        body.insert("tools".to_string(), tools);
    }
    if let Some(tool_choice) = input.get("tool_choice").cloned() {
        body.insert(
            "tool_choice".to_string(),
            anthropic_tool_choice_to_openai(tool_choice),
        );
    }
    Value::Object(body)
}

fn anthropic_to_openai_responses(input: &Value, model: &str) -> Value {
    let mut output = Map::new();
    output.insert("model".to_string(), Value::String(model.to_string()));
    output.insert(
        "input".to_string(),
        Value::String(anthropic_messages_to_prompt(input)),
    );
    if let Some(max_tokens) = input.get("max_tokens").cloned() {
        output.insert("max_output_tokens".to_string(), max_tokens);
    }
    if let Some(temperature) = input.get("temperature").cloned() {
        output.insert("temperature".to_string(), temperature);
    }
    if let Some(stream) = input.get("stream").cloned() {
        output.insert("stream".to_string(), stream);
    }
    if let Some(tools) = anthropic_tools_to_responses(input.get("tools")) {
        output.insert("tools".to_string(), tools);
    }
    Value::Object(output)
}

fn anthropic_messages_to_prompt(input: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(system) = input.get("system").and_then(content_to_text) {
        parts.push(format!("system: {system}"));
    }
    if let Some(items) = input.get("messages").and_then(|v| v.as_array()) {
        for item in items {
            let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let content = item
                .get("content")
                .and_then(content_to_text)
                .unwrap_or_default();
            parts.push(format!("{role}: {content}"));
        }
    }
    parts.join("\n")
}

fn content_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                        item.get("text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    } else if item.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                        item.get("content").and_then(content_to_text)
                    } else if item.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
                        let input = item.get("input").cloned().unwrap_or(Value::Null);
                        Some(format!("tool_use {name}: {input}"))
                    } else if let Some(s) = item.as_str() {
                        Some(s.to_string())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            Some(parts.join("\n"))
        }
        _ => None,
    }
}

fn anthropic_tools_to_openai(tools: Option<&Value>) -> Option<Value> {
    let tools = tools?.as_array()?;
    let mapped = tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(|v| v.as_str())?;
            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let parameters = tool
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
            Some(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": parameters
                }
            }))
        })
        .collect::<Vec<_>>();
    if mapped.is_empty() {
        None
    } else {
        Some(Value::Array(mapped))
    }
}

fn anthropic_tools_to_responses(tools: Option<&Value>) -> Option<Value> {
    let tools = tools?.as_array()?;
    let mapped = tools
        .iter()
        .filter_map(|tool| {
            let name = tool.get("name").and_then(|v| v.as_str())?;
            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let parameters = tool
                .get("input_schema")
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
            Some(json!({
                "type": "function",
                "name": name,
                "description": description,
                "parameters": parameters
            }))
        })
        .collect::<Vec<_>>();
    if mapped.is_empty() {
        None
    } else {
        Some(Value::Array(mapped))
    }
}

fn anthropic_tool_choice_to_openai(value: Value) -> Value {
    if value.get("type").and_then(|v| v.as_str()) == Some("tool") {
        if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
            return json!({ "type": "function", "function": { "name": name } });
        }
    }
    match value.get("type").and_then(|v| v.as_str()) {
        Some("auto") => Value::String("auto".to_string()),
        Some("any") => Value::String("required".to_string()),
        Some("none") => Value::String("none".to_string()),
        _ => Value::String("auto".to_string()),
    }
}

fn upstream_to_anthropic(value: &Value, model: &str) -> Value {
    if let Some(choice) = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|items| items.first())
    {
        let text = choice
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|v| v.as_str())
            .or_else(|| choice.get("text").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(json!({ "type": "text", "text": text }));
        }
        if let Some(tool_calls) = choice
            .get("message")
            .and_then(|message| message.get("tool_calls"))
            .and_then(|v| v.as_array())
        {
            for call in tool_calls {
                let id = call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("toolu_proxy");
                let function = call.get("function").unwrap_or(&Value::Null);
                let name = function
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool");
                let arguments = function
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or_else(|| json!({}));
                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": arguments
                }));
            }
        }
        if content.is_empty() {
            content.push(json!({ "type": "text", "text": "" }));
        }
        let (input_tokens, output_tokens, _) = usage_from_value(value);
        return json!({
            "id": value.get("id").cloned().unwrap_or_else(|| Value::String(format!("msg_{}", uuid::Uuid::new_v4().simple()))),
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": content,
            "stop_reason": if content.iter().any(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_use")) { "tool_use" } else { "end_turn" },
            "stop_sequence": Value::Null,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens
            }
        });
    }
    let text = value
        .get("output_text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| extract_responses_output_text(value))
        .unwrap_or_default();
    let (input_tokens, output_tokens, _) = usage_from_value(value);
    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| Value::String(format!("msg_{}", uuid::Uuid::new_v4().simple()))),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{ "type": "text", "text": text }],
        "stop_reason": "end_turn",
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    })
}

fn extract_responses_output_text(value: &Value) -> Option<String> {
    let output = value.get("output")?.as_array()?;
    let mut parts = Vec::new();
    for item in output {
        if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
            for block in content {
                if let Some(text) = block
                    .get("text")
                    .and_then(|v| v.as_str())
                    .or_else(|| block.get("output_text").and_then(|v| v.as_str()))
                {
                    parts.push(text.to_string());
                }
            }
        }
    }
    Some(parts.join("\n"))
}

fn usage_from_value(value: &Value) -> (u64, u64, u64) {
    let Some(usage) = value.get("usage") else {
        return (0, 0, 0);
    };
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = usage
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(input + output);
    (input, output, total)
}

fn error_summary(value: &Value) -> String {
    let message = value
        .get("error")
        .and_then(|error| {
            error
                .get("message")
                .and_then(|v| v.as_str())
                .or_else(|| error.as_str())
        })
        .or_else(|| value.get("message").and_then(|v| v.as_str()))
        .unwrap_or("upstream request failed")
        .chars()
        .take(240)
        .collect::<String>();
    let lower = message.to_ascii_lowercase();
    if lower.contains("auth") || lower.contains("unauthorized") || lower.contains("api key") {
        format!("auth_failed: {message}")
    } else if lower.contains("model") {
        format!("upstream_model_error: {message}")
    } else {
        format!("upstream_http_error: {message}")
    }
}
