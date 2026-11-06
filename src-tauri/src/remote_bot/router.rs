use std::path::PathBuf;
use std::sync::Arc;
use log::{info, warn};

use super::auth::AuthGuard;
use super::client::TelegramClient;
use super::opencode_bridge::OpenCodeBridge;
use super::storage::{load_config, save_config};
use super::types::{InlineKeyboardButton, InlineKeyboardMarkup, TelegramCallbackQuery, TelegramMessage, TelegramUpdate};

pub struct CommandRouter {
    client: Arc<TelegramClient>,
    bridge: Arc<OpenCodeBridge>,
}

impl CommandRouter {
    pub fn new(client: Arc<TelegramClient>, bridge: Arc<OpenCodeBridge>) -> Self {
        Self { client, bridge }
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
                let welcome = format!(
                    "🎉 *OneSpace 远程控制已成功绑定！*\n\n\
                    您的 Chat ID `{}` 已登记为授权管理员。\n\n\
                    支持的基础指令：\n\
                    • `/status` - 查看 Mac 本地工作台状态\n\
                    • `/projects` - 切换或查看工作区项目\n\
                    • `/newfeat <需求>` - 启动需求规划与实施\n\
                    • `/help` - 查看帮助文档",
                    chat_id
                );
                self.client.send_message(chat_id, &welcome, None, Some("Markdown")).await?;
                return Ok(());
            }

            warn!("拒绝未授权的 Telegram 访问请求，Chat ID: {}", chat_id);
            let reject_text = format!(
                "⛔ *未授权的访问*\n\n\
                您的 Chat ID 为 `{}`。\n\
                请在 Mac 端的 OneSpace -> 设置 -> 远程控制 中将此 ID 加入白名单。",
                chat_id
            );
            self.client.send_message(chat_id, &reject_text, None, Some("Markdown")).await?;
            return Ok(());
        }

        // 授权用户的命令路由
        if text.starts_with("/start") || text.starts_with("/help") {
            let help_text = 
                "🤖 *OneSpace 移动智能工作台*\n\n\
                您已通过鉴权，随时可以下发指令：\n\n\
                📌 *基础运维*\n\
                • `/status` - 查看 Mac 状态与 AI 网关情况\n\
                • `/projects` - 查看和选择工作区项目\n\n\
                🚀 *AI 研发流水线*\n\
                • `/newfeat <需求内容>` - 启动新需求规划与 OpenCode 编码\n\
                • `/plans` - 查看工作区已有规划方案列表并直接实施\n\
                • `/coding <planId>` - 直接实施指定的已规划方案\n\
                • `/cancel` - 中断当前正在执行的任务";
            self.client.send_message(chat_id, help_text, None, Some("Markdown")).await?;
        } else if text == "/status" {
            self.handle_status_command(chat_id).await?;
        } else if text == "/projects" {
            self.handle_projects_command(chat_id).await?;
        } else if text == "/plans" {
            self.handle_plans_command(chat_id).await?;
        } else if text == "/cancel" {
            self.bridge.cancel_active_task().await;
            self.client.send_message(chat_id, "🛑 当前进行中的任务与多轮规划会话已取消。", None, Some("Markdown")).await?;
        } else if text.starts_with("/coding") || text.starts_with("/implement") {
            self.handle_coding_command(chat_id, text).await?;
        } else if text.starts_with("/newfeat") {
            let prompt = text.trim_start_matches("/newfeat").trim();
            if prompt.is_empty() {
                self.client.send_message(
                    chat_id,
                    "⚠️ 请在 `/newfeat` 后附带需求描述，例如：\n`/newfeat 为项目增加 Telegram 登录认证`",
                    None,
                    Some("Markdown")
                ).await?;
            } else {
                let project_dir = resolve_project_dir(&config);

                let ack_text = format!(
                    "🚀 *收到新需求任务*\n\n\
                    📝 描述: {}\n\
                    📂 目标项目: `{}`\n\n\
                    _正在拉起 OpenCode 执行方案规划 (Planning)..._",
                    prompt,
                    project_dir.display()
                );
                self.client.send_message(chat_id, &ack_text, None, Some("Markdown")).await?;

                if let Err(e) = self.bridge.execute_planning(chat_id, project_dir, prompt).await {
                    self.client.send_message(
                        chat_id,
                        &format!("❌ 启动规划任务失败: `{}`", e),
                        None,
                        Some("Markdown")
                    ).await?;
                }
            }
        } else {
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
                        &format!("❌ 推进规划失败: {}", e),
                        None,
                        None,
                    ).await?;
                }
            } else {
                let reply = format!("💡 收到消息: {}\n输入 `/help` 查看可用操作指令。", text);
                self.client.send_message(chat_id, &reply, None, Some("Markdown")).await?;
            }
        }

        Ok(())
    }

    async fn handle_status_command(&self, chat_id: i64) -> Result<(), String> {
        let config = load_config()?;
        let hostname = crate::get_hostname();
        let current_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let status_text = format!(
            "🖥️ *OneSpace 工作台运行状态*\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            • 主机设备: `{}`\n\
            • 系统时间: `{}`\n\
            • 当前活跃项目: `{}`\n\
            • 权限自动审批: `{}`\n\
            • 通信延迟: 正常 (长轮询在线)\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            输入 `/projects` 可切换项目，输入 `/newfeat` 可派发任务。",
            hostname,
            current_time,
            config.active_workspace.as_deref().unwrap_or("未指定"),
            if config.auto_approve_permissions { "已启用" } else { "手动确认" }
        );

        self.client.send_message(chat_id, &status_text, None, Some("Markdown")).await?;
        Ok(())
    }

    async fn handle_projects_command(&self, chat_id: i64) -> Result<(), String> {
        let config = load_config()?;
        let active = config.active_workspace.as_deref().unwrap_or("未设定");
        
        let mut keyboard_rows = Vec::new();
        // 尝试获取当前 app_dir 或项目目录
        let default_project = "onespace-app";
        let row = vec![
            InlineKeyboardButton::callback(
                format!("📁 {} (当前默认)", default_project),
                format!("set_project:{}", default_project),
            )
        ];
        keyboard_rows.push(row);

        let markup = InlineKeyboardMarkup::new(keyboard_rows);
        let text = format!(
            "📂 *工作区项目管理*\n\n\
            当前绑定的活跃项目: `{}`\n\
            点击下方按钮可切换操作的目标项目：",
            active
        );

        self.client.send_message(chat_id, &text, Some(markup), Some("Markdown")).await?;
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
                "📂 当前工作区的 `.ai-workflow/plans/` 下未检测到任何方案。\n输入 `/newfeat <需求描述>` 可开始新方案规划。",
                None,
                None,
            ).await?;
            return Ok(());
        }

        // 按修改时间降序排序，最新的排在最前
        available_plans.sort_by(|a, b| b.1.cmp(&a.1));

        let mut keyboard_rows = Vec::new();
        let mut msg_lines = Vec::new();
        msg_lines.push("📋 【工作区方案列表 (Plans)】".to_string());
        msg_lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
        msg_lines.push("点击下方按钮即可直接为指定方案启动 Coding 实施，或使用指令 `/coding <planId>`：\n".to_string());

        // 展示最新的最多 8 个
        for (idx, (plan_id, _mtime)) in available_plans.iter().take(8).enumerate() {
            let num = idx + 1;
            msg_lines.push(format!("{}. `{}`", num, plan_id));
            keyboard_rows.push(vec![
                InlineKeyboardButton::callback(
                    format!("🚀 实施: {}", plan_id),
                    format!("approve_plan:{}", plan_id),
                )
            ]);
        }

        let markup = InlineKeyboardMarkup::new(keyboard_rows);
        self.client.send_message(
            chat_id,
            &msg_lines.join("\n"),
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
            self.client.send_message(
                chat_id,
                "⚠️ 请指定要实施的方案标识 (planId)，例如：\n`/coding 20261106-settings-service-categories`\n\n或输入 `/plans` 查看当前可选方案列表。",
                None,
                None,
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

        let ack_text = format!(
            "🚀 确认启动指定方案的 Coding 实施\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            📋 方案: {}\n\
            📂 目标项目: {}\n\n\
            正在拉起全新独立会话启动 Coding 实施...",
            clean_plan_id,
            project_dir.display()
        );
        self.client.send_message(chat_id, &ack_text, None, None).await?;

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
            config.active_workspace = Some(project.to_string());
            config.updated_at = chrono::Utc::now().timestamp();
            save_config(&config)?;

            if let Some(msg) = cb.message {
                let update_text = format!("✅ 已成功将当前活跃项目切换为: `{}`", project);
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
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    "⏳ 方案已确认，正在拉起全新独立会话启动 Coding 实施...",
                    None,
                    None,
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
                let prompt_text = format!(
                    "✏️ 【方案补充】\n\
                    ━━━━━━━━━━━━━━━━━━━━\n\
                    目标方案: {}\n\n\
                    请直接打字回复您希望补充或修改的内容，OpenCode 将据此继续完善规划方案。",
                    plan_tag
                );
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &prompt_text,
                    None,
                    None,
                ).await;
            }
        } else if let Some(choice_num_str) = data.strip_prefix("ans_choice:") {
            let num: usize = choice_num_str.parse().unwrap_or(0);
            let choice_text = self.bridge.get_option_text(num).await
                .unwrap_or_else(|| format!("选项 {}", num));

            let chat_id = cb.message.as_ref().map(|m| m.chat.id).unwrap_or(0);
            if let Some(ref msg) = cb.message {
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &format!("✅ 您已选择: {}", choice_text),
                    None,
                    None,
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
                    let _ = self.client.edit_message_text(
                        msg.chat.id,
                        msg.message_id,
                        &format!("✅ 已选择并提交决策: {}", answer_label),
                        None,
                        None,
                    ).await;
                }
            }
        } else if data == "cancel_task" {
            self.bridge.cancel_active_task().await;
            if let Some(ref msg) = cb.message {
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    "🛑 任务已取消。",
                    None,
                    Some("Markdown"),
                ).await;
            }
        }

        Ok(())
    }
}

/// 解析当前目标工作区真实物理路径
pub fn resolve_project_dir(config: &super::types::RemoteBotConfig) -> PathBuf {
    if let Some(ref ws) = config.active_workspace {
        if !ws.is_empty() && ws != "onespace-app" {
            let p = PathBuf::from(ws);
            if p.exists() {
                return p;
            }
        }
    }

    let mut cur = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cur.ends_with("src-tauri") {
        if let Some(parent) = cur.parent() {
            cur = parent.to_path_buf();
        }
    }
    cur
}
