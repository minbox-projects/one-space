fn bound_mcp_server_labels(agent: Option<&AgentDefinition>) -> Vec<String> {
    let Some(agent) = agent else {
        return Vec::new();
    };
    if agent.mcp_server_ids.is_empty() {
        return Vec::new();
    }
    let known = crate::mcp_servers::get_mcp_servers()
        .ok()
        .map(|state| state.servers)
        .unwrap_or_default();
    agent
        .mcp_server_ids
        .iter()
        .map(|server_id| {
            known
                .iter()
                .find(|server| server.id == *server_id)
                .map(|server| format!("{} ({})", server.name, server.id))
                .unwrap_or_else(|| server_id.clone())
        })
        .collect()
}

fn build_memory_summary(conversation: &AssistantConversation) -> Option<String> {
    let mut recent_points = conversation
        .messages
        .iter()
        .filter(|message| message.role == "user")
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())
        .rev()
        .take(3)
        .map(|content| {
            let mut compact = String::new();
            for ch in content.chars().take(120) {
                compact.push(ch);
            }
            compact
        })
        .collect::<Vec<_>>();
    if recent_points.is_empty() {
        return None;
    }
    recent_points.reverse();
    Some(
        recent_points
            .into_iter()
            .map(|item| format!("- {}", item))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn build_system_prompt(
    conversation: &AssistantConversation,
    agent: Option<&AgentDefinition>,
    sources: &[AssistantMessageSource],
    available_tools: &[ToolDefinition],
) -> String {
    let mut sections = Vec::new();
    if let Some(agent) = agent {
        sections.push(agent.system_prompt.clone());
        if !agent.output_contract.trim().is_empty() {
            sections.push(format!("Output contract: {}", agent.output_contract.trim()));
        }
        let capability =
            capability_snapshot_from_agent(Some(agent), conversation.web_search_enabled);
        let mut capability_lines = Vec::new();
        if capability.workspace_read {
            capability_lines.push("Workspace reading is enabled for this assistant.".to_string());
        }
        if capability.notes_search {
            capability_lines.push("Notes search is enabled for this assistant.".to_string());
        }
        if !capability.knowledge_base_ids.is_empty() {
            capability_lines.push(format!(
                "Bound knowledge bases: {}.",
                capability.knowledge_base_ids.join(", ")
            ));
        }
        let mcp_labels = bound_mcp_server_labels(Some(agent));
        if !mcp_labels.is_empty() {
            capability_lines.push(format!("Bound MCP servers: {}.", mcp_labels.join(", ")));
        }
        if capability.memory_enabled {
            capability_lines.push(
                "Memory mode is enabled. Preserve stable preferences and continue prior intent when it helps."
                    .to_string(),
            );
            if let Some(summary) = build_memory_summary(conversation) {
                capability_lines.push(format!("Recent memory cues:\n{}", summary));
            }
        }
        if !capability_lines.is_empty() {
            sections.push(capability_lines.join("\n"));
        }
    } else {
        sections.push(
            "You are OneSpace AI Assistant. Be concise, practical, and cite provided web sources when they exist."
                .to_string(),
        );
    }

    let has_mcp_tools = available_tools
        .iter()
        .any(|tool| tool.name.starts_with("mcp__"));
    if has_mcp_tools {
        sections.push(
            "Bound MCP tools are available. Use the most relevant MCP tool directly when it helps."
                .to_string(),
        );
    }
    if conversation.web_search_enabled {
        sections.push(
            "Search-class MCP tools are enabled for this conversation. Use them for current information when relevant."
                .to_string(),
        );
    } else if has_mcp_tools {
        sections.push(
            "Search-class MCP tools are disabled for this conversation. Documentation MCP tools may still be available."
                .to_string(),
        );
    }

    if !sources.is_empty() {
        let source_lines = sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                format!(
                    "[{}] {} - {} ({})",
                    index + 1,
                    source.title,
                    source.snippet,
                    source.url
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!(
            "Retrieved source context is available. Prefer these sources when relevant:\n{}",
            source_lines
        ));
    }
    sections.join("\n\n")
}
