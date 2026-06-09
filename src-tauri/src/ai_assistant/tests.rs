#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    fn openai_provider(base_url: &str) -> AiAssistantProvider {
        AiAssistantProvider {
            id: "provider-test".to_string(),
            name: "Provider Test".to_string(),
            protocol: "openai-compatible".to_string(),
            base_url: base_url.to_string(),
            auth_scheme: default_bearer(),
            api_key: "sk-test".to_string(),
            enabled: true,
            extra_headers: Vec::new(),
            capabilities: AssistantProviderCapability {
                supports_reasoning: true,
                supports_streaming: true,
                supports_web_search: false,
            },
        }
    }

    fn test_agent() -> AgentDefinition {
        AgentDefinition {
            id: "agent-1".to_string(),
            name: "Search Assistant".to_string(),
            avatar_emoji: None,
            description: String::new(),
            system_prompt: "Be helpful.".to_string(),
            primary_model_id: None,
            light_model_id: None,
            default_model_profile_id: None,
            light_model_profile_id: None,
            tool_policy: AgentToolPolicy {
                web_search: true,
                workspace_read: true,
                notes_search: false,
            },
            knowledge_base_ids: vec!["kb-product".to_string()],
            mcp_server_ids: vec!["mcp-exa".to_string(), "mcp-context7".to_string()],
            memory_enabled: false,
            output_contract: String::new(),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn exa_binding() -> BoundMcpTool {
        BoundMcpTool {
            assistant_tool_name: "mcp__exa__web_search_exa".to_string(),
            server_id: "mcp-exa".to_string(),
            server_name: "Exa MCP".to_string(),
            config_key: "exa".to_string(),
            original_tool_name: "web_search_exa".to_string(),
            category: crate::assistant_mcp::McpCategory::Search,
            definition: ToolDefinition {
                name: "mcp__exa__web_search_exa".to_string(),
                description: "Search the web".to_string(),
                parameters: Some(json!({"type": "object"})),
            },
        }
    }

    fn search_tool_definition() -> ToolDefinition {
        ToolDefinition {
            name: "mcp__exa__web_search_exa".to_string(),
            description: "Search the web".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "numResults": { "type": "integer" },
                    "type": { "type": "string" }
                },
                "required": ["query"]
            })),
        }
    }

    fn workspace_read_definition() -> ToolDefinition {
        ToolDefinition {
            name: "workspace_read".to_string(),
            description: "Read a file from the workspace".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            })),
        }
    }

    #[test]
    fn resolve_provider_endpoint_accepts_full_chat_completion_url() {
        let provider =
            openai_provider("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions");

        let endpoint = resolve_provider_endpoint(&provider, "models");

        assert_eq!(
            endpoint,
            "https://dashscope.aliyuncs.com/compatible-mode/v1/models"
        );
    }

    #[test]
    fn parse_provider_model_catalog_supports_nested_data_models() {
        let provider = openai_provider("https://dashscope.aliyuncs.com/compatible-mode/v1");
        let payload = json!({
            "data": {
                "models": [
                    {
                        "name": "qwen-plus",
                        "display_name": "Qwen Plus",
                        "description": "Aliyun Bailian model"
                    }
                ]
            }
        });

        let catalog = parse_provider_model_catalog(&provider, &payload);

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].model_id, "qwen-plus");
        assert_eq!(catalog[0].label, "Qwen Plus");
    }

    #[test]
    fn unsupported_model_catalog_statuses_are_treated_as_connectivity_only() {
        assert!(is_unsupported_model_catalog_status(
            StatusCode::METHOD_NOT_ALLOWED
        ));
        assert!(is_unsupported_model_catalog_status(StatusCode::NOT_FOUND));
        assert!(is_unsupported_model_catalog_status(
            StatusCode::NOT_IMPLEMENTED
        ));
        assert!(!is_unsupported_model_catalog_status(
            StatusCode::UNAUTHORIZED
        ));
    }

    #[test]
    fn capability_snapshot_uses_conversation_search_toggle() {
        let agent = test_agent();

        let capability = capability_snapshot_from_agent(Some(&agent), false);

        assert!(!capability.web_search);
        assert!(capability.workspace_read);
        assert_eq!(
            capability.mcp_server_ids,
            vec!["mcp-exa".to_string(), "mcp-context7".to_string()]
        );
    }

    #[test]
    fn builtin_tools_do_not_include_legacy_web_search_tool() {
        let tools = build_builtin_tools(&AgentToolPolicy {
            web_search: true,
            workspace_read: true,
            notes_search: true,
        });
        let names = tools.into_iter().map(|tool| tool.name).collect::<Vec<_>>();

        assert!(names.contains(&"workspace_read".to_string()));
        assert!(names.contains(&"notes_search".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
    }

    #[test]
    fn exa_source_extraction_maps_common_result_shape() {
        let output = McpToolCallOutput {
            text: String::new(),
            structured_content: Some(json!({
                "results": [
                    {
                        "title": "Exa Result",
                        "url": "https://example.com/article",
                        "snippet": "Latest facts"
                    }
                ]
            })),
            raw_result: Value::Null,
        };

        let sources = extract_sources_from_mcp_output(&exa_binding(), &output);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "Exa Result");
        assert_eq!(sources[0].url, "https://example.com/article");
        assert_eq!(sources[0].snippet, "Latest facts");
    }

    #[test]
    fn parse_tool_call_arguments_preserves_plain_text_when_json_is_invalid() {
        assert_eq!(
            parse_tool_call_arguments(Some("latest ai news")),
            Value::String("latest ai news".to_string())
        );
        assert_eq!(parse_tool_call_arguments(Some("   ")), Value::Null);
    }

    #[test]
    fn normalize_tool_arguments_maps_plain_string_to_single_required_field() {
        let normalized = normalize_tool_arguments(
            "web_search_exa",
            &Value::String("latest ai news".to_string()),
            Some(&search_tool_definition()),
            None,
        )
        .expect("normalized");

        assert_eq!(normalized, json!({ "query": "latest ai news" }));
    }

    #[test]
    fn normalize_tool_arguments_backfills_required_query_from_alias_field() {
        let normalized = normalize_tool_arguments(
            "web_search_exa",
            &json!({
                "input": "latest ai news",
                "type": "fast"
            }),
            Some(&search_tool_definition()),
            None,
        )
        .expect("normalized");

        assert_eq!(
            normalized,
            json!({
                "input": "latest ai news",
                "type": "fast",
                "query": "latest ai news"
            })
        );
    }

    #[test]
    fn normalize_tool_arguments_uses_user_message_for_missing_search_query() {
        let normalized = normalize_tool_arguments(
            "web_search_exa",
            &json!({}),
            Some(&search_tool_definition()),
            Some("latest ai news"),
        )
        .expect("normalized");

        assert_eq!(normalized, json!({ "query": "latest ai news" }));
    }

    #[test]
    fn normalize_tool_arguments_does_not_treat_search_mode_as_query_text() {
        let normalized = normalize_tool_arguments(
            "web_search_exa",
            &json!({ "type": "fast" }),
            Some(&search_tool_definition()),
            Some("latest ai news"),
        )
        .expect("normalized");

        assert_eq!(
            normalized,
            json!({
                "type": "fast",
                "query": "latest ai news"
            })
        );
    }

    #[test]
    fn normalize_tool_arguments_keeps_non_search_required_fields_strict() {
        let error = normalize_tool_arguments(
            "workspace_read",
            &json!({}),
            Some(&workspace_read_definition()),
            Some("please open README.md"),
        )
        .expect_err("missing path");

        assert!(error.contains("path"));
    }

    #[test]
    fn system_prompt_describes_docs_availability_when_search_tools_are_disabled() {
        let conversation = AssistantConversation {
            id: "conv-1".to_string(),
            title: "Docs".to_string(),
            pinned: false,
            archived: false,
            created_at: 1,
            updated_at: 1,
            assistant_id: None,
            model_profile_id: None,
            model_override_id: None,
            web_search_enabled: false,
            capability_snapshot: None,
            context_reset_count: 0,
            messages: Vec::new(),
        };
        let prompt = build_system_prompt(
            &conversation,
            None,
            &[],
            &[ToolDefinition {
                name: "mcp__context7__query_docs".to_string(),
                description: "Query docs".to_string(),
                parameters: Some(json!({"type": "object"})),
            }],
        );

        assert!(prompt.contains("Search-class MCP tools are disabled"));
        assert!(prompt.contains("Documentation MCP tools may still be available"));
    }
}
