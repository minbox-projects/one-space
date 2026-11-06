use std::path::PathBuf;
use std::sync::Arc;
use log::{info, warn};
use tokio::process::Command;

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
                您已通过鉴权，随时可以下发指令或直接对话交流：\n\n\
                📌 *基础运维*\n\
                • `/status` - 查看 Mac 状态、Git 分支与最新提交\n\
                • `/projects` - 查看和选择工作区项目\n\n\
                🚀 *AI 研发流水线*\n\
                • `/newfeat <需求内容>` - 启动新需求规划与 OpenCode 编码\n\
                • `/plans` - 查看工作区已有规划方案列表及状态\n\
                • `/progress [planId]` - 查看指定或当前正在实施方案的进度\n\
                • `/diff [planId]` - 查看最新代码改动对比或方案差异\n\
                • `/coding <planId>` - 直接实施指定的已规划方案\n\
                • `/cancel` - 中断当前正在执行的任务\n\n\
                💡 *随时自然对话*：在空闲状态下直接发送文字，机器人将基于当前代码库直接解答与思考！";
            self.client.send_message(chat_id, help_text, None, Some("Markdown")).await?;
        } else if text == "/status" {
            self.handle_status_command(chat_id).await?;
        } else if text == "/projects" {
            self.handle_projects_command(chat_id).await?;
        } else if text == "/plans" {
            self.handle_plans_command(chat_id).await?;
        } else if text.starts_with("/progress") {
            self.handle_progress_command(chat_id, text).await?;
        } else if text.starts_with("/diff") {
            self.handle_diff_command(chat_id, text).await?;
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
                // 空闲状态下，直接作为自然语言对话/需求接入！随时随地与 OpenCode 交流
                let project_dir = resolve_project_dir(&config);
                let project_name = project_dir.file_name().unwrap_or_default().to_string_lossy();
                let ack_text = format!(
                    "💬 收到消息，正在基于工作区 `{}` 思考与分析...\n\n_发送 `/help` 可查看快捷指令，发送 `/cancel` 可随时中断。_",
                    project_name
                );
                self.client.send_message(chat_id, &ack_text, None, Some("Markdown")).await?;

                if let Err(e) = self.bridge.execute_planning(chat_id, project_dir, text).await {
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

        let status_text = format!(
            "🖥️ *OneSpace 工作台运行状态*\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            • 主机设备: `{}`\n\
            • 系统时间: `{}`\n\
            • 活跃工作区: `{}`\n\
            • Git 分支: `{}` ({})\n\
            • 最新提交: `{}`\n\
            • 权限自动审批: `{}`\n\
            • 通信延迟: 正常 (长轮询在线)\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            输入 `/diff` 可查看代码变更，`/plans` 可查看方案。",
            hostname,
            current_time,
            config.active_workspace.as_deref().unwrap_or("onespace-app"),
            git_branch,
            git_status,
            git_latest_commit,
            if config.auto_approve_permissions { "已启用" } else { "手动确认" }
        );

        self.client.send_message(chat_id, &status_text, None, Some("Markdown")).await?;
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
        let diff_msg = format!(
            "🔍 【代码改动 Diff 预览】\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            📋 方案/范围: `{}`\n\
            🎯 比较基准: `{}`\n\n\
            📊 *文件改动统计*:\n\
            ```\n\
            {}\n\
            ```\n\n\
            📝 *代码变更摘要*:\n\
            ```diff\n\
            {}\n\
            ```\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            输入 `/plans` 可继续查看其他方案。",
            title_plan,
            target_scope,
            stat_text,
            snippet
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

        let msg = format!(
            "📄 【方案诊断日志】\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            📋 方案: `{}`\n\n\
            {}\n\
            ━━━━━━━━━━━━━━━━━━━━",
            clean_id,
            body
        );

        self.client.send_message(chat_id, &msg, None, None).await?;
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
        msg_lines.push("点击下方按钮即可查看进度或启动 Coding 实施：\n".to_string());

        // 展示最新的最多 6 个，每个方案附带实施状态图标与操作按钮
        for (idx, (plan_id, _mtime)) in available_plans.iter().take(6).enumerate() {
            let num = idx + 1;
            let status_badge = if let Ok(info) = super::plan_progress::get_plan_progress(&project_dir, plan_id) {
                match info.status {
                    super::plan_progress::PlanExecutionStatus::Completed => "🟢 已交付",
                    super::plan_progress::PlanExecutionStatus::InProgress => "🟡 实施中",
                    super::plan_progress::PlanExecutionStatus::NotStarted => "⚪ 未实施",
                    super::plan_progress::PlanExecutionStatus::Failed => "🔴 中断",
                }
            } else {
                "⚪ 未实施"
            };

            msg_lines.push(format!("{}. `{}` [{}]", num, plan_id, status_badge));

            let short_id = if plan_id.len() > 18 { &plan_id[..18] } else { plan_id };
            let mut row = Vec::new();
            if status_badge.contains("未实施") {
                row.push(InlineKeyboardButton::callback(
                    format!("🚀 实施: {}", short_id),
                    format!("approve_plan:{}", plan_id),
                ));
            }
            row.push(InlineKeyboardButton::callback(
                format!("📊 进度: {}", short_id),
                format!("check_progress:{}", plan_id),
            ));
            keyboard_rows.push(row);
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
                self.client.send_message(
                    chat_id,
                    "📂 当前工作区未检测到任何方案，亦无正在实施中的任务。\n发送 `/help` 查看可用操作指令。",
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
                let _ = self.client.edit_message_text(
                    msg.chat.id,
                    msg.message_id,
                    &format!("🔄 正在携带上一轮异常启动自愈修复: `{}`...", clean_id),
                    None,
                    None,
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
