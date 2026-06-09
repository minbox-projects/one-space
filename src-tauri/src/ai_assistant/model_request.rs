use super::{
    apply_provider_headers, build_available_tools, build_context_messages, build_model_params,
    build_reqwest_client, build_system_prompt, build_tool_call_snapshot,
    capability_snapshot_from_agent, close_mcp_clients, derive_title, execute_tool_call,
    legacy_profile_catalog_id, load_bound_mcp_tools, load_state, now_ts, parse_tool_call_arguments,
    read_sse_response, reasoning_from_openai_message, resolve_provider, resolve_provider_endpoint,
    resolve_runtime_profile, running_schedules, save_state, schedule_assistant_id,
    text_from_openai_message, AgentDefinition, AiAssistantModelProfile, AiAssistantProvider,
    AssistantConversation, AssistantMessage, AssistantMessageSource, AssistantStreamEvent,
    AssistantToolCall, ScheduleJob, ScheduleJobView, ScheduleRun, ScheduleTrigger, ToolDefinition,
    ASSISTANT_STREAM_EVENT,
};
use chrono::{Datelike, Timelike, Weekday};
use serde_json::{json, Value};
use std::collections::HashMap;
use tauri::Emitter;

pub(in crate::ai_assistant) fn text_from_openai_delta(delta: &Value) -> String {
    if let Some(text) = delta.get("content").and_then(|content| content.as_str()) {
        return text.to_string();
    }
    if let Some(items) = delta.get("content").and_then(|content| content.as_array()) {
        return items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("content").and_then(|value| value.as_str()))
            })
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}

pub(in crate::ai_assistant) async fn run_model_request_with_tools(
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
    tools: &[ToolDefinition],
) -> Result<(String, Option<String>, Vec<AssistantToolCall>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_provider_endpoint(provider, "chat/completions");

    let mut messages = vec![json!({
        "role": "system",
        "content": system_prompt,
    })];

    for (role, content) in context {
        if role == "tool" {
            if let Ok(tool_msg) = serde_json::from_str::<Value>(content) {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": tool_msg.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
        } else {
            messages.push(json!({
                "role": role,
                "content": content,
            }));
        }
    }

    let model_params = build_model_params(profile);
    let mut payload = json!({
        "model": profile.model_id,
        "messages": messages,
    });
    if let Some(temp) = model_params.get("temperature") {
        payload["temperature"] = temp.clone();
    }
    if let Some(max_tokens) = model_params.get("max_tokens") {
        payload["max_tokens"] = max_tokens.clone();
    }
    if let Some(top_p) = model_params.get("top_p") {
        payload["top_p"] = top_p.clone();
    }
    if let Some(freq) = model_params.get("frequency_penalty") {
        payload["frequency_penalty"] = freq.clone();
    }
    if let Some(pres) = model_params.get("presence_penalty") {
        payload["presence_penalty"] = pres.clone();
    }
    if let Some(stop) = model_params.get("stop") {
        payload["stop"] = stop.clone();
    }

    if !tools.is_empty() {
        payload["tools"] = json!(tools
            .iter()
            .map(|t| json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            }))
            .collect::<Vec<_>>());
    }

    let request = client.post(endpoint).json(&payload);
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let body: Value = response.json().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(body.to_string());
    }

    let message = body
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| "Missing message in response".to_string())?;

    let content = text_from_openai_message(message);
    let reasoning = reasoning_from_openai_message(message);

    let tool_calls = message
        .get("tool_calls")
        .and_then(|value| value.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if !id.is_empty() && !name.is_empty() {
                        Some(build_tool_call_snapshot(
                            id,
                            name,
                            arguments,
                            "pending",
                            None,
                            None,
                            now_ts(),
                            None,
                            None,
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Ok((content, reasoning, tool_calls))
}

pub(in crate::ai_assistant) async fn run_model_request_with_tools_streaming(
    app: &tauri::AppHandle,
    conversation_id: &str,
    message_id: &str,
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
    tools: &[ToolDefinition],
) -> Result<(String, Option<String>, Vec<AssistantToolCall>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_provider_endpoint(provider, "chat/completions");

    let mut messages = vec![json!({
        "role": "system",
        "content": system_prompt,
    })];

    for (role, content) in context {
        if role == "tool" {
            if let Ok(tool_msg) = serde_json::from_str::<Value>(content) {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": tool_msg.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
        } else {
            messages.push(json!({
                "role": role,
                "content": content,
            }));
        }
    }

    let model_params = build_model_params(profile);
    let mut payload = json!({
        "model": profile.model_id,
        "messages": messages,
        "stream": true,
    });
    if let Some(temp) = model_params.get("temperature") {
        payload["temperature"] = temp.clone();
    }
    if let Some(max_tokens) = model_params.get("max_tokens") {
        payload["max_tokens"] = max_tokens.clone();
    }
    if let Some(top_p) = model_params.get("top_p") {
        payload["top_p"] = top_p.clone();
    }
    if let Some(freq) = model_params.get("frequency_penalty") {
        payload["frequency_penalty"] = freq.clone();
    }
    if let Some(pres) = model_params.get("presence_penalty") {
        payload["presence_penalty"] = pres.clone();
    }
    if let Some(stop) = model_params.get("stop") {
        payload["stop"] = stop.clone();
    }

    if !tools.is_empty() {
        payload["tools"] = json!(tools
            .iter()
            .map(|t| json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            }))
            .collect::<Vec<_>>());
    }

    let request = client.post(endpoint).json(&payload);
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tool_calls_map: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    let mut tool_call_order: Vec<String> = Vec::new();

    read_sse_response(response, |_, data| {
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let payload: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;

        if let Some(delta) = payload
            .get("choices")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|choice| choice.get("delta"))
        {
            let text_delta = text_from_openai_delta(delta);
            if !text_delta.is_empty() {
                content.push_str(&text_delta);
                emit_stream_event(
                    app,
                    AssistantStreamEvent {
                        conversation_id: conversation_id.to_string(),
                        message_id: message_id.to_string(),
                        kind: "message.delta".to_string(),
                        text: Some(text_delta),
                        sources: None,
                        tool: None,
                        error: None,
                    },
                );
            }

            if let Some(reasoning_delta) = reasoning_from_openai_message(delta) {
                if !reasoning_delta.is_empty() {
                    reasoning.push_str(&reasoning_delta);
                    emit_stream_event(
                        app,
                        AssistantStreamEvent {
                            conversation_id: conversation_id.to_string(),
                            message_id: message_id.to_string(),
                            kind: "reasoning.delta".to_string(),
                            text: Some(reasoning_delta),
                            sources: None,
                            tool: None,
                            error: None,
                        },
                    );
                }
            }

            if let Some(tool_calls_delta) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tool_calls_delta {
                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                        tool_call_order.push(id.to_string());
                    }
                    let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let id = tc
                        .get("id")
                        .and_then(|v| v.as_str())
                        .or_else(|| tool_call_order.get(idx).map(|s| s.as_str()))
                        .unwrap_or("");
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let args = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if !id.is_empty() {
                        tool_calls_map
                            .entry(id.to_string())
                            .or_insert_with(|| (String::new(), String::new()));
                        let entry = tool_calls_map.get_mut(id).unwrap();
                        if !name.is_empty() {
                            entry.0 = name.to_string();
                        }
                        entry.1.push_str(args);
                    }
                }
            }
        }
        Ok(())
    })
    .await?;

    let tool_calls: Vec<AssistantToolCall> = tool_call_order
        .iter()
        .filter_map(|id| {
            tool_calls_map.get(id).map(|(name, args)| {
                build_tool_call_snapshot(
                    id.clone(),
                    name.clone(),
                    Some(args.clone()),
                    "pending",
                    None,
                    None,
                    now_ts(),
                    None,
                    None,
                )
            })
        })
        .collect();

    let reasoning = if reasoning.trim().is_empty() {
        None
    } else {
        Some(reasoning)
    };

    Ok((content, reasoning, tool_calls))
}

pub(in crate::ai_assistant) fn emit_stream_event(
    app: &tauri::AppHandle,
    payload: AssistantStreamEvent,
) {
    let _ = app.emit(ASSISTANT_STREAM_EVENT, payload);
}

pub(in crate::ai_assistant) fn save_message_result(
    conversation_id: &str,
    message_id: &str,
    content: &str,
    reasoning: Option<String>,
    sources: Vec<AssistantMessageSource>,
    tool_calls: Vec<AssistantToolCall>,
    status: &str,
) -> Result<(), String> {
    let mut state = load_state()?;
    let conversation = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    let message = conversation
        .messages
        .iter_mut()
        .find(|message| message.id == message_id)
        .ok_or_else(|| "Message not found".to_string())?;
    message.content = content.to_string();
    message.reasoning = reasoning;
    message.sources = sources;
    message.tool_calls = tool_calls;
    message.status = status.to_string();
    conversation.updated_at = now_ts();
    if conversation.title.trim().is_empty() {
        if let Some(user_message) = conversation
            .messages
            .iter()
            .find(|item| item.role == "user")
        {
            conversation.title = derive_title(&user_message.content);
        }
    }
    save_state(&state)
}

pub(in crate::ai_assistant) async fn execute_workspace_conversation_run(
    app: tauri::AppHandle,
    conversation_id: String,
    assistant_message_id: String,
    explicit_model_id: Option<String>,
    explicit_assistant_id: Option<String>,
    _force_web_search: Option<bool>,
) -> Result<(), String> {
    let state = load_state()?;
    let conversation = state
        .conversations
        .iter()
        .find(|item| item.id == conversation_id)
        .cloned()
        .ok_or_else(|| "Conversation not found".to_string())?;
    let assistant = explicit_assistant_id
        .as_deref()
        .or(conversation.assistant_id.as_deref())
        .and_then(|id| state.agents.iter().find(|item| item.id == id))
        .cloned();
    let role = if assistant.is_some() {
        "assistant"
    } else {
        "chat"
    };
    let profile = resolve_runtime_profile(
        &state,
        explicit_model_id
            .as_deref()
            .or(conversation.model_override_id.as_deref()),
        assistant.as_ref(),
        role,
    )?;
    let provider = resolve_provider(&state, &profile)?.clone();
    if !provider.enabled {
        return Err(format!("Model provider is disabled: {}", provider.name));
    }
    if provider.api_key.trim().is_empty() {
        return Err(format!(
            "Model provider API key is empty: {}",
            provider.name
        ));
    }

    let mut tool_policy = assistant
        .as_ref()
        .map(|a| a.tool_policy.clone())
        .unwrap_or_default();
    tool_policy.web_search = conversation.web_search_enabled;

    let (mut mcp_clients, mcp_tools_by_name) =
        load_bound_mcp_tools(assistant.as_ref(), tool_policy.web_search).await?;
    let mut mcp_tools = mcp_tools_by_name.values().cloned().collect::<Vec<_>>();
    mcp_tools.sort_by(|a, b| a.assistant_tool_name.cmp(&b.assistant_tool_name));
    let available_tools = build_available_tools(&tool_policy, &mcp_tools);
    let available_tools_by_name = available_tools
        .iter()
        .cloned()
        .map(|tool| (tool.name.clone(), tool))
        .collect::<HashMap<_, _>>();

    let mut all_tool_calls = Vec::new();
    let mut all_sources = Vec::new();
    let mut accumulated_content = String::new();
    let mut accumulated_reasoning: Option<String> = None;

    let max_tool_iterations = 5;
    let mut iteration = 0;

    let mut context = build_context_messages(&conversation);
    let initial_system_prompt =
        build_system_prompt(&conversation, assistant.as_ref(), &[], &available_tools);
    let mut system_prompt = initial_system_prompt.clone();

    let run_result = async {
        loop {
            iteration += 1;
            if iteration > max_tool_iterations {
                break;
            }

            let (content, reasoning, tool_calls_requested) =
                if provider.capabilities.supports_streaming {
                    run_model_request_with_tools_streaming(
                        &app,
                        &conversation_id,
                        &assistant_message_id,
                        &provider,
                        &profile,
                        &context,
                        &system_prompt,
                        &available_tools,
                    )
                    .await?
                } else {
                    run_model_request_with_tools(
                        &provider,
                        &profile,
                        &context,
                        &system_prompt,
                        &available_tools,
                    )
                    .await?
                };

            accumulated_content.push_str(&content);
            if let Some(r) = reasoning {
                accumulated_reasoning = Some(
                    accumulated_reasoning
                        .map(|existing| format!("{}\n{}", existing, r))
                        .unwrap_or(r),
                );
            }

            if tool_calls_requested.is_empty() {
                break;
            }

            for tool_call in &tool_calls_requested {
                let arguments = parse_tool_call_arguments(tool_call.arguments.as_deref());

                let result = execute_tool_call(
                    &app,
                    &state,
                    &tool_call.name,
                    &arguments,
                    &conversation_id,
                    &assistant_message_id,
                    &available_tools_by_name,
                    &mcp_tools_by_name,
                    &mut mcp_clients,
                )
                .await;
                let bound_tool = mcp_tools_by_name.get(&tool_call.name);

                let tool_result_content = match result {
                    Ok((text, sources)) => {
                        all_sources.extend(sources);
                        let success_tool = build_tool_call_snapshot(
                            tool_call.id.clone(),
                            tool_call.name.clone(),
                            tool_call.arguments.clone(),
                            "success",
                            Some("Tool executed successfully".to_string()),
                            Some(text.clone()),
                            tool_call.started_at,
                            Some(now_ts()),
                            bound_tool,
                        );
                        all_tool_calls.push(success_tool);
                        text
                    }
                    Err(error) => {
                        let failed_tool = build_tool_call_snapshot(
                            tool_call.id.clone(),
                            tool_call.name.clone(),
                            tool_call.arguments.clone(),
                            "failed",
                            Some(error.clone()),
                            Some(format!("Error: {}", error)),
                            tool_call.started_at,
                            Some(now_ts()),
                            bound_tool,
                        );
                        all_tool_calls.push(failed_tool);
                        format!("Error: {}", error)
                    }
                };

                context.push(("assistant".to_string(), content.clone()));
                context.push((
                    "tool".to_string(),
                    json!({
                        "tool_call_id": tool_call.id,
                        "content": tool_result_content,
                    })
                    .to_string(),
                ));
            }

            system_prompt = build_system_prompt(
                &conversation,
                assistant.as_ref(),
                &all_sources,
                &available_tools,
            );
        }

        save_message_result(
            &conversation_id,
            &assistant_message_id,
            &accumulated_content,
            accumulated_reasoning.clone(),
            all_sources.clone(),
            all_tool_calls.clone(),
            "done",
        )?;

        emit_stream_event(
            &app,
            AssistantStreamEvent {
                conversation_id,
                message_id: assistant_message_id,
                kind: "message.completed".to_string(),
                text: None,
                sources: Some(all_sources),
                tool: None,
                error: None,
            },
        );

        Ok(())
    }
    .await;

    close_mcp_clients(&mut mcp_clients).await;
    run_result
}

pub(in crate::ai_assistant) fn new_message(
    role: &str,
    content: String,
    status: &str,
) -> AssistantMessage {
    AssistantMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: role.to_string(),
        content,
        reasoning: None,
        sources: Vec::new(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        schedule_draft: None,
        created_at: now_ts(),
        status: status.to_string(),
    }
}

pub(in crate::ai_assistant) fn upsert_agent(
    mut incoming: AgentDefinition,
) -> Result<AgentDefinition, String> {
    let mut state = load_state()?;
    let now = now_ts();
    if incoming.id.trim().is_empty() {
        incoming.id = uuid::Uuid::new_v4().to_string();
        incoming.created_at = now;
    }
    incoming.updated_at = now;
    if incoming.created_at == 0 {
        incoming.created_at = now;
    }
    if let Some(existing) = state
        .agents
        .iter_mut()
        .find(|agent| agent.id == incoming.id)
    {
        *existing = incoming.clone();
    } else {
        state.agents.push(incoming.clone());
    }
    save_state(&state)?;
    Ok(incoming)
}

pub(in crate::ai_assistant) fn compute_next_run_at(
    trigger: &ScheduleTrigger,
    from_ts: u64,
    timezone: Option<&str>,
) -> Option<u64> {
    let tz: chrono_tz::Tz = timezone
        .and_then(|tz| tz.parse().ok())
        .unwrap_or(chrono_tz::Tz::Asia__Shanghai);

    match trigger.kind.as_str() {
        "interval" => trigger
            .interval_minutes
            .map(|minutes| from_ts + minutes.saturating_mul(60)),
        "daily" => {
            let time = trigger.time_of_day.as_deref().unwrap_or("09:00");
            let (hour, minute) = parse_time_of_day(time)?;
            let now_utc = chrono::Utc::now();
            let now_in_tz = now_utc.with_timezone(&tz);
            let today = now_in_tz
                .with_hour(hour)?
                .with_minute(minute)?
                .with_second(0)?
                .with_nanosecond(0)?;
            let next = if today.timestamp() as u64 > from_ts {
                today
            } else {
                today + chrono::Duration::days(1)
            };
            Some(next.timestamp() as u64)
        }
        "weekly" => {
            let days = if trigger.weekdays.is_empty() {
                vec![1]
            } else {
                trigger.weekdays.clone()
            };
            let time = trigger.time_of_day.as_deref().unwrap_or("09:00");
            let (hour, minute) = parse_time_of_day(time)?;
            let now_utc = chrono::Utc::now();
            let base = now_utc.with_timezone(&tz);
            for offset in 0..8 {
                let candidate = base + chrono::Duration::days(offset);
                let weekday = weekday_to_u8(candidate.weekday());
                if !days.contains(&weekday) {
                    continue;
                }
                let scheduled = candidate
                    .with_hour(hour)?
                    .with_minute(minute)?
                    .with_second(0)?
                    .with_nanosecond(0)?;
                if scheduled.timestamp() as u64 > from_ts {
                    return Some(scheduled.timestamp() as u64);
                }
            }
            None
        }
        _ => None,
    }
}

pub(in crate::ai_assistant) fn weekday_to_u8(weekday: Weekday) -> u8 {
    match weekday {
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
        Weekday::Sun => 7,
    }
}

pub(in crate::ai_assistant) fn parse_time_of_day(value: &str) -> Option<(u32, u32)> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let hour = parts[0].trim().parse::<u32>().ok()?;
    let minute = parts[1].trim().parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

pub(in crate::ai_assistant) fn schedule_view(
    job: &ScheduleJob,
    runs: &[ScheduleRun],
) -> ScheduleJobView {
    let recent_runs = runs
        .iter()
        .filter(|run| run.schedule_id == job.id)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    ScheduleJobView {
        job: job.clone(),
        recent_runs,
    }
}

pub(in crate::ai_assistant) async fn trigger_schedule_run(
    app: tauri::AppHandle,
    schedule_id: String,
) -> Result<(), String> {
    {
        let mut running = running_schedules()
            .lock()
            .map_err(|_| "running schedule lock poisoned".to_string())?;
        if !running.insert(schedule_id.clone()) {
            return Ok(());
        }
    }

    let result = trigger_schedule_run_inner(app.clone(), schedule_id.clone()).await;

    if let Ok(mut running) = running_schedules().lock() {
        running.remove(&schedule_id);
    }
    result
}

pub(in crate::ai_assistant) async fn trigger_schedule_run_inner(
    app: tauri::AppHandle,
    schedule_id: String,
) -> Result<(), String> {
    let mut state = load_state()?;
    let schedule_index = state
        .schedules
        .iter()
        .position(|schedule| schedule.id == schedule_id)
        .ok_or_else(|| "Schedule not found".to_string())?;
    let schedule_snapshot = state.schedules[schedule_index].clone();
    let agent = state
        .agents
        .iter()
        .find(|agent| agent.id == schedule_snapshot.agent_id)
        .cloned()
        .ok_or_else(|| "Schedule agent not found".to_string())?;

    let conversation_id = if let Some(existing_id) = schedule_snapshot.conversation_id.clone() {
        existing_id
    } else {
        let conversation = AssistantConversation {
            id: uuid::Uuid::new_v4().to_string(),
            title: schedule_snapshot.name.clone(),
            pinned: false,
            archived: false,
            created_at: now_ts(),
            updated_at: now_ts(),
            assistant_id: schedule_assistant_id(&schedule_snapshot).map(|value| value.to_string()),
            model_profile_id: schedule_snapshot.model_profile_id.clone(),
            model_override_id: schedule_snapshot.model_override_id.clone().or_else(|| {
                legacy_profile_catalog_id(
                    &state.settings,
                    schedule_snapshot.model_profile_id.as_deref(),
                )
            }),
            web_search_enabled: schedule_snapshot.web_search_enabled,
            capability_snapshot: Some(capability_snapshot_from_agent(
                Some(&agent),
                schedule_snapshot.web_search_enabled,
            )),
            context_reset_count: 0,
            messages: Vec::new(),
        };
        let id = conversation.id.clone();
        state.schedules[schedule_index].conversation_id = Some(id.clone());
        state.conversations.push(conversation);
        id
    };
    let prompt = if schedule_snapshot.prompt.trim().is_empty() {
        format!("Run scheduled task for {}", schedule_snapshot.name)
    } else {
        schedule_snapshot.prompt.clone()
    };
    let user_message = new_message("user", prompt.clone(), "done");
    let assistant_message = new_message("assistant", String::new(), "streaming");

    if let Some(conversation) = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
    {
        conversation.messages.push(user_message.clone());
        conversation.messages.push(assistant_message.clone());
        conversation.updated_at = now_ts();
    }

    let run = ScheduleRun {
        id: uuid::Uuid::new_v4().to_string(),
        schedule_id: schedule_snapshot.id.clone(),
        started_at: now_ts(),
        ended_at: None,
        status: "running".to_string(),
        summary: None,
        error_message: None,
        conversation_id: Some(conversation_id.clone()),
    };
    state.runs.insert(0, run.clone());
    state.schedules[schedule_index].last_run_at = Some(now_ts());
    state.schedules[schedule_index].last_status = Some("running".to_string());
    state.schedules[schedule_index].last_error = None;
    state.schedules[schedule_index].next_run_at = compute_next_run_at(
        &schedule_snapshot.trigger,
        now_ts(),
        schedule_snapshot.timezone.as_deref(),
    );
    state.schedules[schedule_index].updated_at = now_ts();
    save_state(&state)?;

    let execution = execute_workspace_conversation_run(
        app.clone(),
        conversation_id.clone(),
        assistant_message.id.clone(),
        schedule_snapshot.model_override_id.clone().or_else(|| {
            legacy_profile_catalog_id(
                &state.settings,
                schedule_snapshot.model_profile_id.as_deref(),
            )
        }),
        schedule_assistant_id(&schedule_snapshot)
            .map(|value| value.to_string())
            .or_else(|| Some(agent.id.clone())),
        Some(schedule_snapshot.web_search_enabled),
    )
    .await;

    let mut latest_state = load_state()?;
    if let Some(latest_run) = latest_state.runs.iter_mut().find(|item| item.id == run.id) {
        latest_run.ended_at = Some(now_ts());
        match &execution {
            Ok(()) => {
                latest_run.status = "success".to_string();
                latest_run.summary = Some("Schedule run completed".to_string());
            }
            Err(error) => {
                latest_run.status = "failed".to_string();
                latest_run.error_message = Some(error.clone());
            }
        }
    }
    if let Some(latest_schedule) = latest_state
        .schedules
        .iter_mut()
        .find(|item| item.id == schedule_id)
    {
        match &execution {
            Ok(()) => {
                latest_schedule.last_status = Some("success".to_string());
                latest_schedule.last_error = None;
                latest_schedule.retry_count = 0;
            }
            Err(error) => {
                let should_retry = latest_schedule.retry_count < latest_schedule.max_retries;
                if should_retry {
                    latest_schedule.retry_count += 1;
                    latest_schedule.last_status = Some("retrying".to_string());
                    latest_schedule.last_error = Some(format!(
                        "{} (retry {}/{})",
                        error, latest_schedule.retry_count, latest_schedule.max_retries
                    ));
                    // Schedule retry in 5 minutes
                    latest_schedule.next_run_at = Some(now_ts() + 300);
                } else {
                    latest_schedule.last_status = Some("failed".to_string());
                    latest_schedule.last_error = Some(error.clone());
                    latest_schedule.retry_count = 0;
                }
            }
        }
        latest_schedule.updated_at = now_ts();
    }
    save_state(&latest_state)?;
    execution
}
