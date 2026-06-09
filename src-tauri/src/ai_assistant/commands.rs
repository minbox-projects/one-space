#[tauri::command]
pub fn ai_workspace_bootstrap() -> Result<AiWorkspaceBootstrap, String> {
    let state = load_state()?;
    let mut assistants = state.agents.clone();
    assistants.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let mut conversations = state
        .conversations
        .iter()
        .map(conversation_list_item)
        .collect::<Vec<_>>();
    conversations.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    let mut automations = state
        .schedules
        .iter()
        .map(|job| schedule_view(job, &state.runs))
        .collect::<Vec<_>>();
    automations.sort_by(|a, b| {
        b.job.enabled.cmp(&a.job.enabled).then_with(|| {
            a.job
                .next_run_at
                .unwrap_or(u64::MAX)
                .cmp(&b.job.next_run_at.unwrap_or(u64::MAX))
        })
    });
    Ok(AiWorkspaceBootstrap {
        settings: state.settings,
        assistants,
        conversations,
        automations,
        quick_assistant: state.quick_assistant,
        selection_assistant: state.selection_assistant,
    })
}

#[tauri::command]
pub fn workspace_settings_get() -> Result<AiAssistantSettings, String> {
    Ok(load_state()?.settings)
}

#[tauri::command]
pub fn workspace_settings_save(
    settings: AiAssistantSettings,
) -> Result<AiAssistantSettings, String> {
    let mut state = load_state()?;
    state.settings = settings.clone();
    state.revision = now_ts();
    save_state(&state)?;
    Ok(load_state()?.settings)
}

#[tauri::command]
pub fn workspace_model_roles_get() -> Result<Vec<ModelRoleBinding>, String> {
    Ok(load_state()?.settings.role_bindings)
}

#[tauri::command]
pub fn workspace_model_roles_save(
    role_bindings: Vec<ModelRoleBinding>,
) -> Result<Vec<ModelRoleBinding>, String> {
    let mut state = load_state()?;
    state.settings.role_bindings = role_bindings;
    state.revision = now_ts();
    save_state(&state)?;
    Ok(load_state()?.settings.role_bindings)
}

#[tauri::command]
pub async fn provider_connection_test(
    input: ProviderConnectionTestInput,
) -> Result<AssistantConnectionTestResult, String> {
    let state = load_state()?;
    let provider = state
        .settings
        .providers
        .iter()
        .find(|provider| provider.id == input.provider_id)
        .ok_or_else(|| "Provider not found".to_string())?
        .clone();
    if !provider.enabled {
        return Err(format!("Provider is disabled: {}", provider.name));
    }
    let existing_catalog_count = state
        .settings
        .model_catalog
        .iter()
        .filter(|item| item.provider_id == provider.id)
        .count();
    let started = std::time::Instant::now();
    match fetch_provider_model_catalog_detailed(&provider).await {
        Ok(catalog) => Ok(AssistantConnectionTestResult {
            ok: true,
            message: format!(
                "{} connected successfully. {} model(s) discovered.",
                provider.name,
                catalog.len()
            ),
            latency_ms: started.elapsed().as_millis() as u64,
        }),
        Err(error) if error.unsupported_catalog_endpoint => Ok(AssistantConnectionTestResult {
            ok: true,
            message: if existing_catalog_count > 0 {
                format!(
                    "{} connected successfully. This provider does not expose a standard model catalog endpoint, so detection verified connectivity and kept {} existing local catalog item(s).",
                    provider.name,
                    existing_catalog_count
                )
            } else {
                format!(
                    "{} connected successfully. This provider does not expose a standard model catalog endpoint, so detection verified connectivity only.",
                    provider.name
                )
            },
            latency_ms: started.elapsed().as_millis() as u64,
        }),
        Err(error) => Err(error.message),
    }
}

#[tauri::command]
pub async fn provider_models_fetch(
    input: ProviderModelsFetchInput,
) -> Result<Vec<ModelCatalogItem>, String> {
    let mut state = load_state()?;
    let provider = state
        .settings
        .providers
        .iter()
        .find(|provider| provider.id == input.provider_id)
        .ok_or_else(|| "Provider not found".to_string())?
        .clone();
    let catalog = fetch_provider_model_catalog(&provider).await?;
    state
        .settings
        .model_catalog
        .retain(|item| item.provider_id != provider.id);
    state.settings.model_catalog.extend(catalog.clone());
    if state.settings.role_bindings.is_empty() {
        state.settings.role_bindings = build_default_role_bindings(&state.settings);
    }
    state.revision = now_ts();
    save_state(&state)?;
    Ok(catalog)
}

#[tauri::command]
pub fn workspace_assistants_list() -> Result<Vec<AgentDefinition>, String> {
    let mut assistants = load_state()?.agents;
    assistants.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(assistants)
}

#[tauri::command]
pub fn workspace_assistant_upsert(
    mut assistant: AgentDefinition,
) -> Result<AgentDefinition, String> {
    let state = load_state()?;
    let is_new = assistant.id.trim().is_empty();
    if assistant.primary_model_id.is_none() {
        assistant.primary_model_id = default_role_model_id(&state.settings, "assistant");
    }
    if assistant.light_model_id.is_none() {
        assistant.light_model_id = default_role_model_id(&state.settings, "summary");
    }
    drop(state);
    if is_new && assistant.mcp_server_ids.is_empty() {
        assistant.mcp_server_ids = crate::assistant_mcp::ensure_default_assistant_mcp_server_ids()?;
    }
    upsert_agent(assistant)
}

#[tauri::command]
pub fn workspace_assistant_delete(assistant_id: String) -> Result<bool, String> {
    let mut state = load_state()?;
    let before = state.agents.len();
    state
        .agents
        .retain(|assistant| assistant.id != assistant_id);
    for conversation in &mut state.conversations {
        if conversation.assistant_id.as_deref() == Some(assistant_id.as_str()) {
            conversation.assistant_id = None;
        }
    }
    for schedule in &mut state.schedules {
        if schedule_assistant_id(schedule) == Some(assistant_id.as_str()) {
            schedule.assistant_id = None;
            schedule.agent_id.clear();
        }
    }
    if state.quick_assistant.preferred_assistant_id.as_deref() == Some(assistant_id.as_str()) {
        state.quick_assistant.preferred_assistant_id = None;
    }
    save_state(&state)?;
    Ok(before != state.agents.len())
}

#[tauri::command]
pub async fn workspace_assistant_test_run(
    app: tauri::AppHandle,
    input: AgentTestRunInput,
) -> Result<AgentTestRunResult, String> {
    let assistant = load_state()?
        .agents
        .into_iter()
        .find(|assistant| assistant.id == input.agent_id)
        .ok_or_else(|| "Assistant not found".to_string())?;
    let conversation = workspace_conversation_create(Some(WorkspaceConversationCreateInput {
        title: Some(format!("{} Topic", assistant.name)),
        assistant_id: Some(assistant.id.clone()),
        model_override_id: assistant.primary_model_id.clone(),
    }))?;
    let _ = workspace_conversation_send(
        app,
        WorkspaceConversationSendInput {
            conversation_id: conversation.id.clone(),
            content: input.prompt,
            assistant_id: Some(assistant.id),
            model_override_id: None,
            web_search_enabled: Some(assistant.tool_policy.web_search),
        },
    )
    .await?;
    Ok(AgentTestRunResult {
        conversation_id: conversation.id,
    })
}

#[tauri::command]
pub fn workspace_conversations_list() -> Result<Vec<AssistantConversationListItem>, String> {
    let mut items = load_state()?
        .conversations
        .iter()
        .map(conversation_list_item)
        .collect::<Vec<_>>();
    items.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    Ok(items)
}

#[tauri::command]
pub fn workspace_conversation_get(
    conversation_id: String,
) -> Result<AssistantConversation, String> {
    load_state()?
        .conversations
        .into_iter()
        .find(|conversation| conversation.id == conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())
}

#[tauri::command]
pub fn workspace_conversation_create(
    input: Option<WorkspaceConversationCreateInput>,
) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let now = now_ts();
    let requested_assistant_id = input
        .as_ref()
        .and_then(|payload| payload.assistant_id.clone())
        .or_else(|| state.quick_assistant.preferred_assistant_id.clone());
    let assistant = requested_assistant_id
        .as_deref()
        .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
        .cloned();
    let model_override_id = input
        .as_ref()
        .and_then(|payload| payload.model_override_id.clone())
        .or_else(|| {
            assistant
                .as_ref()
                .and_then(|item| item.primary_model_id.clone())
        })
        .or_else(|| {
            default_role_model_id(
                &state.settings,
                if assistant.is_some() {
                    "assistant"
                } else {
                    "chat"
                },
            )
        });
    let title = input
        .as_ref()
        .and_then(|payload| payload.title.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "新主题".to_string());

    let conversation = AssistantConversation {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        pinned: false,
        archived: false,
        created_at: now,
        updated_at: now,
        assistant_id: assistant.as_ref().map(|item| item.id.clone()),
        model_profile_id: None,
        model_override_id,
        web_search_enabled: assistant
            .as_ref()
            .map(|item| item.tool_policy.web_search)
            .unwrap_or(false),
        capability_snapshot: Some(capability_snapshot_from_agent(
            assistant.as_ref(),
            assistant
                .as_ref()
                .map(|item| item.tool_policy.web_search)
                .unwrap_or(false),
        )),
        context_reset_count: 0,
        messages: Vec::new(),
    };
    state.conversations.insert(0, conversation.clone());
    save_state(&state)?;
    Ok(conversation)
}

#[tauri::command]
pub fn workspace_conversation_update(
    input: WorkspaceConversationUpdateInput,
) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let assistant_override = input
        .assistant_id
        .as_deref()
        .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
        .cloned();
    let conversation = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == input.conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    if let Some(title) = input.title {
        conversation.title = title.trim().to_string();
    }
    if let Some(pinned) = input.pinned {
        conversation.pinned = pinned;
    }
    if let Some(archived) = input.archived {
        conversation.archived = archived;
    }
    if let Some(assistant_id) = input.assistant_id {
        conversation.assistant_id = if assistant_id.trim().is_empty() {
            None
        } else {
            Some(assistant_id.trim().to_string())
        };
    }
    if let Some(model_override_id) = input.model_override_id {
        conversation.model_override_id = if model_override_id.trim().is_empty() {
            None
        } else {
            Some(model_override_id.trim().to_string())
        };
    }
    if let Some(web_search_enabled) = input.web_search_enabled {
        conversation.web_search_enabled = web_search_enabled;
    }
    conversation.capability_snapshot = Some(capability_snapshot_from_agent(
        assistant_override.as_ref().or_else(|| {
            conversation
                .assistant_id
                .as_deref()
                .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
        }),
        conversation.web_search_enabled,
    ));
    conversation.updated_at = now_ts();
    let updated = conversation.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub fn workspace_conversation_delete(conversation_id: String) -> Result<bool, String> {
    let mut state = load_state()?;
    let before = state.conversations.len();
    state
        .conversations
        .retain(|conversation| conversation.id != conversation_id);
    save_state(&state)?;
    Ok(before != state.conversations.len())
}

#[tauri::command]
pub fn workspace_conversation_reset_context(
    conversation_id: String,
) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let conversation = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    conversation.messages.push(AssistantMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "context_reset".to_string(),
        content: "上下文已重置".to_string(),
        reasoning: None,
        sources: Vec::new(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        schedule_draft: None,
        created_at: now_ts(),
        status: "done".to_string(),
    });
    conversation.context_reset_count = conversation.context_reset_count.saturating_add(1);
    conversation.updated_at = now_ts();
    let updated = conversation.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub async fn workspace_schedule_resolve_draft(
    app: tauri::AppHandle,
    input: ScheduleDraftResolveInput,
) -> Result<AssistantConversation, String> {
    assistant_schedule_resolve_draft(app, input).await
}

#[tauri::command]
pub async fn workspace_conversation_send(
    app: tauri::AppHandle,
    input: WorkspaceConversationSendInput,
) -> Result<AssistantSendResult, String> {
    let mut state = load_state()?;
    let conversation_index = state
        .conversations
        .iter()
        .position(|conversation| conversation.id == input.conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    let assistant = input
        .assistant_id
        .as_deref()
        .or(state.conversations[conversation_index]
            .assistant_id
            .as_deref())
        .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
        .cloned();
    let mut schedule_draft = build_schedule_draft(&state, input.content.trim());
    if let Some(draft) = schedule_draft.as_mut() {
        if let Some(schedule) = draft.schedule.as_mut() {
            if schedule
                .assistant_id
                .as_deref()
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                if let Some(assistant) = assistant.as_ref() {
                    schedule.assistant_id = Some(assistant.id.clone());
                    schedule.agent_id = assistant.id.clone();
                    if schedule.model_override_id.is_none() {
                        schedule.model_override_id = assistant.primary_model_id.clone();
                    }
                }
            }
            if schedule.model_override_id.is_none() {
                schedule.model_override_id = input
                    .model_override_id
                    .clone()
                    .or_else(|| {
                        state.conversations[conversation_index]
                            .model_override_id
                            .clone()
                    })
                    .or_else(|| default_role_model_id(&state.settings, "automation"));
            }
        }
        if draft.agent_name.is_none() {
            draft.agent_name = assistant.as_ref().map(|item| item.name.clone());
        }
    }

    let user_message = new_message("user", input.content.trim().to_string(), "done");
    let mut assistant_message = new_message("assistant", String::new(), "streaming");
    if let Some(draft) = schedule_draft.clone() {
        assistant_message.content = draft.summary.clone();
        assistant_message.status = "done".to_string();
        assistant_message.schedule_draft = Some(draft);
    }
    let conversation = &mut state.conversations[conversation_index];
    if let Some(assistant_id) = input.assistant_id.clone() {
        conversation.assistant_id = if assistant_id.trim().is_empty() {
            None
        } else {
            Some(assistant_id)
        };
    }
    if let Some(model_override_id) = input.model_override_id.clone() {
        conversation.model_override_id = if model_override_id.trim().is_empty() {
            None
        } else {
            Some(model_override_id)
        };
    }
    if let Some(web_search_enabled) = input.web_search_enabled {
        conversation.web_search_enabled = web_search_enabled;
    }
    conversation.capability_snapshot = Some(capability_snapshot_from_agent(
        assistant.as_ref(),
        conversation.web_search_enabled,
    ));
    conversation.messages.push(user_message.clone());
    conversation.messages.push(assistant_message.clone());
    conversation.updated_at = now_ts();
    if conversation.title.trim().is_empty() || conversation.title == "新主题" {
        conversation.title = derive_title(&user_message.content);
    }
    save_state(&state)?;

    if schedule_draft.is_some() {
        return Ok(AssistantSendResult {
            conversation_id: input.conversation_id,
            user_message_id: user_message.id,
            assistant_message_id: assistant_message.id,
        });
    }

    let app_handle = app.clone();
    let conversation_id = input.conversation_id.clone();
    let assistant_message_id = assistant_message.id.clone();
    let model_override_id = input.model_override_id.clone();
    let assistant_id = input.assistant_id.clone();
    let web_search_enabled = input.web_search_enabled;
    tauri::async_runtime::spawn(async move {
        if let Err(error) = execute_workspace_conversation_run(
            app_handle.clone(),
            conversation_id.clone(),
            assistant_message_id.clone(),
            model_override_id,
            assistant_id,
            web_search_enabled,
        )
        .await
        {
            let _ = save_message_result(
                &conversation_id,
                &assistant_message_id,
                "",
                None,
                Vec::new(),
                Vec::new(),
                "failed",
            );
            emit_stream_event(
                &app_handle,
                AssistantStreamEvent {
                    conversation_id,
                    message_id: assistant_message_id,
                    kind: "message.failed".to_string(),
                    text: None,
                    sources: None,
                    tool: None,
                    error: Some(error),
                },
            );
        }
    });

    Ok(AssistantSendResult {
        conversation_id: input.conversation_id,
        user_message_id: user_message.id,
        assistant_message_id: assistant_message.id,
    })
}

#[tauri::command]
pub fn workspace_automations_list() -> Result<Vec<ScheduleJobView>, String> {
    assistant_schedules_list()
}

#[tauri::command]
pub fn workspace_automation_upsert(mut schedule: ScheduleJob) -> Result<ScheduleJob, String> {
    if let Some(assistant_id) = schedule
        .assistant_id
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        schedule.agent_id = assistant_id;
    } else if !schedule.agent_id.trim().is_empty() {
        schedule.assistant_id = Some(schedule.agent_id.clone());
    }
    assistant_schedule_upsert(schedule)
}

#[tauri::command]
pub fn workspace_automation_delete(schedule_id: String) -> Result<bool, String> {
    assistant_schedule_delete(schedule_id)
}

#[tauri::command]
pub fn workspace_automation_toggle(input: ScheduleToggleInput) -> Result<ScheduleJob, String> {
    assistant_schedule_toggle(input)
}

#[tauri::command]
pub async fn workspace_automation_run_now(
    app: tauri::AppHandle,
    input: ScheduleRunNowInput,
) -> Result<bool, String> {
    assistant_schedule_run_now(app, input).await
}

#[tauri::command]
pub fn workspace_quick_assistant_get() -> Result<QuickAssistantPreferences, String> {
    Ok(load_state()?.quick_assistant)
}

#[tauri::command]
pub fn workspace_quick_assistant_save(
    preferences: QuickAssistantPreferences,
) -> Result<QuickAssistantPreferences, String> {
    let mut state = load_state()?;
    state.quick_assistant = preferences.clone();
    state.revision = now_ts();
    save_state(&state)?;
    Ok(preferences)
}

#[tauri::command]
pub fn workspace_selection_assistant_get() -> Result<SelectionAssistantPreferences, String> {
    Ok(load_state()?.selection_assistant)
}

#[tauri::command]
pub fn workspace_selection_assistant_save(
    preferences: SelectionAssistantPreferences,
) -> Result<SelectionAssistantPreferences, String> {
    let mut state = load_state()?;
    state.selection_assistant = preferences.clone();
    state.revision = now_ts();
    save_state(&state)?;
    Ok(preferences)
}

#[tauri::command]
pub async fn assistant_schedule_resolve_draft(
    app: tauri::AppHandle,
    input: ScheduleDraftResolveInput,
) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let conversation_index = state
        .conversations
        .iter()
        .position(|conversation| conversation.id == input.conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    let message_index = state.conversations[conversation_index]
        .messages
        .iter()
        .position(|message| message.id == input.message_id)
        .ok_or_else(|| "Draft message not found".to_string())?;
    let draft = state.conversations[conversation_index].messages[message_index]
        .schedule_draft
        .clone()
        .ok_or_else(|| "No schedule draft found on this message".to_string())?;

    let now = now_ts();
    if !input.approved {
        let message = &mut state.conversations[conversation_index].messages[message_index];
        message.content = "已取消本次定时任务变更。".to_string();
        message.schedule_draft = None;
        message.tool_calls = vec![build_tool_call_snapshot(
            uuid::Uuid::new_v4().to_string(),
            "schedule.cancel".to_string(),
            None,
            "cancelled",
            Some("Schedule draft was cancelled".to_string()),
            None,
            now,
            Some(now),
            None,
        )];
        state.conversations[conversation_index].updated_at = now;
        let updated = state.conversations[conversation_index].clone();
        save_state(&state)?;
        return Ok(updated);
    }

    let mut tool_name = "schedule.update".to_string();
    let result_message = match draft.action.as_str() {
        "create" | "update" => {
            let mut schedule = draft
                .schedule
                .clone()
                .ok_or_else(|| "Draft schedule payload is missing".to_string())?;
            if schedule.id.trim().is_empty() {
                schedule.id = uuid::Uuid::new_v4().to_string();
                schedule.created_at = now;
                tool_name = "schedule.create".to_string();
            } else if schedule.created_at == 0 {
                schedule.created_at = now;
            }
            if schedule.output_target.trim().is_empty() {
                schedule.output_target = "assistant_conversation".to_string();
            }
            schedule.updated_at = now;
            schedule.next_run_at = if schedule.enabled {
                compute_next_run_at(&schedule.trigger, now, schedule.timezone.as_deref())
            } else {
                None
            };
            if let Some(existing) = state
                .schedules
                .iter_mut()
                .find(|item| item.id == schedule.id)
            {
                *existing = schedule.clone();
                format!(
                    "已更新定时任务“{}”，计划：{}。",
                    schedule.name,
                    format_trigger_label(&schedule.trigger)
                )
            } else {
                state.schedules.push(schedule.clone());
                format!(
                    "已创建定时任务“{}”，计划：{}。",
                    schedule.name,
                    format_trigger_label(&schedule.trigger)
                )
            }
        }
        "toggle_off" | "toggle_on" => {
            let schedule_id = draft
                .target_schedule_id
                .clone()
                .ok_or_else(|| "Target schedule is missing".to_string())?;
            let enabled = draft.desired_enabled.unwrap_or(draft.action == "toggle_on");
            let schedule = state
                .schedules
                .iter_mut()
                .find(|item| item.id == schedule_id)
                .ok_or_else(|| "Target schedule not found".to_string())?;
            schedule.enabled = enabled;
            schedule.updated_at = now;
            schedule.next_run_at = if enabled {
                compute_next_run_at(&schedule.trigger, now, schedule.timezone.as_deref())
            } else {
                None
            };
            tool_name = if enabled {
                "schedule.enable".to_string()
            } else {
                "schedule.pause".to_string()
            };
            if enabled {
                format!("已启用定时任务“{}”。", schedule.name)
            } else {
                format!("已暂停定时任务“{}”。", schedule.name)
            }
        }
        "delete" => {
            let schedule_id = draft
                .target_schedule_id
                .clone()
                .ok_or_else(|| "Target schedule is missing".to_string())?;
            let schedule_name = draft
                .target_schedule_name
                .clone()
                .unwrap_or_else(|| "未命名任务".to_string());
            state.schedules.retain(|item| item.id != schedule_id);
            state.runs.retain(|run| run.schedule_id != schedule_id);
            tool_name = "schedule.delete".to_string();
            format!("已删除定时任务“{}”。", schedule_name)
        }
        "run_now" => {
            let schedule_id = draft
                .target_schedule_id
                .clone()
                .ok_or_else(|| "Target schedule is missing".to_string())?;
            let schedule_name = draft
                .target_schedule_name
                .clone()
                .unwrap_or_else(|| "未命名任务".to_string());
            tool_name = "schedule.run_now".to_string();
            let result_message = format!("已提交立即执行请求：定时任务“{}”。", schedule_name);
            save_state(&state)?;
            trigger_schedule_run(app, schedule_id).await?;
            let mut refreshed = load_state()?;
            let conversation = refreshed
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == input.conversation_id)
                .ok_or_else(|| "Conversation not found after run".to_string())?;
            let message = conversation
                .messages
                .iter_mut()
                .find(|message| message.id == input.message_id)
                .ok_or_else(|| "Draft message not found after run".to_string())?;
            message.content = result_message.clone();
            message.schedule_draft = None;
            message.tool_calls = vec![build_tool_call_snapshot(
                uuid::Uuid::new_v4().to_string(),
                tool_name,
                None,
                "success",
                Some(result_message.clone()),
                None,
                now,
                Some(now_ts()),
                None,
            )];
            conversation.updated_at = now_ts();
            let updated = conversation.clone();
            save_state(&refreshed)?;
            return Ok(updated);
        }
        _ => return Err("Unsupported draft action".to_string()),
    };

    let finished_at = now_ts();
    let conversation = &mut state.conversations[conversation_index];
    let message = &mut conversation.messages[message_index];
    message.content = result_message.clone();
    message.schedule_draft = None;
    message.tool_calls = vec![build_tool_call_snapshot(
        uuid::Uuid::new_v4().to_string(),
        tool_name,
        None,
        "success",
        Some(result_message.clone()),
        None,
        now,
        Some(finished_at),
        None,
    )];
    conversation.updated_at = finished_at;
    let updated = conversation.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub fn assistant_schedules_list() -> Result<Vec<ScheduleJobView>, String> {
    let mut state = load_state()?;
    state.runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    let mut schedules = state
        .schedules
        .iter()
        .map(|job| schedule_view(job, &state.runs))
        .collect::<Vec<_>>();
    schedules.sort_by(|a, b| {
        b.job.enabled.cmp(&a.job.enabled).then_with(|| {
            a.job
                .next_run_at
                .unwrap_or(u64::MAX)
                .cmp(&b.job.next_run_at.unwrap_or(u64::MAX))
        })
    });
    Ok(schedules)
}

#[tauri::command]
pub fn assistant_schedule_upsert(mut schedule: ScheduleJob) -> Result<ScheduleJob, String> {
    let mut state = load_state()?;
    let now = now_ts();
    if schedule.id.trim().is_empty() {
        schedule.id = uuid::Uuid::new_v4().to_string();
        schedule.created_at = now;
    }
    if schedule.created_at == 0 {
        schedule.created_at = now;
    }
    schedule.updated_at = now;
    if schedule.output_target.trim().is_empty() {
        schedule.output_target = "assistant_conversation".to_string();
    }
    schedule.next_run_at = if schedule.enabled {
        compute_next_run_at(&schedule.trigger, now, schedule.timezone.as_deref())
    } else {
        None
    };

    if let Some(existing) = state
        .schedules
        .iter_mut()
        .find(|item| item.id == schedule.id)
    {
        *existing = schedule.clone();
    } else {
        state.schedules.push(schedule.clone());
    }
    save_state(&state)?;
    Ok(schedule)
}

#[tauri::command]
pub fn assistant_schedule_delete(schedule_id: String) -> Result<bool, String> {
    let mut state = load_state()?;
    let before = state.schedules.len();
    state
        .schedules
        .retain(|schedule| schedule.id != schedule_id);
    state.runs.retain(|run| run.schedule_id != schedule_id);
    save_state(&state)?;
    Ok(before != state.schedules.len())
}

#[tauri::command]
pub fn assistant_schedule_toggle(input: ScheduleToggleInput) -> Result<ScheduleJob, String> {
    let mut state = load_state()?;
    let schedule = state
        .schedules
        .iter_mut()
        .find(|schedule| schedule.id == input.schedule_id)
        .ok_or_else(|| "Schedule not found".to_string())?;
    schedule.enabled = input.enabled;
    schedule.updated_at = now_ts();
    schedule.next_run_at = if input.enabled {
        compute_next_run_at(&schedule.trigger, now_ts(), schedule.timezone.as_deref())
    } else {
        None
    };
    let updated = schedule.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub async fn assistant_schedule_run_now(
    app: tauri::AppHandle,
    input: ScheduleRunNowInput,
) -> Result<bool, String> {
    trigger_schedule_run(app, input.schedule_id).await?;
    Ok(true)
}
