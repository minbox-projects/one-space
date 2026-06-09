use super::*;
use serde_json::json;
use serde_json::Map;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn protocol_router_service_provider_uri_uses_provider_id_route() {
    let provider_id = "3be11230-a785-4b2a-ae95-54ee4a0252e8";

    assert_eq!(route_id_for_claude_provider(provider_id), provider_id);
    assert_eq!(
        parse_anthropic_route_id(&format!(
            "/protocol-router/service-providers/{provider_id}/anthropic/v1/messages"
        ))
        .as_deref(),
        Some(provider_id)
    );
    assert_eq!(
        parse_anthropic_route_id(&format!(
            "/protocol-router/service-providers/{provider_id}/anthropic/v1/messages?beta=true"
        ))
        .as_deref(),
        Some(provider_id)
    );
}

#[test]
fn protocol_router_service_provider_uri_rejects_legacy_single_segment_route() {
    let provider_id = "3be11230-a785-4b2a-ae95-54ee4a0252e8";

    assert_eq!(
        parse_anthropic_route_id(&format!(
            "/anthropic/service-providers/{provider_id}/v1/messages"
        )),
        None
    );
}

#[test]
fn parses_openai_models_catalog() {
    let value = json!({
        "object": "list",
        "data": [
            { "id": "kimi-k2.6", "object": "model", "created": 1, "owned_by": "moonshot" },
            { "id": "gpt-5.1", "object": "model", "created": 2, "owned_by": "openai" }
        ]
    });
    let models = parse_openai_models_catalog(&value, None).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "kimi-k2.6");
    assert_eq!(models[1].owned_by.as_deref(), Some("openai"));
}

#[test]
fn parses_opencode_go_fixture_shape() {
    let value = json!({
        "object": "list",
        "data": [
            { "id": "claude-sonnet-4", "object": "model", "created": 0, "owned_by": "opencode-go" }
        ]
    });
    let models = parse_openai_models_catalog(&value, Some("go:")).unwrap();
    assert_eq!(models[0].id, "go:claude-sonnet-4");
}

#[test]
fn opencode_go_style_routes_should_use_openai_responses_endpoint() {
    let provider = crate::app_store::ServiceProviderRecord {
        id: "opencode-go".to_string(),
        name: "OpenCode Go".to_string(),
        tool: "claude".to_string(),
        icon: None,
        api_key: "sk-test".to_string(),
        base_url: Some("https://opencode.ai/zen/go/v1".to_string()),
        model: Some("claude-sonnet-4".to_string()),
        claude_api_format: "open_ai_responses".to_string(),
        claude_connection_mode: "protocol_router".to_string(),
        protocol_router_upstream_provider_id: None,
        protocol_router_wire_api: "open_ai_responses".to_string(),
        claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
        claude_model_mappings: vec![],
        claude_enable_tool_search: None,
        claude_auto_memory_enabled: None,
        claude_always_thinking_enabled: None,
        claude_away_summary_enabled: None,
        claude_include_git_instructions: None,
        claude_enable_attribution: None,
        code: Some("opencode-go".to_string()),
        is_enabled: Some(true),
        provider_key: None,
        favorite_at: None,
        env_managed: Some(true),
        tool_config: Map::new(),
        history: vec![],
        extra: Map::new(),
        fetched_models: None,
    };

    let route = route_from_claude_provider(&provider).unwrap();
    assert_eq!(route.wire_api, WireApi::OpenAiResponses);
    assert_eq!(
        join_url(&route.base_url, "responses"),
        "https://opencode.ai/zen/go/v1/responses"
    );
}

#[test]
fn route_from_claude_provider_prefers_claude_default_model_over_first_mapping() {
    let provider = crate::app_store::ServiceProviderRecord {
        id: "router-claude".to_string(),
        name: "Router Claude".to_string(),
        tool: "claude".to_string(),
        icon: None,
        api_key: "sk-test".to_string(),
        base_url: Some("https://example.com/v1".to_string()),
        model: Some("claude-sonnet-4-5".to_string()),
        claude_api_format: "open_ai_chat".to_string(),
        claude_connection_mode: "protocol_router".to_string(),
        protocol_router_upstream_provider_id: None,
        protocol_router_wire_api: "open_ai_chat".to_string(),
        claude_auth_env_key: "ANTHROPIC_API_KEY".to_string(),
        claude_model_mappings: vec![
            crate::app_store::ClaudeModelMapping {
                family: "haiku".to_string(),
                display_name: "Haiku".to_string(),
                upstream_model: "qwen-haiku-upstream".to_string(),
                supports_1m: Some(false),
                supported_capabilities: None,
            },
            crate::app_store::ClaudeModelMapping {
                family: "sonnet".to_string(),
                display_name: "Sonnet".to_string(),
                upstream_model: "qwen-sonnet-upstream".to_string(),
                supports_1m: Some(false),
                supported_capabilities: None,
            },
        ],
        claude_enable_tool_search: None,
        claude_auto_memory_enabled: None,
        claude_always_thinking_enabled: None,
        claude_away_summary_enabled: None,
        claude_include_git_instructions: None,
        claude_enable_attribution: None,
        code: Some("router-claude".to_string()),
        is_enabled: Some(true),
        provider_key: None,
        favorite_at: None,
        env_managed: Some(true),
        tool_config: Map::new(),
        history: vec![],
        extra: Map::new(),
        fetched_models: None,
    };

    let route = route_from_claude_provider(&provider).unwrap();
    assert_eq!(route.default_model.as_deref(), Some("claude-sonnet-4-5"));
}

#[test]
fn converts_anthropic_to_openai_chat() {
    let input = json!({
        "model": "sonnet",
        "system": "be brief",
        "messages": [
            { "role": "user", "content": [{ "type": "text", "text": "hello" }] }
        ],
        "max_tokens": 42
    });
    let output = anthropic_to_openai_chat(&input, "kimi-k2.6");
    assert_eq!(output["model"], "kimi-k2.6");
    assert_eq!(output["messages"][0]["role"], "system");
    assert_eq!(output["messages"][1]["content"], "hello");
    assert_eq!(output["max_tokens"], 42);
}

#[test]
fn converts_openai_chat_to_anthropic() {
    let input = json!({
        "id": "chatcmpl_1",
        "choices": [{ "message": { "content": "hi" } }],
        "usage": { "prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7 }
    });
    let output = upstream_to_anthropic(&input, "kimi-k2.6");
    assert_eq!(output["type"], "message");
    assert_eq!(output["content"][0]["text"], "hi");
    assert_eq!(output["usage"]["input_tokens"], 3);
    assert_eq!(output["usage"]["output_tokens"], 4);
}

#[test]
fn converts_tools_to_openai_chat_tools() {
    let input = json!({
        "model": "sonnet",
        "messages": [{ "role": "user", "content": "use a tool" }],
        "tools": [{
            "name": "read_file",
            "description": "Read a file",
            "input_schema": { "type": "object", "properties": { "path": { "type": "string" } } }
        }]
    });
    let output = anthropic_to_openai_chat(&input, "kimi-k2.6");
    assert_eq!(output["tools"][0]["type"], "function");
    assert_eq!(output["tools"][0]["function"]["name"], "read_file");
}

#[test]
fn converts_openai_tool_call_to_anthropic_tool_use() {
    let input = json!({
        "id": "chatcmpl_1",
        "choices": [{
            "message": {
                "content": "",
                "tool_calls": [{
                    "id": "call_1",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"README.md\"}"
                    }
                }]
            }
        }]
    });
    let output = upstream_to_anthropic(&input, "kimi-k2.6");
    assert_eq!(output["stop_reason"], "tool_use");
    assert_eq!(output["content"][0]["type"], "tool_use");
    assert_eq!(output["content"][0]["name"], "read_file");
    assert_eq!(output["content"][0]["input"]["path"], "README.md");
}

#[test]
fn converts_openai_sse_to_anthropic_sse() {
    let input = br#"data: {"choices":[{"delta":{"content":"hello"}}]}
data: {"choices":[{"delta":{"content":" world"}}]}
data: [DONE]
"#;
    let output = String::from_utf8(openai_sse_to_anthropic_sse(input, "kimi-k2.6")).unwrap();
    assert!(output.contains("event: message_start"));
    assert!(output.contains("event: content_block_delta"));
    assert!(output.contains("hello"));
    assert!(output.contains("world"));
    assert!(output.contains("event: message_stop"));
}

#[test]
fn converts_openai_sse_tool_call_to_anthropic_tool_use_stream() {
    let input = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read_file","arguments":"{\"path\""}}]}}]}
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"README.md\"}"}}]}}]}
data: [DONE]
"#;
    let output = String::from_utf8(openai_sse_to_anthropic_sse(input, "kimi-k2.6")).unwrap();
    assert!(output.contains("\"type\":\"tool_use\""));
    assert!(output.contains("\"name\":\"read_file\""));
    assert!(output.contains("input_json_delta"));
    assert!(output.contains("README.md"));
    assert!(output.contains("\"stop_reason\":\"tool_use\""));
}

#[test]
fn prunes_by_retention_days() {
    let now = now_ts();
    let mut calls = vec![
        ProtocolRouterCallRecord {
            ts: now.saturating_sub(40 * 24 * 60 * 60),
            route_id: "old".into(),
            provider: "p".into(),
            model: "m".into(),
            endpoint: "/v1/messages".into(),
            wire_api: WireApi::OpenAiChat,
            status: 200,
            latency_ms: 1,
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
            error_summary: None,
        },
        ProtocolRouterCallRecord {
            ts: now,
            route_id: "new".into(),
            provider: "p".into(),
            model: "m".into(),
            endpoint: "/v1/messages".into(),
            wire_api: WireApi::OpenAiChat,
            status: 200,
            latency_ms: 1,
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
            error_summary: None,
        },
    ];
    prune_calls(&mut calls, 30);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].route_id, "new");
}

async fn spawn_mock_server(response_body: &'static str) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tauri::async_runtime::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    format!("http://{}", addr)
}

#[tokio::test]
async fn forwards_openai_chat_to_mock_endpoint() {
    let base = spawn_mock_server(
        r#"{"id":"chatcmpl_mock","choices":[{"message":{"content":"mock ok"}}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}"#,
    )
    .await;
    let route = ProtocolRoute {
        id: "mock".to_string(),
        name: "Mock".to_string(),
        claude_provider_id: "claude-mock".to_string(),
        claude_provider_name: "Claude Mock".to_string(),
        upstream_provider_id: "mock".to_string(),
        upstream_provider_name: "Mock".to_string(),
        base_url: base,
        auth_header: Some("Authorization".to_string()),
        api_key: String::new(),
        wire_api: WireApi::OpenAiChat,
        default_model: Some("mock-model".to_string()),
        mappings: Vec::new(),
        enabled: true,
    };
    let input = json!({
        "model": "mock-model",
        "messages": [{ "role": "user", "content": "hello" }],
        "max_tokens": 8
    });
    let result = forward_request(&route, &input, "mock-model").await.unwrap();
    let UpstreamResult::Json { status, body } = result else {
        panic!("expected json response");
    };
    assert_eq!(status, 200);
    let anthropic = upstream_to_anthropic(&body, "mock-model");
    assert_eq!(anthropic["content"][0]["text"], "mock ok");
    assert_eq!(anthropic["usage"]["output_tokens"], 3);
}
