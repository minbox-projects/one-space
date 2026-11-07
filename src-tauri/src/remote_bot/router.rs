use std::path::PathBuf;
use std::sync::Arc;
use log::{info, warn};
use tokio::process::Command;

use super::auth::AuthGuard;
use super::card_builder::CardBuilder;
use super::client::TelegramClient;
use super::opencode_bridge::OpenCodeBridge;
use super::storage::{load_config, save_config};
use super::types::{InlineKeyboardButton, InlineKeyboardMarkup, PromptTargetSession, TelegramCallbackQuery, TelegramMessage, TelegramUpdate};

pub struct CommandRouter {
    client: Arc<TelegramClient>,
    bridge: Arc<OpenCodeBridge>,
    pub supervisor: Arc<super::session_supervisor::SessionSupervisor>,
    pending_prompt_targets: tokio::sync::RwLock<std::collections::HashMap<i64, PromptTargetSession>>,
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
                self.client.send_message(chat_id, &welcome, None, Some("Markdown")).await?;
                return Ok(());
            }

            warn!("拒绝未授权的 Telegram 访问请求，Chat ID: {}", chat_id);
            let reject_text = CardBuilder::unauthorized_card(chat_id);
            self.client.send_message(chat_id, &reject_text, None, Some("Markdown")).await?;
            return Ok(());
        }

        // 检查该用户是否正处于“向特定外部会话发送提示词”的等待输入状态
        let pending_target = {
            let mut lock = self.pending_prompt_targets.write().await;
            lock.remove(&chat_id)
        };

        if let Some(target) = pending_target {
            if !text.starts_with('/') {
                return self.dispatch_prompt_to_session(chat_id, target, text).await;
            }
        }

        // 授权用户的命令路由
        if text.starts_with("/start") || text.starts_with("/help") {
            let help_text = CardBuilder::help_card();
            self.client.send_message(chat_id, &help_text, None, Some("Markdown")).await?;
        } else if text == "/running" || text.starts_with("/running") || text == "/sessions" || text.starts_with("/sessions") {
            self.handle_running_command(chat_id).await?;
        } else if text == "/host" || text.starts_with("/host ") || text == "/hosts" {
            self.handle_host_command(chat_id, text).await?;
        } else if text == "/unhost" {
            self.handle_unhost_command(chat_id).await?;
        } else if text.starts_with("/prompt") || text.starts_with("/send") {
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
            self.handle_progress_command(chat_id, text).await?;
        } else if text.starts_with("/diff") {
            self.handle_diff_command(chat_id, text).await?;
        } else if text == "/cancel" {
            self.bridge.cancel_active_task().await;
            let cancel_text = CardBuilder::task_cancelled_card();
            self.client.send_message(chat_id, &cancel_text, None, Some("Markdown")).await?;
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
                    None,
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
                        None,
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
                                self.client.send_message(chat_id, &format!("⚠️ 决断提交失败: {}", e), None, None).await?;
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
                        None,
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
                    self.client.send_message(chat_id, &guard_text, None, Some("Markdown")).await?;
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
                        None,
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
                        self.client.send_message(chat_id, &format!("❌ 启动托管失败: {}", e), None, None).await?;
                    }
                }
                return Ok(());
            } else {
                self.client.send_message(chat_id, &format!("⚠️ 未找到匹配 `{}` 的正在运行会话。\n输入 `/running` 查看当前活跃会话。", arg), None, None).await?;
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
            self.client.send_message(chat_id, &format!("🛑 已成功停止针对 {} 个会话的完全托管守护。", count), None, None).await?;
        } else {
            self.client.send_message(chat_id, "ℹ️ 当前没有处于托管守护状态的会话。", None, None).await?;
        }
        Ok(())
    }

    async fn handle_prompt_command(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let raw_prompt = if let Some(stripped) = text.strip_prefix("/prompt") {
            stripped.trim()
        } else if let Some(stripped) = text.strip_prefix("/send") {
            stripped.trim()
        } else {
            ""
        };

        let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;

        if sessions.is_empty() {
            let hostname = crate::get_hostname();
            let (empty_text, markup) = CardBuilder::no_external_session_card(&hostname);
            self.client.send_message(chat_id, &empty_text, markup, Some("Markdown")).await?;
            return Ok(());
        }

        if sessions.len() == 1 {
            let session = &sessions[0];
            let sid = match session.session_id.clone() {
                Some(s) => s,
                None => {
                    self.client.send_message(chat_id, "⚠️ 运行中的 OpenCode 尚未初始化 session_id，请稍候再试。", None, None).await?;
                    return Ok(());
                }
            };

            let target = PromptTargetSession {
                pid: session.pid,
                session_id: sid,
                cwd: session.cwd.clone(),
                project_name: session.project_name.clone(),
            };

            if raw_prompt.is_empty() {
                self.pending_prompt_targets.write().await.insert(chat_id, target);
                let (req_text, markup) = CardBuilder::prompt_input_request_card(session);
                self.client.send_message(chat_id, &req_text, Some(markup), Some("Markdown")).await?;
            } else {
                self.dispatch_prompt_to_session(chat_id, target, raw_prompt).await?;
            }
        } else {
            // 多个外部会话
            if raw_prompt.is_empty() {
                let (list_text, markup) = CardBuilder::multiple_external_sessions_card(&sessions);
                self.client.send_message(chat_id, &list_text, Some(markup), Some("Markdown")).await?;
            } else {
                let mut prompt_buttons = Vec::new();
                for (idx, sess) in sessions.iter().enumerate() {
                    let label = format!("{} 投递给: {} (PID: {})", CardBuilder::number_badge(idx + 1), sess.project_name, sess.pid);
                    prompt_buttons.push(vec![
                        InlineKeyboardButton::callback(label, format!("prompt_ext_session:{}", sess.pid)),
                    ]);
                }
                let choose_text = format!(
                    "💬 *检测到多个运行中的外部会话*\n\
                    > 准备发送提示词: `{}`\n\n\
                    请点击下方按钮选择目标会话：",
                    raw_prompt
                );
                self.client.send_message(chat_id, &choose_text, Some(InlineKeyboardMarkup::new(prompt_buttons)), Some("Markdown")).await?;
            }
        }

        Ok(())
    }

    async fn dispatch_prompt_to_session(
        &self,
        chat_id: i64,
        target: PromptTargetSession,
        prompt: &str,
    ) -> Result<(), String> {
        let ack = CardBuilder::prompt_dispatched_ack_card(&target, prompt);
        self.client.send_message(chat_id, &ack, None, Some("Markdown")).await?;

        if let Err(e) = self.bridge.execute_prompt_for_session(
            chat_id,
            PathBuf::from(&target.cwd),
            &target.session_id,
            prompt,
        ).await {
            self.client.send_message(chat_id, &format!("❌ 发送提示词失败: {}", e), None, None).await?;
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
            let limit = 1200;
            if raw_diff.len() > limit {
                format!("{}\n\n...（差异较大，已截取前 1200 字符）...", &raw_diff[..limit])
            } else {
                raw_diff
            }
        };

        let title_plan = plan_id_opt.unwrap_or("最新提交对比");
        let diff_msg = CardBuilder::diff_preview_card(
            title_plan,
            &target_scope,
            &stat_text,
            &snippet,
        );

        self.client.send_message(chat_id, &diff_msg, None, Some("Markdown")).await?;
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
                self.client.send_message(chat_id, &confirm_text, None, Some("Markdown")).await?;
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
            self.client.send_message(chat_id, &confirm_text, None, Some("Markdown")).await?;
        } else if matched.len() > 1 {
            // 匹配到多个，最多展示 6 个候选项供点击
            let search_candidates: Vec<(&str, &str)> = matched.iter().take(6).map(|it| {
                (it.name.as_str(), it.name.as_str())
            }).collect();
            let (text, markup) = CardBuilder::projects_search_card(raw_arg, &search_candidates);
            self.client.send_message(chat_id, &text, Some(markup), Some("Markdown")).await?;
        } else {
            let not_found_text = CardBuilder::project_not_found_card(raw_arg);
            self.client.send_message(chat_id, &not_found_text, None, Some("Markdown")).await?;
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
                None,
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

    async fn handle_progress_command(&self, chat_id: i64, text: &str) -> Result<(), String> {
        let config = load_config()?;
        let project_dir = resolve_project_dir(&config);

        let arg = text.trim_start_matches("/progress").trim();
        let target_plan_id = if !arg.is_empty() {
            Some(arg.to_string())
        } else {
            // 自动寻找正在运行的 planId
            super::plan_progress::find_active_plan_id(&project_dir)
                .or_else(|| {
                    // 若无活跃的，取 plans 目录最近更新的一个方案
                    let plans_dir = project_dir.join(".ai-workflow").join("plans");
                    if let Ok(entries) = std::fs::read_dir(&plans_dir) {
                        let mut candidates: Vec<(String, std::time::SystemTime)> = entries.flatten()
                            .filter(|e| e.path().join("plan.md").is_file())
                            .map(|e| {
                                let mtime = e.path().metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                (e.file_name().to_string_lossy().to_string(), mtime)
                            })
                            .collect();
                        candidates.sort_by(|a, b| b.1.cmp(&a.1));
                        candidates.into_iter().next().map(|(id, _)| id)
                    } else {
                        None
                    }
                })
        };

        let plan_id = match target_plan_id {
            Some(id) => id,
            None => {
                // 如果当前工作区无方案，但检测到终端有外部运行中的 OpenCode 会话，则直接展示外部会话进度
                let ext_sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                if !ext_sessions.is_empty() {
                    return self.handle_running_command(chat_id).await;
                }

                self.client.send_message(
                    chat_id,
                    "📂 当前工作区未检测到任何方案，亦无正在实施中的任务。\n发送 `/running` 可探测终端运行中的会话，发送 `/help` 查看说明。",
                    None,
                    None,
                ).await?;
                return Ok(());
            }
        };

        match super::plan_progress::get_plan_progress(&project_dir, &plan_id) {
            Ok(info) => {
                let (msg, markup) = super::plan_progress::format_plan_progress_message(&info);
                self.client.send_message(
                    chat_id,
                    &msg,
                    markup,
                    Some("Markdown"),
                ).await?;
            }
            Err(e) => {
                self.client.send_message(
                    chat_id,
                    &format!("⚠️ 获取方案进度失败: {}", e),
                    None,
                    None,
                ).await?;
            }
        }

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
                None,
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
            self.client.send_message(chat_id, &not_found_msg, None, Some("Markdown")).await?;
            return Ok(());
        }

        let (ack_text, _) = CardBuilder::coding_running_card(&clean_plan_id, false);
        self.client.send_message(chat_id, &ack_text, None, Some("Markdown")).await?;

        if let Err(e) = self.bridge.execute_coding(chat_id, project_dir, &clean_plan_id).await {
            self.client.send_message(chat_id, &format!("❌ 启动 Coding 实施失败: {}", e), None, None).await?;
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
                    None,
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
                    None,
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
                    let (msg, markup) = super::plan_progress::format_plan_progress_message(&info);
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
                            None,
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
                    None,
                    None,
                ).await;
            }
        } else if let Some(q_info) = data.strip_prefix("ans_q:") {
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
                            None,
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
                let _ = self.client.send_message(chat_id, &format!("⚠️ 查看代码改动 Diff 失败: {}", e), None, None).await;
            }
        } else if let Some(plan_id) = data.strip_prefix("view_failure_log:") {
            let config = load_config()?;
            let project_dir = resolve_project_dir(&config);
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if let Err(e) = self.send_failure_log_view(chat_id, &project_dir, plan_id).await {
                let _ = self.client.send_message(chat_id, &format!("⚠️ 查看诊断日志失败: {}", e), None, None).await;
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
                    None,
                    None,
                ).await;
            }
        } else if data == "cancel_task" {
            self.bridge.cancel_active_task().await;
            if let Some(ref msg) = cb.message {
                let update_text = CardBuilder::task_cancelled_card();
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &update_text,
                    None,
                    Some("Markdown"),
                ).await;
            }
        } else if data == "open_projects" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.handle_projects_command(chat_id, None).await?;
        } else if data == "open_plans" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.handle_plans_command(chat_id).await?;
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
                            &format!("⚠️ PID `{}` 的外部 OpenCode 会话已退出或结束运行。\n发送 `/running` 可重新扫描会话列表。", pid),
                            None,
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
                    self.client.send_message(chat_id, &confirm_text, None, Some("Markdown")).await?;
                }
            }
        } else if let Some(pid_str) = data.strip_prefix("start_host:") {
            if let Ok(pid) = pid_str.parse::<u32>() {
                let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
                let sessions = super::opencode_inspector::discover_external_opencode_sessions().await;
                if let Some(session) = sessions.into_iter().find(|s| s.pid == pid) {
                    let mode = super::types::SessionHostingDecisionMode::Manual;
                    if let Err(e) = self.supervisor.start_hosting(chat_id, session, mode).await {
                        self.client.send_message(chat_id, &format!("❌ 开启托管失败: {}", e), None, None).await?;
                    }
                } else {
                    self.client.send_message(chat_id, &format!("⚠️ PID `{}` 会话已结束或无法被托管。", pid), None, None).await?;
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
                self.client.send_message(chat_id, &format!("ℹ️ 会话 `{}` 并未在托管中或已结束。", sid), None, None).await?;
            }
        } else if let Some(sid) = data.strip_prefix("toggle_host_mode:") {
            if let Some(new_mode) = self.supervisor.toggle_decision_mode(sid).await {
                let mode_str = match new_mode {
                    super::types::SessionHostingDecisionMode::Manual => "🎯 人工决断优先",
                    super::types::SessionHostingDecisionMode::AutoRecommended => "⚡ 自动推荐优先",
                };
                if let Some(ref msg) = cb.message {
                    let notice = format!("🔄 会话 `{}` 的托管决策模式已切换为：*{}*", sid, mode_str);
                    self.client.send_message(msg.chat.id, &notice, None, Some("Markdown")).await?;
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
                            None,
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
                        let _ = self.client.send_message(chat_id, &err_msg, None, None).await;
                    }
                } else {
                    let err_msg = format!("⚠️ PID `{}` 的外部会话已结束或不存在。", pid);
                    let _ = self.client.send_message(chat_id, &err_msg, None, None).await;
                }
            }
        } else if data == "cancel_prompt_target" {
            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            self.pending_prompt_targets.write().await.remove(&chat_id);
            if let Some(ref msg) = cb.message {
                let cancel_text = "❎ *已取消向会话发送提示词*\n\n您可以随时发送 `/running` 查看会话进度或重新发起交互。";
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    cancel_text,
                    None,
                    Some("Markdown"),
                ).await;
            }
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
