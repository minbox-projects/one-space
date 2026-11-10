use std::path::PathBuf;
use std::sync::Arc;
use log::{info, warn};
use tokio::process::Command;

use super::auth::AuthGuard;
use super::card_builder::CardBuilder;
use super::client::TelegramClient;
use super::opencode_bridge::{CancelOutcome, OpenCodeBridge, PermissionDecisionOutcome};
use super::permission::{parse_permission_callback, PermissionCallback, PermissionDecision};
use super::storage::{load_config, save_config};
use super::types::{ExternalOpenCodeSessionInfo, InlineKeyboardButton, InlineKeyboardMarkup, PromptTargetSession, TelegramCallbackQuery, TelegramMessage, TelegramUpdate};

/// `/sessions` 的两种命令形态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionsCommand {
    Overview,
    Detail(String),
}

/// 仅接受精确的 `/sessions` 或 `/sessions <arg>` 边界。
pub fn parse_sessions_command(text: &str) -> Option<SessionsCommand> {
    let rest = text.trim().strip_prefix("/sessions")?;
    if rest.is_empty() {
        return Some(SessionsCommand::Overview);
    }
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let arg = rest.trim();
    if arg.is_empty() {
        Some(SessionsCommand::Overview)
    } else {
        Some(SessionsCommand::Detail(arg.to_string()))
    }
}

/// 会话概览按钮的回调数据解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCallback {
    Overview,
    Detail(String),
}

/// 仅接受 `open_sessions` 或带非空标识的 `open_session:<id>`。
pub fn parse_session_callback(data: &str) -> Option<SessionCallback> {
    if data == "open_sessions" {
        return Some(SessionCallback::Overview);
    }
    let identifier = data.strip_prefix("open_session:")?;
    if identifier.is_empty() {
        return None;
    }
    Some(SessionCallback::Detail(identifier.to_string()))
}

/// `/prompt` 与 `/send` 的解析结果：可选的目标标识与提示词内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPromptCommand {
    pub target: Option<String>,
    pub content: String,
}

/// 仅接受精确的 `/prompt` 或 `/send` 命令边界，并把首个目标标识从内容中分离。
pub fn parse_prompt_command(text: &str, target_tokens: &[String]) -> Option<ParsedPromptCommand> {
    let trimmed = text.trim();
    let rest = trimmed
        .strip_prefix("/prompt")
        .or_else(|| trimmed.strip_prefix("/send"))?;
    if !rest.is_empty() && !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }

    let remainder = rest.trim();
    if remainder.is_empty() {
        return Some(ParsedPromptCommand {
            target: None,
            content: String::new(),
        });
    }

    let mut parts = remainder.splitn(2, |c: char| c.is_whitespace());
    let first = parts.next().unwrap_or("");
    let tail = parts.next().unwrap_or("").trim();

    if target_tokens.iter().any(|token| token == first) {
        Some(ParsedPromptCommand {
            target: Some(first.to_string()),
            content: tail.to_string(),
        })
    } else {
        Some(ParsedPromptCommand {
            target: None,
            content: remainder.to_string(),
        })
    }
}

/// 仅接受 `prompt_choice:<非负整数>` 的目标选择回调。
pub fn parse_prompt_choice_callback(data: &str) -> Option<usize> {
    let index = data.strip_prefix("prompt_choice:")?;
    if index.is_empty() || !index.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    index.parse::<usize>().ok()
}

fn is_prompt_command_boundary(text: &str) -> bool {
    ["/prompt", "/send"].iter().any(|command| {
        text.strip_prefix(command)
            .map(|rest| rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace()))
            .unwrap_or(false)
    })
}

fn prompt_target_for(session: &ExternalOpenCodeSessionInfo) -> Result<PromptTargetSession, String> {
    let session_id = session.session_id.clone().ok_or_else(|| {
        "⚠️ 运行中的 OpenCode 尚未初始化 session_id，请稍候再试。".to_string()
    })?;
    Ok(PromptTargetSession {
        pid: session.pid,
        session_id,
        cwd: session.cwd.clone(),
        project_name: session.project_name.clone(),
    })
}

struct PendingPromptChoice {
    session: ExternalOpenCodeSessionInfo,
    content: String,
}

pub struct CommandRouter {
    client: Arc<TelegramClient>,
    bridge: Arc<OpenCodeBridge>,
    pub supervisor: Arc<super::session_supervisor::SessionSupervisor>,
    pending_prompt_targets: tokio::sync::RwLock<std::collections::HashMap<i64, PromptTargetSession>>,
    pending_prompt_choices: tokio::sync::RwLock<std::collections::HashMap<i64, Vec<PendingPromptChoice>>>,
}

impl CommandRouter {
    pub fn new(client: Arc<TelegramClient>, bridge: Arc<OpenCodeBridge>) -> Self {
        let supervisor = Arc::new(super::session_supervisor::SessionSupervisor::new(
            client.clone(),
            bridge.clone(),
        ));

        Self {
            client,
            bridge,
            supervisor,
            pending_prompt_targets: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            pending_prompt_choices: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 分发处理单条 Telegram Update
    pub async fn dispatch_update(&self, update: TelegramUpdate) -> Result<(), String> {
        if let Some(msg) = update.message {
            self.handle_message(msg).await?;
        } else if let Some(cb) = update.callback_query {
            self.handle_callback_query(cb).await?;
        }
        Ok(())
    }

    async fn handle_message(&self, msg: TelegramMessage) -> Result<(), String> {
        let chat_id = msg.chat.id;
        let text = msg.text.as_deref().unwrap_or("").trim();
        let mut config = load_config()?;

        // 鉴权检查
        if !AuthGuard::is_authorized(chat_id, &config) {
            // 首次绑定通道：如果白名单为空且用户发送 /bind 或 /start
            if AuthGuard::can_initial_bind(&config) && (text == "/bind" || text.starts_with("/start")) {
                config.allowed_chat_ids.push(chat_id);
                config.updated_at = chrono::Utc::now().timestamp();
                save_config(&config)?;

                info!("Telegram 远程控制已成功绑定首个管理员 Chat ID: {}", chat_id);
                let hostname = crate::get_hostname();
                let welcome = CardBuilder::welcome_card(chat_id, &hostname);
                self.client
                    .send_message(
                        chat_id,
                        &welcome,
                        Some(CardBuilder::quick_actions()),
                        Some("Markdown"),
                    )
                    .await?;
                return Ok(());
            }

            warn!("拒绝未授权的 Telegram 访问请求，Chat ID: {}", chat_id);
            let reject_text = CardBuilder::unauthorized_card(chat_id);
            self.client.send_message(chat_id, &reject_text, None, Some("Markdown")).await?;
            return Ok(());
        }

        // 新消息开始时清理旧的提示词目标与目标选择器，避免误消费过期状态
        let pending_target = {
            let mut lock = self.pending_prompt_targets.write().await;
            lock.remove(&chat_id)
        };
        self.pending_prompt_choices.write().await.remove(&chat_id);

        if let Some(target) = pending_target {
            if !text.starts_with('/') {
                return self.dispatch_prompt_to_session(chat_id, target, text).await;
            }
        }

        // 授权用户的命令路由
        if text.starts_with("/start") || text.starts_with("/help") {
            let help_text = CardBuilder::help_card();
            self.client
                .send_message(
                    chat_id,
                    &help_text,
                    Some(CardBuilder::quick_actions()),
                    Some("Markdown"),
                )
                .await?;
        } else if let Some(command) = parse_sessions_command(text) {
            self.handle_sessions_command(chat_id, command).await?;
        } else if text.starts_with("/running") {
            let redirect = CardBuilder::sessions_redirect_text();
            self.client
                .send_message(
                    chat_id,
                    &redirect,
                    Some(CardBuilder::sessions_actions()),
                    Some("Markdown"),
                )
                .await?;
        } else if text == "/host" || text == "/hosts" {
            let redirect = CardBuilder::sessions_redirect_text();
            self.client
                .send_message(
                    chat_id,
                    &redirect,
                    Some(CardBuilder::sessions_actions()),
                    Some("Markdown"),
                )
                .await?;
        } else if text.starts_with("/host ") {
            self.handle_host_command(chat_id, text).await?;
        } else if text == "/unhost" {
            self.handle_unhost_command(chat_id).await?;
        } else if is_prompt_command_boundary(text) {
            self.handle_prompt_command(chat_id, text).await?;
        } else if text == "/status" {
            self.handle_status_command(chat_id).await?;
        } else if text == "/projects" || text.starts_with("/projects ") {
            let arg = text.trim_start_matches("/projects").trim();
            let opt_arg = if arg.is_empty() { None } else { Some(arg) };
            self.handle_projects_command(chat_id, opt_arg).await?;
        } else if text == "/plans" {
            self.handle_plans_command(chat_id).await?;
        } else if text.starts_with("/progress") {
            let arg = text.trim_start_matches("/progress").trim();
            let plan_id = (!arg.is_empty()).then_some(arg);
            let redirect = CardBuilder::progress_redirect_text(plan_id);
            self.client
                .send_message(
                    chat_id,
                    &redirect,
                    Some(CardBuilder::sessions_actions()),
                    Some("Markdown"),
                )
                .await?;
        } else if text.starts_with("/diff") {
            self.handle_diff_command(chat_id, text).await?;
        } else if text == "/cancel" {
            let outcome = self.bridge.cancel_active_task().await;
            self.pending_prompt_targets.write().await.remove(&chat_id);
            self.pending_prompt_choices.write().await.remove(&chat_id);
            let cancel_text = match outcome {
                CancelOutcome::NothingToCancel => CardBuilder::nothing_to_cancel_card(),
                CancelOutcome::Cancelled => CardBuilder::task_cancelled_card(),
                CancelOutcome::ForcedCancelled => CardBuilder::task_forced_cancel_card(),
            };
            self.client
                .send_message(
                    chat_id,
                    &cancel_text,
                    Some(CardBuilder::quick_actions()),
                    Some("Markdown"),
                )
                .await?;
        } else if text.starts_with("/coding") || text.starts_with("/implement") {
            self.handle_coding_command(chat_id, text).await?;
        } else if text == "/planning" || text.starts_with("/planning ") || text == "/plan" || text.starts_with("/plan ") {
            let prompt = if let Some(stripped) = text.strip_prefix("/planning") {
                stripped.trim()
            } else if let Some(stripped) = text.strip_prefix("/plan") {
                stripped.trim()
            } else {
                ""
            };

            if prompt.is_empty() {
                let err_text = CardBuilder::argument_error_card(
                    "/planning",
                    "未提供需求描述文本",
                    "/planning 为项目增加 Telegram 登录认证",
                );
                self.client.send_message(
                    chat_id,
                    &err_text,
                    Some(CardBuilder::quick_actions()),
                    Some("Markdown")
                ).await?;
            } else {
                let project_dir = resolve_project_dir(&config);
                let project_name = project_dir.file_name().unwrap_or_default().to_string_lossy();
                let (ack_text, markup) = CardBuilder::new_feature_ack_card(&project_name, prompt, true);
                self.client.send_message(chat_id, &ack_text, Some(markup), Some("Markdown")).await?;

                if let Err(e) = self.bridge.execute_task(chat_id, project_dir, prompt, true).await {
                    self.client.send_message(
                        chat_id,
                        &format!("❌ 启动规划任务失败: `{}`", e),
                        Some(CardBuilder::quick_actions()),
                        Some("Markdown")
                    ).await?;
                }
            }
        } else {
            // 托管模式优先处理：检查当前是否存在完全托管中的会话
            let hosted_sessions = self.supervisor.list_hosted_sessions(Some(chat_id)).await;
            if let Some(hosted) = hosted_sessions.first() {
                let trimmed = text.trim();
                let parsed_num_opt = trimmed.trim_end_matches('.').trim_end_matches('、').parse::<usize>().ok();

                // 1. 若当前会话正阻塞在提问等待应答，且用户回复了数字序号
                if let Some(opt_idx) = parsed_num_opt {
                    if self.supervisor.get_active_question(&hosted.session_id).await.is_some() {
                        match self.supervisor.handle_answer_question(chat_id, &hosted.session_id, opt_idx).await {
                            Ok(chosen) => {
                                let confirm = format!("✅ *已代决断*：已采纳方案「{}」\n_OpenCode 正在继续推进..._", chosen);
                                self.client.send_message(chat_id, &confirm, None, Some("Markdown")).await?;
                                return Ok(());
                            }
                            Err(e) => {
                                self.client.send_message(chat_id, &format!("⚠️ 决断提交失败: {}", e), Some(CardBuilder::quick_actions()), None).await?;
                                return Ok(());
                            }
                        }
                    }
                }

                // 2. 托管状态下的自由动态输入：直接将自然语言作为提示词注入该托管会话
                let target = PromptTargetSession {
                    pid: hosted.pid,
                    session_id: hosted.session_id.clone(),
                    cwd: hosted.cwd.clone(),
                    project_name: hosted.project_name.clone(),
                };
                self.dispatch_prompt_to_session(chat_id, target, text).await?;
                return Ok(());
            }

            // 普通文本消息：如果正有活跃规划会话，则作为回复继续推进；否则提示帮助
            if self.bridge.has_active_session().await {
                // 如果用户输入的是纯数字序号（如 "1" 或 "1." 或 "1、"），优先映射为对应的 active_options 完整文本
                let trimmed = text.trim();
                let resolved_text = if let Ok(num) = trimmed.trim_end_matches('.').trim_end_matches('、').parse::<usize>() {
                    if let Some(opt_text) = self.bridge.get_option_text(num).await {
                        opt_text
                    } else {
                        text.to_string()
                    }
                } else {
                    text.to_string()
                };

                if let Err(e) = self.bridge.continue_planning(chat_id, &resolved_text).await {
                    self.client.send_message(
                        chat_id,
                        &format!("❌ 推进处理失败: {}", e),
                        Some(CardBuilder::quick_actions()),
                        None,
                    ).await?;
                }
            } else {
                // 空闲状态下：检查是否开启自然语言自由对话
                if !config.enable_natural_dialogue {
                    let project_dir = resolve_project_dir(&config);
                    let project_name = project_dir.file_name().unwrap_or_default().to_string_lossy();
                    let guard_text = format!(
                        "💡 *自然语言自由对话已关闭*\n\
                        > ▫️ 防误触保护: `已开启`\n\
                        > ▫️ 生效工作区: `{}`\n\n\
                        若要唤起 OpenCode 开始新需求规划，请使用指令：\n\
                        `/planning <需求描述>`\n\n\
                        _（可在 Mac 端 OneSpace 设置 -> 远程工作台中开启自由对话）_",
                        project_name
                    );
                    self.client.send_message(chat_id, &guard_text, Some(CardBuilder::quick_actions()), Some("Markdown")).await?;
                    return Ok(());
                }

                // 开启时，普通聊天对话开始新会话，遵循 OpenCode 自身工作流自主选择 coding 或 planning
                let project_dir = resolve_project_dir(&config);
                let project_name = project_dir.file_name().unwrap_or_default().to_string_lossy();
                let (ack_text, markup) = CardBuilder::new_feature_ack_card(&project_name, text, false);
                self.client.send_message(chat_id, &ack_text, Some(markup), Some("Markdown")).await?;

                if let Err(e) = self.bridge.execute_task(chat_id, project_dir, text, false).await {
                    self.client.send_message(
                        chat_id,
                        &format!("❌ 启动处理失败: {}", e),
                        Some(CardBuilder::quick_actions()),
                        None,
                    ).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_status_command(&self, chat_id: i64) -> Result<(), String> {
        let config = load_config()?;
        let project_dir = resolve_project_dir(&config);
        let hostname = crate::get_hostname();
        let current_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 探测外部运行中的 OpenCode 会话
        let ext_sessions = super::opencode_inspector::discover_external_opencode_sessions().await;

        // 探测 Git 分支
        let git_branch = Command::new("git")
            .current_dir(&project_dir)
            .args(["branch", "--show-current"])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "detached / 未知".to_string());

        // 探测 Git 工作区变更状态
        let git_status = Command::new("git")
            .current_dir(&project_dir)
            .args(["status", "--porcelain"])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let lines_count = stdout.lines().count();
                if lines_count == 0 {
                    "干净 (Clean)".to_string()
                } else {
                    format!("存在 {} 处未提交变更", lines_count)
                }
            })
            .unwrap_or_else(|| "无法获取状态".to_string());

        // 探测最新 Git 提交记录
        let git_latest_commit = Command::new("git")
            .current_dir(&project_dir)
            .args(["log", "-1", "--pretty=format:%h - %s (%cr)"])
            .output()
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "暂无提交记录".to_string());

        let (status_text, markup) = CardBuilder::status_card_with_sessions(
            &hostname,
            &current_time,
            config.active_workspace.as_deref().unwrap_or("onespace-app"),
            &git_branch,
            &git_status,
            &git_latest_commit,
            config.auto_approve_permissions,
            ext_sessions.len(),
        );

        self.client.send_message(chat_id, &status_text, markup, Some("Markdown")).await?;
        Ok(())
    }

    async fn handle_running_command(&self, chat_id: i64) -> Result<(), String> {
        let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
        if sessions.is_empty() {
            let hostname = crate::get_hostname();
            let (text, markup) = CardBuilder::no_external_session_card(&hostname);
            self.client.send_message(chat_id, &text, markup, Some("Markdown")).await?;
        } else if sessions.len() == 1 {
            let (text, markup) = CardBuilder::external_session_card(&sessions[0]);
            self.client.send_message(chat_id, &text, markup, Some("Markdown")).await?;
        } else {
            let (text, markup) = CardBuilder::multiple_external_sessions_card(&sessions);
            self.client.send_message(chat_id, &text, Some(markup), Some("Markdown")).await?;
        }
        Ok(())
    }

    /// 聚合外部终端、托管、Telegram 会话与当前方案执行会话。
    async fn collect_session_inventory(&self) -> Vec<super::session_inventory::SessionEntry> {
        use super::session_inventory::{
            build_session_entries, ConversationSessionState, PlanExecutionState,
            SessionInventorySources,
        };

        let external = super::opencode_inspector::discover_inventory_opencode_sessions().await;
        let hosted = self.supervisor.list_hosted_sessions(None).await;
        let snapshot = self.bridge.session_snapshot().await;

        let conversation = if snapshot.conversation_session_id.is_some()
            || snapshot.conversation_project_name.is_some()
            || snapshot.conversation_busy
        {
            Some(ConversationSessionState {
                session_id: snapshot.conversation_session_id,
                project_name: snapshot.conversation_project_name,
                is_busy: snapshot.conversation_busy,
            })
        } else {
            None
        };

        let plan_execution = snapshot.task.and_then(|task| {
            if task.kind == super::opencode_bridge::BridgeTaskKind::Coding
                && task.plan_id.is_some()
                && task.is_busy
            {
                Some(PlanExecutionState {
                    plan_id: task.plan_id.unwrap_or_default(),
                    session_id: task.session_id,
                    project_name: task.project_name.unwrap_or_default(),
                    is_busy: task.is_busy,
                })
            } else {
                None
            }
        });

        build_session_entries(&SessionInventorySources {
            external,
            hosted,
            conversation,
            plan_execution,
        })
    }

    /// 仅当桥接任务正在实施的方案 ID 匹配时，返回当前执行状态。
    async fn current_plan_execution_state(
        &self,
        plan_id: &str,
    ) -> Option<super::session_inventory::PlanExecutionState> {
        use super::opencode_bridge::BridgeTaskKind;

        let snapshot = self.bridge.session_snapshot().await;
        snapshot.task.and_then(|task| {
            if task.kind == BridgeTaskKind::Coding
                && task.is_busy
                && task.plan_id.as_deref() == Some(plan_id)
            {
                Some(super::session_inventory::PlanExecutionState {
                    plan_id: task.plan_id.unwrap_or_default(),
                    session_id: task.session_id,
                    project_name: task.project_name.unwrap_or_default(),
                    is_busy: task.is_busy,
                })
            } else {
                None
            }
        })
    }

    async fn handle_sessions_command(
        &self,
        chat_id: i64,
        command: SessionsCommand,
    ) -> Result<(), String> {
        match command {
            SessionsCommand::Overview => {
                let entries = self.collect_session_inventory().await;
                let (text, markup) = super::session_inventory::format_sessions_overview(&entries);
                self.client
                    .send_message(chat_id, &text, markup, Some("Markdown"))
                    .await?;
            }
            SessionsCommand::Detail(identifier) => {
                let entries = self.collect_session_inventory().await;
                match super::session_inventory::resolve_session_identifier(&entries, &identifier) {
                    super::session_inventory::IdentifierResolution::Session(entry) => {
                        let progress = entry.session_id.as_deref().and_then(|session_id| {
                            super::session_inventory::lookup_session_progress(
                                session_id,
                                &super::opencode_inspector::resolve_opencode_db_path(),
                            )
                        });
                        let (text, markup) =
                            CardBuilder::session_detail_card(&entry, progress.as_ref());
                        self.client
                            .send_message(chat_id, &text, markup, Some("Markdown"))
                            .await?;
                    }
                    super::session_inventory::IdentifierResolution::Plan { plan_id, .. } => {
                        let config = load_config()?;
                        let project_dir = resolve_project_dir(&config);
                        match super::plan_progress::get_plan_progress(&project_dir, &plan_id) {
                            Ok(info) => {
                                let execution =
                                    self.current_plan_execution_state(&info.plan_id).await;
                                let (text, markup) = CardBuilder::plan_session_detail_card(
                                    &info,
                                    execution.as_ref(),
                                );
                                self.client
                                    .send_message(chat_id, &text, markup, Some("Markdown"))
                                    .await?;
                            }
                            Err(_) => {
                                let text = CardBuilder::identifier_error_text(
                                    &identifier,
                                    &super::session_inventory::IdentifierResolution::Unknown,
                                );
                                self.client
                                    .send_message(
                                        chat_id,
                                        &text,
                                        Some(CardBuilder::sessions_actions()),
                                        Some("Markdown"),
                                    )
                                    .await?;
                            }
                        }
                    }
                    super::session_inventory::IdentifierResolution::Ambiguous(candidates) => {
                        let text = CardBuilder::identifier_error_text(
                            &identifier,
                            &super::session_inventory::IdentifierResolution::Ambiguous(candidates),
                        );
                        self.client
                            .send_message(
                                chat_id,
                                &text,
                                Some(CardBuilder::sessions_actions()),
                                Some("Markdown"),
                            )
                            .await?;
                    }
                    super::session_inventory::IdentifierResolution::Unknown => {
                        let text = CardBuilder::identifier_error_text(
                            &identifier,
                            &super::session_inventory::IdentifierResolution::Unknown,
                        );
                        self.client
                            .send_message(
                                chat_id,
                                &text,
                                Some(CardBuilder::sessions_actions()),
                                Some("Markdown"),
                            )
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_host_command(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let arg = text.trim_start_matches("/host").trim_start_matches("s").trim();

        // 1. 若带参数（PID 或 SessionID 或项目名称）
        if !arg.is_empty() {
            let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
            let matched = sessions.into_iter().find(|s| {
                s.pid.to_string() == arg || s.session_id.as_deref() == Some(arg) || s.project_name.to_lowercase() == arg.to_lowercase()
            });

            if let Some(session) = matched {
                let mode = super::types::SessionHostingDecisionMode::Manual;
                match self.supervisor.start_hosting(chat_id, session, mode).await {
                    Ok(msg) => info!("{}", msg),
                    Err(e) => {
                        self.client.send_message(chat_id, &format!("❌ 启动托管失败: {}", e), Some(CardBuilder::sessions_actions()), None).await?;
                    }
                }
                return Ok(());
            } else {
                self.client.send_message(chat_id, &format!("⚠️ 未找到匹配 `{}` 的正在运行会话。\n输入 `/sessions` 查看当前活跃会话。", arg), Some(CardBuilder::sessions_actions()), None).await?;
                return Ok(());
            }
        }

        // 2. 无参数：检查当前是否已有托管中的会话
        let current_hosted = self.supervisor.list_hosted_sessions(Some(chat_id)).await;
        if !current_hosted.is_empty() {
            let mut summary = "🤖 *当前正在完全托管的会话*：\n\n".to_string();
            let mut buttons = Vec::new();
            for (idx, h) in current_hosted.iter().enumerate() {
                let mode_str = match h.mode {
                    super::types::SessionHostingDecisionMode::Manual => "🎯 手动决断",
                    super::types::SessionHostingDecisionMode::AutoRecommended => "⚡ 自动推荐",
                };
                summary.push_str(&format!(
                    "{} *{}* (PID: {})\n> ├─ 会话: `{}`\n> ├─ 模式: `{}`\n> └─ 进度: Step {} · Todos {}/{} 完成\n\n",
                    CardBuilder::number_badge(idx + 1),
                    h.project_name,
                    h.pid,
                    h.session_id,
                    mode_str,
                    h.last_observed_step,
                    h.completed_todos_count,
                    h.total_todos_count
                ));
                buttons.push(vec![
                    InlineKeyboardButton::callback(format!("⚙️ 切换模式 ({})", h.project_name), format!("toggle_host_mode:{}", h.session_id)),
                    InlineKeyboardButton::callback(format!("🛑 退出托管 ({})", h.project_name), format!("stop_host:{}", h.session_id)),
                ]);
            }
            summary.push_str("_可在聊天框直接回复以注入指令，或发送 `/unhost` 退出托管。_");
            self.client.send_message(chat_id, &summary, Some(InlineKeyboardMarkup::new(buttons)), Some("Markdown")).await?;
            return Ok(());
        }

        // 3. 无托管会话时：扫描当前外部会话并提供一键托管按钮
        let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
        if sessions.is_empty() {
            let hostname = crate::get_hostname();
            let (no_sess, markup) = CardBuilder::no_external_session_card(&hostname);
            self.client.send_message(chat_id, &no_sess, markup, Some("Markdown")).await?;
            return Ok(());
        }

        if sessions.len() == 1 {
            let sess = sessions.into_iter().next().unwrap();
            let mode = super::types::SessionHostingDecisionMode::Manual;
            self.supervisor.start_hosting(chat_id, sess, mode).await?;
        } else {
            let mut host_buttons = Vec::new();
            for (idx, sess) in sessions.iter().enumerate() {
                let label = format!("{} 托管: {} (PID: {})", CardBuilder::number_badge(idx + 1), sess.project_name, sess.pid);
                host_buttons.push(vec![
                    InlineKeyboardButton::callback(label, format!("start_host:{}", sess.pid)),
                ]);
            }
            let choose_text = "🤖 *请选择要交由 OneSpace + Telegram 完全托管的运行中会话*：\n\n\
            托管后将自动监听进展、主动通知待办完成、接管提问决策并支持随时动态干预。";
            self.client.send_message(chat_id, choose_text, Some(InlineKeyboardMarkup::new(host_buttons)), Some("Markdown")).await?;
        }

        Ok(())
    }

    async fn handle_unhost_command(&self, chat_id: i64) -> Result<(), String> {
        let count = self.supervisor.stop_all_for_chat(chat_id).await;
        if count > 0 {
            self.client
                .send_message(
                    chat_id,
                    &format!("🛑 已成功停止针对 {} 个会话的完全托管守护。", count),
                    Some(CardBuilder::sessions_actions()),
                    None,
                )
                .await?;
        } else {
            self.client
                .send_message(
                    chat_id,
                    "ℹ️ 当前没有处于托管守护状态的会话。",
                    Some(CardBuilder::sessions_actions()),
                    None,
                )
                .await?;
        }
        Ok(())
    }

    async fn handle_prompt_command(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
        let target_tokens: Vec<String> = sessions
            .iter()
            .flat_map(|session| {
                let pid = session.pid.to_string();
                session.session_id.clone().into_iter().chain(std::iter::once(pid))
            })
            .collect();
        let parsed = match parse_prompt_command(text, &target_tokens) {
            Some(parsed) => parsed,
            None => return Ok(()),
        };

        if sessions.is_empty() {
            let hostname = crate::get_hostname();
            let (empty_text, markup) = CardBuilder::no_external_session_card(&hostname);
            self.client.send_message(chat_id, &empty_text, markup, Some("Markdown")).await?;
            return Ok(());
        }

        if sessions.len() == 1 {
            let session = &sessions[0];
            let target = match prompt_target_for(session) {
                Ok(target) => target,
                Err(message) => {
                    self.client.send_message(chat_id, &message, Some(CardBuilder::sessions_actions()), None).await?;
                    return Ok(());
                }
            };

            if parsed.content.is_empty() {
                self.pending_prompt_targets.write().await.insert(chat_id, target);
                let (req_text, markup) = CardBuilder::prompt_input_request_card(session);
                self.client.send_message(chat_id, &req_text, Some(markup), Some("Markdown")).await?;
            } else {
                self.dispatch_prompt_to_session(chat_id, target, &parsed.content).await?;
            }
            return Ok(());
        }

        let selected = parsed.target.as_deref().and_then(|target| {
            sessions.iter().find(|session| {
                session.session_id.as_deref() == Some(target) || session.pid.to_string() == target
            })
        });

        if let Some(session) = selected {
            let target = match prompt_target_for(session) {
                Ok(target) => target,
                Err(message) => {
                    self.client.send_message(chat_id, &message, Some(CardBuilder::sessions_actions()), None).await?;
                    return Ok(());
                }
            };

            if parsed.content.is_empty() {
                self.pending_prompt_targets.write().await.insert(chat_id, target);
                let (req_text, markup) = CardBuilder::prompt_input_request_card(session);
                self.client.send_message(chat_id, &req_text, Some(markup), Some("Markdown")).await?;
            } else {
                self.dispatch_prompt_to_session(chat_id, target, &parsed.content).await?;
            }
            return Ok(());
        }

        let choices = sessions
            .iter()
            .cloned()
            .map(|session| PendingPromptChoice {
                session,
                content: parsed.content.clone(),
            })
            .collect();
        self.pending_prompt_choices.write().await.insert(chat_id, choices);

        let (selection_text, markup) =
            CardBuilder::prompt_target_selection_card(&parsed.content, &sessions);
        self.client
            .send_message(chat_id, &selection_text, Some(markup), Some("Markdown"))
            .await?;

        Ok(())
    }

    async fn dispatch_prompt_to_session(
        &self,
        chat_id: i64,
        target: PromptTargetSession,
        prompt: &str,
    ) -> Result<(), String> {
        let ack = CardBuilder::prompt_dispatched_ack_card(&target, prompt);
        self.client
            .send_message(
                chat_id,
                &ack,
                Some(CardBuilder::prompt_ack_actions()),
                Some("Markdown"),
            )
            .await?;

        if let Err(e) = self.bridge.execute_prompt_for_session(
            chat_id,
            PathBuf::from(&target.cwd),
            &target.session_id,
            prompt,
        ).await {
            self.client.send_message(chat_id, &format!("❌ 发送提示词失败: {}", e), Some(CardBuilder::prompt_ack_actions()), None).await?;
        }

        Ok(())
    }

    async fn handle_diff_command(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let config = load_config()?;
        let project_dir = resolve_project_dir(&config);
        let arg = text.trim_start_matches("/diff").trim();
        self.send_diff_view(chat_id, &project_dir, if arg.is_empty() { None } else { Some(arg) }).await
    }

    pub async fn send_diff_view(&self, chat_id: i64, project_dir: &std::path::Path, plan_id_opt: Option<&str>) -> Result<(), String> {
        let mut target_scope = "HEAD~1..HEAD".to_string();

        if let Some(plan_id) = plan_id_opt {
            let clean_id = plan_id
                .trim_start_matches(".ai-workflow/plans/")
                .trim_start_matches("plans/")
                .trim_matches('/');

            let mut found_commit = false;
            let yaml_path = project_dir.join(".ai-workflow").join("plans").join(clean_id).join("implementation.yaml");
            if yaml_path.is_file() {
                if let Ok(content) = tokio::fs::read_to_string(&yaml_path).await {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("commit:") {
                            let c = trimmed.trim_start_matches("commit:").trim().trim_matches('\'').trim_matches('"');
                            if !c.is_empty() {
                                target_scope = format!("{}~1..{}", c, c);
                                found_commit = true;
                                break;
                            }
                        }
                    }
                }
            }

            if !found_commit {
                let branch_name = format!("ai-workflow/{}", clean_id);
                let check_branch = Command::new("git")
                    .current_dir(project_dir)
                    .args(["rev-parse", "--verify", &branch_name])
                    .output()
                    .await;
                if let Ok(out) = check_branch {
                    if out.status.success() {
                        target_scope = format!("HEAD..{}", branch_name);
                    }
                }
            }
        }

        // 1. 获取 diff stat 统计
        let stat_out = Command::new("git")
            .current_dir(project_dir)
            .args(["diff", &target_scope, "--stat"])
            .output()
            .await;

        let stat_text = match stat_out {
            Ok(ref out) if out.status.success() => {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if s.is_empty() {
                    let uncommitted_stat = Command::new("git")
                        .current_dir(project_dir)
                        .args(["diff", "--stat"])
                        .output()
                        .await;
                    match uncommitted_stat {
                        Ok(ref u_out) if !u_out.stdout.is_empty() => {
                            target_scope = "工作区未提交修改".to_string();
                            String::from_utf8_lossy(&u_out.stdout).trim().to_string()
                        }
                        _ => "暂无代码改动".to_string(),
                    }
                } else {
                    s
                }
            }
            _ => "暂无代码改动或无法执行 diff".to_string(),
        };

        // 2. 获取具体代码差异内容片段
        let diff_args = if target_scope == "工作区未提交修改" {
            vec!["diff".to_string()]
        } else {
            vec!["diff".to_string(), target_scope.clone()]
        };

        let diff_content_out = Command::new("git")
            .current_dir(project_dir)
            .args(&diff_args)
            .output()
            .await;

        let raw_diff = match diff_content_out {
            Ok(ref out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            _ => String::new(),
        };

        let snippet = if raw_diff.is_empty() {
            "（无具体变更行差异）".to_string()
        } else {
            raw_diff
        };

        let title_plan = plan_id_opt.unwrap_or("最新提交对比");
        let diff_msg = CardBuilder::diff_preview_card(
            title_plan,
            &target_scope,
            &stat_text,
            &snippet,
        );

        self.client
            .send_message(
                chat_id,
                &diff_msg,
                Some(CardBuilder::quick_actions()),
                Some("Markdown"),
            )
            .await?;
        Ok(())
    }

    pub async fn send_failure_log_view(&self, chat_id: i64, project_dir: &std::path::Path, plan_id: &str) -> Result<(), String> {
        let clean_id = plan_id
            .trim_start_matches(".ai-workflow/plans/")
            .trim_start_matches("plans/")
            .trim_matches('/');

        let reports_dir = project_dir.join(".ai-workflow").join("plans").join(clean_id).join("reports");
        let mut log_content: Option<String> = None;

        if let Ok(entries) = std::fs::read_dir(&reports_dir) {
            let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            files.sort_by(|a, b| {
                let ma = a.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let mb = b.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                mb.cmp(&ma)
            });

            for f in files {
                if let Ok(content) = tokio::fs::read_to_string(&f).await {
                    let lines: Vec<&str> = content.lines().collect();
                    let tail_lines: Vec<&str> = if lines.len() > 30 {
                        lines[lines.len() - 30..].to_vec()
                    } else {
                        lines
                    };
                    log_content = Some(format!("📄 来自 `{}`:\n\n{}", f.file_name().unwrap_or_default().to_string_lossy(), tail_lines.join("\n")));
                    break;
                }
            }
        }

        let body = log_content.unwrap_or_else(|| {
            format!("未能找到方案 `{}` 的详细报告日志文件。\n建议通过 `/status` 检查 Git 状态或点击重试按钮继续修复。", clean_id)
        });

        let (msg, markup) = CardBuilder::diagnostic_log_card(clean_id, None, &body);
        self.client.send_message(chat_id, &msg, Some(markup), Some("Markdown")).await?;
        Ok(())
    }

    async fn handle_projects_command(&self, chat_id: i64, arg: Option<&str>) -> Result<(), String> {
        let mut config = load_config()?;
        let all = collect_all_workspaces(&config);
        let featured = collect_featured_projects(&config);
        let active_path = resolve_project_dir(&config);
        let active_name = all.iter()
            .find(|it| it.path == active_path)
            .map(|it| it.name.as_str())
            .unwrap_or_else(|| {
                active_path.file_name().and_then(|n| n.to_str()).unwrap_or("onespace-app")
            });

        // 1. 若没有带参数，展示精选常用项目卡片
        let Some(raw_arg) = arg.map(str::trim).filter(|s| !s.is_empty()) else {
            let candidate_refs: Vec<&str> = featured.iter().map(|it| it.name.as_str()).collect();
            let (text, markup) = CardBuilder::projects_card(
                active_name,
                &active_path.display().to_string(),
                &candidate_refs,
            );
            self.client.send_message(chat_id, &text, Some(markup), Some("Markdown")).await?;
            return Ok(());
        };

        // 2. 带参数：检查是否为纯数字序号（针对精选列表）
        let time_now = chrono::Local::now().format("%H:%M:%S").to_string();
        if let Ok(idx) = raw_arg.parse::<usize>() {
            if idx >= 1 && idx <= featured.len() {
                let target = &featured[idx - 1];
                config.active_workspace = Some(target.path.to_string_lossy().to_string());
                config.updated_at = chrono::Utc::now().timestamp();
                save_config(&config)?;

                let confirm_text = CardBuilder::project_switch_confirmed(&target.name, &time_now);
                self.client
                    .send_message(
                        chat_id,
                        &confirm_text,
                        Some(CardBuilder::quick_actions()),
                        Some("Markdown"),
                    )
                    .await?;
                return Ok(());
            }
        }

        // 3. 带参数：在全量系统工作空间与自定义目录中进行模糊匹配
        let kw_lower = raw_arg.to_lowercase();
        let matched: Vec<&ProjectItem> = all.iter().filter(|it| {
            it.name.to_lowercase().contains(&kw_lower)
                || it.path.to_string_lossy().to_lowercase().contains(&kw_lower)
        }).collect();

        if matched.len() == 1 || matched.iter().any(|it| it.name.to_lowercase() == kw_lower) {
            let target = matched.iter().find(|it| it.name.to_lowercase() == kw_lower).unwrap_or(&matched[0]);
            config.active_workspace = Some(target.path.to_string_lossy().to_string());
            config.updated_at = chrono::Utc::now().timestamp();
            save_config(&config)?;

            let confirm_text = CardBuilder::project_switch_confirmed(&target.name, &time_now);
            self.client
                .send_message(
                    chat_id,
                    &confirm_text,
                    Some(CardBuilder::quick_actions()),
                    Some("Markdown"),
                )
                .await?;
        } else if matched.len() > 1 {
            // 匹配到多个，最多展示 6 个候选项供点击
            let search_candidates: Vec<(&str, &str)> = matched.iter().take(6).map(|it| {
                (it.name.as_str(), it.name.as_str())
            }).collect();
            let (text, markup) = CardBuilder::projects_search_card(raw_arg, &search_candidates);
            self.client.send_message(chat_id, &text, Some(markup), Some("Markdown")).await?;
        } else {
            let not_found_text = CardBuilder::project_not_found_card(raw_arg);
            self.client
                .send_message(
                    chat_id,
                    &not_found_text,
                    Some(CardBuilder::projects_actions()),
                    Some("Markdown"),
                )
                .await?;
        }

        Ok(())
    }

    async fn handle_plans_command(&self, chat_id: i64) -> Result<(), String> {
        let config = load_config()?;
        let project_dir = resolve_project_dir(&config);
        let plans_dir = project_dir.join(".ai-workflow").join("plans");

        let mut available_plans: Vec<(String, std::time::SystemTime)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&plans_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("plan.md").is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let mtime = path.join("plan.md").metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    available_plans.push((name, mtime));
                }
            }
        }

        if available_plans.is_empty() {
            self.client.send_message(
                chat_id,
                "📂 当前工作区的 `.ai-workflow/plans/` 下未检测到任何方案。\n输入 `/planning <需求描述>` 可开始新方案规划。",
                Some(CardBuilder::quick_actions()),
                None,
            ).await?;
            return Ok(());
        }

        // 按修改时间降序排序，最新的排在最前
        available_plans.sort_by(|a, b| b.1.cmp(&a.1));

        let mut plan_triplets = Vec::new();
        for (plan_id, _) in available_plans.iter().take(6) {
            let (status_badge, can_run) = if let Ok(info) = super::plan_progress::get_plan_progress(&project_dir, plan_id) {
                match info.status {
                    super::plan_progress::PlanExecutionStatus::Completed => ("🟢 `已交付`".to_string(), false),
                    super::plan_progress::PlanExecutionStatus::InProgress => ("🟡 `实施中`".to_string(), false),
                    super::plan_progress::PlanExecutionStatus::NotStarted => ("⚪ `未实施`".to_string(), true),
                    super::plan_progress::PlanExecutionStatus::Failed => ("🔴 `中断/失败`".to_string(), true),
                }
            } else {
                ("⚪ `未实施`".to_string(), true)
            };
            plan_triplets.push((plan_id.clone(), status_badge, can_run));
        }

        let project_name = project_dir.file_name().unwrap_or_default().to_string_lossy();
        let (msg, markup) = CardBuilder::plans_list_card(&project_name, &plan_triplets);
        self.client.send_message(
            chat_id,
            &msg,
            Some(markup),
            Some("Markdown"),
        ).await?;

        Ok(())
    }

    async fn handle_coding_command(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let config = load_config()?;
        let project_dir = resolve_project_dir(&config);

        let arg = if let Some(stripped) = text.strip_prefix("/coding") {
            stripped.trim()
        } else if let Some(stripped) = text.strip_prefix("/implement") {
            stripped.trim()
        } else {
            ""
        };

        if arg.is_empty() {
            let err_text = CardBuilder::argument_error_card(
                "/coding",
                "请指定要实施的方案标识 (planId)",
                "/coding 20261106-settings-service-categories",
            );
            self.client.send_message(
                chat_id,
                &err_text,
                Some(CardBuilder::quick_actions()),
                Some("Markdown"),
            ).await?;
            return Ok(());
        }

        let clean_plan_id = arg
            .trim_start_matches(".ai-workflow/plans/")
            .trim_start_matches("plans/")
            .trim_matches('/')
            .to_string();

        let plan_file = project_dir.join(".ai-workflow").join("plans").join(&clean_plan_id).join("plan.md");
        if !plan_file.is_file() {
            let not_found_msg = format!(
                "⚠️ 未找到方案 `{}`\n文件不存在: `{}`\n\n输入 `/plans` 可查看当前工作区所有可用方案。",
                clean_plan_id,
                plan_file.display()
            );
            self.client
                .send_message(
                    chat_id,
                    &not_found_msg,
                    Some(CardBuilder::sessions_actions()),
                    Some("Markdown"),
                )
                .await?;
            return Ok(());
        }

        let (ack_text, _) = CardBuilder::coding_running_card(&clean_plan_id, false);
        self.client.send_message(chat_id, &ack_text, None, Some("Markdown")).await?;

        if let Err(e) = self.bridge.execute_coding(chat_id, project_dir, &clean_plan_id).await {
            self.client.send_message(chat_id, &format!("❌ 启动 Coding 实施失败: {}", e), Some(CardBuilder::quick_actions()), None).await?;
        }

        Ok(())
    }

    async fn handle_callback_query(&self, cb: TelegramCallbackQuery) -> Result<(), String> {
        let data = cb.data.as_deref().unwrap_or("");
        
        // 快速响应确认，消除客户端 loading 圈
        self.client.answer_callback_query(&cb.id, None).await?;

        if let Some(project) = data.strip_prefix("set_project:") {
            let mut config = load_config()?;
            let all = collect_all_workspaces(&config);
            let (target_name, target_path) = if let Some(found) = all.iter().find(|it| it.name == project || it.path.to_string_lossy() == project) {
                (found.name.clone(), found.path.to_string_lossy().to_string())
            } else {
                (project.to_string(), project.to_string())
            };

            config.active_workspace = Some(target_path);
            config.updated_at = chrono::Utc::now().timestamp();
            save_config(&config)?;

            if let Some(msg) = cb.message {
                let time_now = chrono::Local::now().format("%H:%M:%S").to_string();
                let update_text = CardBuilder::project_switch_confirmed(&target_name, &time_now);
                self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &update_text,
                    Some(CardBuilder::quick_actions()),
                    Some("Markdown"),
                ).await?;
            }
        } else if let Some(plan_tag) = data.strip_prefix("approve_plan:") {
            let config = load_config()?;
            let project_dir = resolve_project_dir(&config);

            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if let Some(ref msg) = cb.message {
                let (loading_text, _) = CardBuilder::coding_running_card(plan_tag, false);
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &loading_text,
                    None,
                    Some("Markdown"),
                ).await;
            }

            if let Err(e) = self.bridge.execute_coding(chat_id, project_dir, plan_tag).await {
                let _ = self.client.send_message(
                    chat_id,
                    &format!("❌ 启动 Coding 实施失败: {}", e),
                    Some(CardBuilder::quick_actions()),
                    None,
                ).await;
            }
        } else if let Some(plan_tag) = data.strip_prefix("revise_plan:") {
            if let Some(ref msg) = cb.message {
                let prompt_text = CardBuilder::revise_prompt_card(plan_tag);
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &prompt_text,
                    None,
                    Some("Markdown"),
                ).await;
            }
        } else if let Some(plan_id) = data.strip_prefix("check_progress:") {
            let config = load_config()?;
            let project_dir = resolve_project_dir(&config);

            match super::plan_progress::get_plan_progress(&project_dir, plan_id) {
                Ok(info) => {
                    let execution = self.current_plan_execution_state(&info.plan_id).await;
                    let (msg, markup) =
                        CardBuilder::plan_session_detail_card(&info, execution.as_ref());
                    if let Some(ref m) = cb.message {
                        let _ = self.client.edit_message_text(
                            m.chat.id,
                            m.message_id,
                            &msg,
                            markup,
                            Some("Markdown"),
                        ).await;
                    }
                }
                Err(e) => {
                    if let Some(ref m) = cb.message {
                        let _ = self.client.send_message(
                            m.chat.id,
                            &format!("⚠️ 刷新进度失败: {}", e),
                            Some(CardBuilder::sessions_actions()),
                            None,
                        ).await;
                    }
                }
            }
        } else if let Some(choice_num_str) = data.strip_prefix("ans_choice:") {
            let num: usize = choice_num_str.parse().unwrap_or(0);
            let choice_text = self.bridge.get_option_text(num).await
                .unwrap_or_else(|| format!("选项 {}", num));

            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if let Some(ref msg) = cb.message {
                let update_text = CardBuilder::question_answered_card(&choice_text);
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &update_text,
                    None,
                    Some("Markdown"),
                ).await;
            }

            if let Err(e) = self.bridge.continue_planning(chat_id, &choice_text).await {
                let _ = self.client.send_message(
                    chat_id,
                    &format!("❌ 推进规划失败: {}", e),
                    Some(CardBuilder::quick_actions()),
                    None,
                ).await;
            }
        } else if let Some(q_info) = data.strip_prefix("ans_q:") {
            // 取消后的任务不再接受迟到的提问回答。
            if !self.bridge.has_active_question().await {
                return Ok(());
            }
            // ans_q:<question_id>:<option_index>
            let parts: Vec<&str> = q_info.splitn(2, ':').collect();
            if parts.len() == 2 {
                let q_id = parts[0];
                let opt_idx: usize = parts[1].parse().unwrap_or(0);
                
                let answer_label = self.bridge.get_question_option_label(q_id, opt_idx).await
                    .unwrap_or_else(|| format!("Option {}", opt_idx + 1));
                if let Err(e) = self.bridge.reply_question(q_id, &answer_label).await {
                    if let Some(ref msg) = cb.message {
                        let _ = self.client.send_message(
                            msg.chat.id,
                            &format!("⚠️ 提交提问应答失败: {}", e),
                            Some(CardBuilder::sessions_actions()),
                            None,
                        ).await;
                    }
                } else if let Some(ref msg) = cb.message {
                    let update_text = CardBuilder::question_answered_card(&answer_label);
                    let _ = self.client.edit_message_text(
                        msg.chat.id,
                        msg.message_id,
                        &update_text,
                        None,
                        Some("Markdown"),
                    ).await;
                }
            }
        } else if let Some(plan_id) = data.strip_prefix("view_diff:") {
            let config = load_config()?;
            let project_dir = resolve_project_dir(&config);
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if let Err(e) = self.send_diff_view(chat_id, &project_dir, Some(plan_id)).await {
                let _ = self.client.send_message(chat_id, &format!("⚠️ 查看代码改动 Diff 失败: {}", e), Some(CardBuilder::quick_actions()), None).await;
            }
        } else if let Some(plan_id) = data.strip_prefix("view_failure_log:") {
            let config = load_config()?;
            let project_dir = resolve_project_dir(&config);
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if let Err(e) = self.send_failure_log_view(chat_id, &project_dir, plan_id).await {
                let _ = self.client.send_message(chat_id, &format!("⚠️ 查看诊断日志失败: {}", e), Some(CardBuilder::quick_actions()), None).await;
            }
        } else if let Some(plan_id) = data.strip_prefix("retry_repair:") {
            let config = load_config()?;
            let project_dir = resolve_project_dir(&config);
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);

            let clean_id = plan_id
                .trim_start_matches(".ai-workflow/plans/")
                .trim_start_matches("plans/")
                .trim_matches('/')
                .to_string();

            if let Some(ref msg) = cb.message {
                let update_text = CardBuilder::self_repair_running_card(&clean_id);
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &update_text,
                    None,
                    Some("Markdown"),
                ).await;
            }

            let repair_prompt = format!(
                "调用 coding 技能，实施 .ai-workflow/plans/{}，重点排查并修复上一轮测试或规范审核未通过的异常问题",
                clean_id
            );

            if let Err(e) = self.bridge.execute_coding_with_prompt(chat_id, project_dir, &clean_id, Some(repair_prompt)).await {
                let _ = self.client.send_message(
                    chat_id,
                    &format!("❌ 启动自愈修复失败: {}", e),
                    Some(CardBuilder::quick_actions()),
                    None,
                ).await;
            }
        } else if let Some(callback) = parse_permission_callback(data) {
            let (request_id, decision) = match callback {
                PermissionCallback::Once(id) => (id, PermissionDecision::Once),
                PermissionCallback::Always(id) => (id, PermissionDecision::Always),
                PermissionCallback::Deny(id) => (id, PermissionDecision::Deny),
            };
            // The runtime owns the result card edit; the router only warns when
            // the agent reply itself failed and the tap can be retried.
            if self
                .bridge
                .decide_permission(&request_id, decision)
                .await
                == PermissionDecisionOutcome::ReplyFailed
            {
                let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
                let _ = self
                    .client
                    .send_message(chat_id, "⚠️ 提交权限决议失败，请稍后重试。", None, None)
                    .await;
            }
        } else if data == "cancel_task" {
            let outcome = self.bridge.cancel_active_task().await;
            if let Some(ref msg) = cb.message {
                let chat_id = msg.chat.id;
                self.pending_prompt_targets.write().await.remove(&chat_id);
                self.pending_prompt_choices.write().await.remove(&chat_id);
                let update_text = match outcome {
                    CancelOutcome::NothingToCancel => CardBuilder::nothing_to_cancel_card(),
                    CancelOutcome::Cancelled => CardBuilder::task_cancelled_card(),
                    CancelOutcome::ForcedCancelled => CardBuilder::task_forced_cancel_card(),
                };
                let _ = self.client.edit_message_text(
                    chat_id,
                    msg.message_id,
                    &update_text,
                    Some(CardBuilder::quick_actions()),
                    Some("Markdown"),
                ).await;
            }
        } else if data == "open_projects" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.handle_projects_command(chat_id, None).await?;
        } else if data == "open_plans" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.handle_plans_command(chat_id).await?;
        } else if let Some(callback) = parse_session_callback(data) {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            let command = match callback {
                SessionCallback::Overview => SessionsCommand::Overview,
                SessionCallback::Detail(identifier) => SessionsCommand::Detail(identifier),
            };
            self.handle_sessions_command(chat_id, command).await?;
        } else if data == "view_ext_session" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.handle_running_command(chat_id).await?;
        } else if data == "refresh_ext_list" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if let Some(ref msg) = cb.message {
                let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                let (text, markup) = if sessions.is_empty() {
                    let hostname = crate::get_hostname();
                    CardBuilder::no_external_session_card(&hostname)
                } else if sessions.len() == 1 {
                    CardBuilder::external_session_card(&sessions[0])
                } else {
                    let (t, m) = CardBuilder::multiple_external_sessions_card(&sessions);
                    (t, Some(m))
                };
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &text,
                    markup,
                    Some("Markdown"),
                ).await;
            } else {
                self.handle_running_command(chat_id).await?;
            }
        } else if let Some(pid_str) = data.strip_prefix("refresh_ext_session:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                if let Some(ref msg) = cb.message {
                    let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                    if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                        let (text, markup) = CardBuilder::external_session_card(&session);
                        let _ = self.client.edit_message_text(
                            msg.chat.id,
                            msg.message_id,
                            &text,
                            markup,
                            Some("Markdown"),
                        ).await;
                    } else {
                        let _ = self.client.edit_message_text(
                            msg.chat.id,
                            msg.message_id,
                            &format!("⚠️ PID `{}` 的外部 OpenCode 会话已退出或结束运行。\n发送 `/sessions` 可重新扫描会话列表。", pid),
                            Some(CardBuilder::sessions_actions()),
                            Some("Markdown"),
                        ).await;
                    }
                }
            }
        } else if let Some(pid_str) = data.strip_prefix("logs_ext_session:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                if let Some(ref msg) = cb.message {
                    let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                    if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                        let (text, markup) = CardBuilder::external_session_logs_card(&session);
                        let _ = self.client.edit_message_text(
                            msg.chat.id,
                            msg.message_id,
                            &text,
                            Some(markup),
                            Some("Markdown"),
                        ).await;
                    }
                }
            }
        } else if let Some(pid_str) = data.strip_prefix("select_ext_session:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                if let Some(ref msg) = cb.message {
                    let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                    if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                        let (text, markup) = CardBuilder::external_session_card(&session);
                        let _ = self.client.edit_message_text(
                            msg.chat.id,
                            msg.message_id,
                            &text,
                            markup,
                            Some("Markdown"),
                        ).await;
                    }
                }
            }
        } else if let Some(pid_str) = data.strip_prefix("diff_ext_session:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
                let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                    self.send_diff_view(chat_id, std::path::Path::new(&session.cwd), None).await?;
                }
            }
        } else if let Some(pid_str) = data.strip_prefix("switch_ext_ws:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
                let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                    let mut config = load_config()?;
                    config.active_workspace = Some(session.project_name.clone());
                    if !config.workspace_directories.contains(&session.cwd) {
                        config.workspace_directories.push(session.cwd.clone());
                    }
                    config.updated_at = chrono::Utc::now().timestamp();
                    save_config(&config)?;

                    let confirm_text = format!(
                        "✅ *已成功将 OneSpace 活跃工作区切换至*：\n\
                        > ├─ 项目名称: `{}`\n\
                        > └─ 物理路径: `{}`\n\n\
                        后续输入的 `/status`、`/diff` 或 `/coding` 将默认作用于该项目。",
                        session.project_name,
                        session.cwd
                    );
                    self.client.send_message(chat_id, &confirm_text, Some(CardBuilder::quick_actions()), Some("Markdown")).await?;
                }
            }
        } else if let Some(pid_str) = data.strip_prefix("start_host:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
                let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                    let mode = super::types::SessionHostingDecisionMode::Manual;
                    if let Err(e) = self.supervisor.start_hosting(chat_id, session, mode).await {
                        self.client.send_message(chat_id, &format!("❌ 开启托管失败: {}", e), Some(CardBuilder::sessions_actions()), None).await?;
                    }
                } else {
                    self.client.send_message(chat_id, &format!("⚠️ PID `{}` 会话已结束或无法被托管。", pid), Some(CardBuilder::sessions_actions()), None).await?;
                }
            }
        } else if let Some(sid) = data.strip_prefix("stop_host:") {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if self.supervisor.stop_hosting(sid).await {
                if let Some(ref msg) = cb.message {
                    let text = format!("🛑 *已退出对会话 `{}` 的完全托管*\n\n会话已恢复为普通外部运行状态。", sid);
                    let _ = self.client.edit_message_text(
                        msg.chat.id,
                        msg.message_id,
                        &text,
                        None,
                        Some("Markdown"),
                    ).await;
                }
            } else {
                self.client.send_message(chat_id, &format!("ℹ️ 会话 `{}` 并未在托管中或已结束。", sid), Some(CardBuilder::sessions_actions()), None).await?;
            }
        } else if let Some(sid) = data.strip_prefix("toggle_host_mode:") {
            if let Some(new_mode) = self.supervisor.toggle_decision_mode(sid).await {
                let mode_str = match new_mode {
                    super::types::SessionHostingDecisionMode::Manual => "🎯 人工决断优先",
                    super::types::SessionHostingDecisionMode::AutoRecommended => "⚡ 自动推荐优先",
                };
                if let Some(ref msg) = cb.message {
                    let notice = format!("🔄 会话 `{}` 的托管决策模式已切换为：*{}*", sid, mode_str);
                    self.client.send_message(msg.chat.id, &notice, Some(CardBuilder::sessions_actions()), Some("Markdown")).await?;
                }
            }
        } else if let Some(ans_payload) = data.strip_prefix("host_ans:") {
            // host_ans:<session_id>:<opt_idx>
            let parts: Vec<&str> = ans_payload.splitn(2, ':').collect();
            if parts.len() == 2 {
                let sid = parts[0];
                let opt_idx: usize = parts[1].parse().unwrap_or(0);
                let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);

                match self.supervisor.handle_answer_question(chat_id, sid, opt_idx).await {
                    Ok(label) => {
                        if let Some(ref msg) = cb.message {
                            let updated_card = format!(
                                "✅ *已决断提问并注入应答*\n\
                                > ├─ 目标会话: `{}`\n\
                                > └─ 采纳方案: `{}`\n\n\
                                ⏳ _外部 OpenCode 正在继续推进执行..._",
                                sid,
                                label
                            );
                            let _ = self.client.edit_message_text(
                                msg.chat.id,
                                msg.message_id,
                                &updated_card,
                                None,
                                Some("Markdown"),
                            ).await;
                        }
                    }
                    Err(e) => {
                        let _ = self.client.send_message(
                            chat_id,
                            &format!("⚠️ 提交选项答复失败: {}", e),
                            Some(CardBuilder::sessions_actions()),
                            None,
                        ).await;
                    }
                }
            }
        } else if let Some(pid_str) = data.strip_prefix("prompt_ext_session:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
                let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                    if let Some(ref sid) = session.session_id {
                        let target = PromptTargetSession {
                            pid: session.pid,
                            session_id: sid.clone(),
                            cwd: session.cwd.clone(),
                            project_name: session.project_name.clone(),
                        };
                        self.pending_prompt_targets.write().await.insert(chat_id, target);

                        let (text, markup) = CardBuilder::prompt_input_request_card(&session);
                        if let Some(ref msg) = cb.message {
                            let _ = self.client.edit_message_text(
                                msg.chat.id,
                                msg.message_id,
                                &text,
                                Some(markup),
                                Some("Markdown"),
                            ).await;
                        } else {
                            let _ = self.client.send_message(
                                chat_id,
                                &text,
                                Some(markup),
                                Some("Markdown"),
                            ).await;
                        }
                    } else {
                        let err_msg = format!("⚠️ PID `{}` 会话尚未分配 Session ID，暂无法注入提示词。", pid);
                        let _ = self.client.send_message(chat_id, &err_msg, Some(CardBuilder::sessions_actions()), None).await;
                    }
                } else {
                    let err_msg = format!("⚠️ PID `{}` 的外部会话已结束或不存在。", pid);
                    let _ = self.client.send_message(chat_id, &err_msg, Some(CardBuilder::sessions_actions()), None).await;
                }
            }
        } else if let Some(index) = parse_prompt_choice_callback(data) {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            let choices = self
                .pending_prompt_choices
                .write()
                .await
                .remove(&chat_id)
                .unwrap_or_default();

            if let Some(choice) = choices.into_iter().nth(index) {
                let target = match prompt_target_for(&choice.session) {
                    Ok(target) => target,
                    Err(message) => {
                        self.client.send_message(chat_id, &message, Some(CardBuilder::sessions_actions()), None).await?;
                        return Ok(());
                    }
                };

                if choice.content.is_empty() {
                    self.pending_prompt_targets.write().await.insert(chat_id, target);
                    let (text, markup) = CardBuilder::prompt_input_request_card(&choice.session);
                    if let Some(ref msg) = cb.message {
                        let _ = self.client.edit_message_text(
                            msg.chat.id,
                            msg.message_id,
                            &text,
                            Some(markup),
                            Some("Markdown"),
                        ).await;
                    } else {
                        let _ = self.client.send_message(
                            chat_id,
                            &text,
                            Some(markup),
                            Some("Markdown"),
                        ).await;
                    }
                } else {
                    if let Some(ref msg) = cb.message {
                        let selected_text = format!(
                            "✅ *已选择目标会话*：`{}` (PID: {})\n\n⏳ _正在投递提示词..._",
                            choice.session.project_name,
                            choice.session.pid
                        );
                        let _ = self.client.edit_message_text(
                            msg.chat.id,
                            msg.message_id,
                            &selected_text,
                            None,
                            Some("Markdown"),
                        ).await;
                    }
                    self.dispatch_prompt_to_session(chat_id, target, &choice.content).await?;
                }
            } else {
                let expired_text =
                    "⚠️ *目标选择已失效或超出范围*\n\n请重新发送 `/prompt` 并选择目标会话。";
                if let Some(ref msg) = cb.message {
                    let _ = self.client.edit_message_text(
                        msg.chat.id,
                        msg.message_id,
                        expired_text,
                        Some(CardBuilder::sessions_actions()),
                        Some("Markdown"),
                    ).await;
                } else {
                    let _ = self.client.send_message(chat_id, expired_text, Some(CardBuilder::sessions_actions()), Some("Markdown")).await;
                }
            }
        } else if data == "cancel_prompt_choice" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.pending_prompt_choices.write().await.remove(&chat_id);
            let cancel_text = "❎ *已取消选择目标会话*\n\n您可以重新发送 `/prompt` 选择目标会话。";
            if let Some(ref msg) = cb.message {
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    cancel_text,
                    Some(CardBuilder::sessions_actions()),
                    Some("Markdown"),
                ).await;
            } else {
                let _ = self.client
                    .send_message(
                        chat_id,
                        cancel_text,
                        Some(CardBuilder::sessions_actions()),
                        Some("Markdown"),
                    )
                    .await;
            }
        } else if data == "cancel_prompt_target" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.pending_prompt_targets.write().await.remove(&chat_id);
            if let Some(ref msg) = cb.message {
                let cancel_text = "❎ *已取消向会话发送提示词*\n\n您可以随时发送 `/sessions` 查看会话进度或重新发起交互。";
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    cancel_text,
                    Some(CardBuilder::sessions_actions()),
                    Some("Markdown"),
                ).await;
            }
        } else if data == "open_help" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            let help_text = CardBuilder::help_card();
            let _ = self
                .client
                .send_message(
                    chat_id,
                    &help_text,
                    Some(CardBuilder::quick_actions()),
                    Some("Markdown"),
                )
                .await;
        } else if data == "open_status" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.handle_status_command(chat_id).await?;
        }

        Ok(())
    }
}

/// 工作空间项目候选项
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectItem {
    pub name: String,
    pub path: PathBuf,
}

/// 收集系统中所有的可用工作空间（OneSpace 系统 Workspaces + 自定义路径 + 当前目录）
pub fn collect_all_workspaces(config: &super::types::RemoteBotConfig) -> Vec<ProjectItem> {
    let mut items: Vec<ProjectItem> = Vec::new();
    let mut seen_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    // 1. OneSpace 系统工作空间
    for w in crate::workspaces::workspace_records() {
        let p = PathBuf::from(super::commands::resolve_executable_path(&w.root_path));
        if p.is_dir() {
            let canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
            if seen_paths.insert(canon.clone()) {
                let name = if w.name.trim().is_empty() {
                    canon.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "workspace".to_string())
                } else {
                    w.name
                };
                items.push(ProjectItem { name, path: canon });
            }
        }
    }

    // 2. 自定义路径 (workspace_directories)
    for dir_str in &config.workspace_directories {
        let p = PathBuf::from(super::commands::resolve_executable_path(dir_str));
        if p.is_dir() {
            let canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
            if seen_paths.insert(canon.clone()) {
                let name = canon.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| dir_str.clone());
                items.push(ProjectItem { name, path: canon });
            }
        }
    }

    // 3. 当前运行环境所在目录
    let mut cur = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cur.ends_with("src-tauri") {
        if let Some(parent) = cur.parent() {
            cur = parent.to_path_buf();
        }
    }
    let canon_cur = std::fs::canonicalize(&cur).unwrap_or(cur);
    if seen_paths.insert(canon_cur.clone()) {
        let name = canon_cur.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "onespace-app".to_string());
        items.push(ProjectItem { name, path: canon_cur });
    }

    items
}

/// 收集 Telegram 手机端精选常用项目（优先使用 config.workspace_directories，上限 6~8 个以防刷屏）
pub fn collect_featured_projects(config: &super::types::RemoteBotConfig) -> Vec<ProjectItem> {
    let all = collect_all_workspaces(config);
    let mut featured: Vec<ProjectItem> = Vec::new();
    let mut seen_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    // 如果 config.workspace_directories 有配置，按配置挑选
    for dir_str in &config.workspace_directories {
        let p = PathBuf::from(super::commands::resolve_executable_path(dir_str));
        let canon = std::fs::canonicalize(&p).unwrap_or(p);
        if let Some(item) = all.iter().find(|it| it.path == canon || it.name == *dir_str) {
            if seen_paths.insert(item.path.clone()) {
                featured.push(item.clone());
            }
        }
    }

    // 保证当前活跃项目若有效，优先置入精选列表首位
    let current_active = resolve_project_dir(config);
    if let Some(item) = all.iter().find(|it| it.path == current_active) {
        if seen_paths.insert(item.path.clone()) {
            featured.insert(0, item.clone());
        }
    }

    // 若精选列表依然过少（< 3），从 all 中补充至最多 6 个（避免过多）
    for it in &all {
        if featured.len() >= 6 {
            break;
        }
        if seen_paths.insert(it.path.clone()) {
            featured.push(it.clone());
        }
    }

    featured
}

/// 解析当前目标工作区真实物理路径
pub fn resolve_project_dir(config: &super::types::RemoteBotConfig) -> PathBuf {
    let all = collect_all_workspaces(config);

    if let Some(ref ws) = config.active_workspace {
        if !ws.is_empty() {
            // 1. 如果是绝对路径且是有效目录
            let p = PathBuf::from(super::commands::resolve_executable_path(ws));
            if p.is_dir() {
                return std::fs::canonicalize(&p).unwrap_or(p);
            }

            // 2. 匹配已录入项目的名称或文件夹名
            let ws_lower = ws.to_lowercase();
            if let Some(item) = all.iter().find(|it| {
                it.name.to_lowercase() == ws_lower
                    || it.path.file_name().map(|n| n.to_string_lossy().to_lowercase()) == Some(ws_lower.clone())
            }) {
                return item.path.clone();
            }
        }
    }

    // 3. 回退到精选列表第一个
    if let Some(first_featured) = config.workspace_directories.first() {
        let p = PathBuf::from(super::commands::resolve_executable_path(first_featured));
        if p.is_dir() {
            return std::fs::canonicalize(&p).unwrap_or(p);
        }
    }

    // 4. 回退到 all 中的首个
    if let Some(first) = all.first() {
        return first.path.clone();
    }

    // 5. 兜底当前目录
    let mut cur = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cur.ends_with("src-tauri") {
        if let Some(parent) = cur.parent() {
            cur = parent.to_path_buf();
        }
    }
    cur
}
