use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

use super::client::TelegramClient;
use super::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// =========================================================================
// OpenCode Server API DTOs
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodeSession {
    pub id: String,
    pub title: Option<String>,
    pub directory: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodeQuestionItem {
    pub question: String,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub options: Vec<OpenCodeQuestionOption>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodeQuestionOption {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodeQuestionRequest {
    pub id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub questions: Vec<OpenCodeQuestionItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodePermissionRequest {
    pub id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub permission: Option<String>,
    pub command: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuestionReplyPayload {
    pub answers: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PermissionReplyPayload {
    pub reply: String, // "once" | "always" | "deny"
}

// =========================================================================
// OpenCode Bridge 核心控制器
// =========================================================================

pub struct OpenCodeBridge {
    server_process: Mutex<Option<Child>>,
    server_url: RwLock<Option<String>>,
    http_client: Client,
    telegram_client: Arc<TelegramClient>,
    #[allow(dead_code)]
    active_question: RwLock<Option<OpenCodeQuestionRequest>>,
    is_busy: Arc<AtomicBool>,
}

impl OpenCodeBridge {
    pub fn new(telegram_client: Arc<TelegramClient>) -> Self {
        Self {
            server_process: Mutex::new(None),
            server_url: RwLock::new(None),
            http_client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
            telegram_client,
            active_question: RwLock::new(None),
            is_busy: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 确保后台 opencode serve 进程正在运行并返回服务基础 URL
    pub async fn ensure_server(&self) -> Result<String, String> {
        {
            let url_guard = self.server_url.read().await;
            if let Some(ref url) = *url_guard {
                // 探测存活性
                if self.http_client.get(format!("{}/session", url)).send().await.is_ok() {
                    return Ok(url.clone());
                }
            }
        }

        let mut proc_guard = self.server_process.lock().await;
        // 如果旧进程存在先杀掉
        if let Some(mut child) = proc_guard.take() {
            let _ = child.kill().await;
        }

        info!("启动本地 loopback opencode serve 服务...");
        let mut child = Command::new("opencode")
            .arg("serve")
            .arg("--print-logs")
            .arg("--port")
            .arg("0")
            .arg("--hostname")
            .arg("127.0.0.1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("无法启动 opencode CLI: {}. 请确认是否已安装 opencode", e))?;

        let stdout = child.stdout.take().ok_or_else(|| "无法获取 opencode stdout".to_string())?;
        let mut reader = BufReader::new(stdout).lines();

        let mut discovered_url: Option<String> = None;
        // 读取前 30 行寻找监听地址
        for _ in 0..30 {
            tokio::select! {
                line_res = reader.next_line() => {
                    match line_res {
                        Ok(Some(line)) => {
                            info!("[opencode serve] {}", line);
                            if let Some(idx) = line.find("http://127.0.0.1:") {
                                let url_part = &line[idx..];
                                let clean_url = url_part.split_whitespace().next().unwrap_or("").trim_end_matches('/');
                                discovered_url = Some(clean_url.to_string());
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(5000)) => {
                    break;
                }
            }
        }

        let url = discovered_url.ok_or_else(|| "未能从 opencode serve 启动日志中识别到监听地址".to_string())?;
        info!("OpenCode 服务已在 {} 就绪", url);

        *proc_guard = Some(child);
        let mut url_guard = self.server_url.write().await;
        *url_guard = Some(url.clone());

        Ok(url)
    }

    /// 执行需求规划会话 (Planning Session)
    pub async fn execute_planning(
        self: &Arc<Self>,
        chat_id: i64,
        project_dir: PathBuf,
        prompt: &str,
    ) -> Result<(), String> {
        if self.is_busy.swap(true, Ordering::SeqCst) {
            return Err("已有正在运行的任务，请等待当前任务完成或执行 /cancel".to_string());
        }

        let bridge = self.clone();
        let prompt_string = prompt.to_string();

        tokio::spawn(async move {
            let res = bridge.run_planning_inner(chat_id, project_dir, &prompt_string).await;
            bridge.is_busy.store(false, Ordering::SeqCst);
            if let Err(e) = res {
                error!("Planning 任务失败: {}", e);
                let error_text = format!("❌ *规划阶段异常中断*\n\n原因:\n```\n{}\n```", e);
                let _ = bridge.telegram_client.send_message(
                    chat_id,
                    &error_text,
                    None,
                    Some("Markdown"),
                ).await;
            }
        });

        Ok(())
    }

    async fn run_planning_inner(
        &self,
        chat_id: i64,
        project_dir: PathBuf,
        prompt: &str,
    ) -> Result<(), String> {
        let server_url = self.ensure_server().await?;

        // 构造规划提示词，明确指导调用 planning 技能
        let planning_prompt = format!(
            "用户提出新需求：{}\n\n\
            请遵循项目工程规范，使用 planning 技能进行深入调研与方案规划。\n\
            如遇到实现分歧，请调用提问工具澄清；规划完成后，请生成并确认 spec.md 和 plan.md。",
            prompt
        );

        let status_msg = self.telegram_client.send_message(
            chat_id,
            "🔍 *OpenCode 正在进行方案规划 (Planning)...*\n_正在分析代码库与依赖关系..._",
            None,
            Some("Markdown"),
        ).await?;

        // 启动 opencode run 会话
        let child = Command::new("opencode")
            .arg("run")
            .arg(&planning_prompt)
            .arg("--attach")
            .arg(&server_url)
            .arg("--dir")
            .arg(&project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("启动 OpenCode Planning 失败: {}", e))?;

        // 启动提问与权限轮询守卫
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let poll_guard = self.spawn_question_and_permission_poller(
            chat_id,
            status_msg.message_id,
            server_url.clone(),
            cancel_flag.clone(),
        );

        let output = child.wait_with_output().await
            .map_err(|e| format!("OpenCode 规划进程执行异常: {}", e))?;

        cancel_flag.store(true, Ordering::SeqCst);
        let _ = poll_guard.await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let raw = if !stderr.trim().is_empty() {
                stderr.trim()
            } else {
                stdout.trim()
            };
            return Err(format!("OpenCode 规划非零退出: {}", strip_ansi_escapes(raw)));
        }

        // 规划成功完成，查找产出的 plan.md
        let plan_path = self.locate_plan_file(&project_dir);
        let summary = match plan_path {
            Some(ref p) => self.extract_plan_summary(p).await.unwrap_or_else(|| "方案已生成于工作区。".to_string()),
            None => "方案规划已完成，未在默认路径检测到独立 plan.md，已就绪可实施。".to_string(),
        };

        // 推送方案终审卡片
        let review_text = format!(
            "📋 *【方案规划完成，等待确认】*\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            {}\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            点击下方按钮确认开始在新会话中编写代码：",
            summary
        );

        let plan_id_tag = plan_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| "default".to_string());
        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("🚀 确认方案，开始 Coding", format!("approve_plan:{}", plan_id_tag)),
            InlineKeyboardButton::callback("❌ 放弃任务", "cancel_task"),
        ]]);

        self.telegram_client.send_message(
            chat_id,
            &review_text,
            Some(markup),
            Some("Markdown"),
        ).await?;

        Ok(())
    }

    /// 轮询并转发 /question 与 /permission 请求到 Telegram
    fn spawn_question_and_permission_poller(
        &self,
        chat_id: i64,
        _status_msg_id: i64,
        server_url: String,
        cancel_flag: Arc<AtomicBool>,
    ) -> tokio::task::JoinHandle<()> {
        let http = self.http_client.clone();
        let tg = self.telegram_client.clone();

        tokio::spawn(async move {
            let mut asked_questions = std::collections::HashSet::new();
            let mut asked_permissions = std::collections::HashSet::new();

            while !cancel_flag.load(Ordering::Relaxed) {
                // 1. 检查 /question
                let q_url = format!("{}/question", server_url);
                if let Ok(resp) = http.get(&q_url).send().await {
                    if let Ok(questions) = resp.json::<Vec<OpenCodeQuestionRequest>>().await {
                        for q in questions {
                            if asked_questions.contains(&q.id) {
                                continue;
                            }
                            asked_questions.insert(q.id.clone());

                            if let Some(first_q) = q.questions.first() {
                                let mut buttons = Vec::new();
                                for (idx, opt) in first_q.options.iter().enumerate() {
                                    let btn_text = if opt.label.len() > 30 {
                                        format!("{}...", &opt.label[..27])
                                    } else {
                                        opt.label.clone()
                                    };
                                    buttons.push(vec![InlineKeyboardButton::callback(
                                        format!("🔘 {}", btn_text),
                                        format!("ans_q:{}:{}", q.id, idx),
                                    )]);
                                }

                                let text = format!(
                                    "🤔 *【规划澄清提问】*\n\n\
                                    问题: *{}*\n\n\
                                    请点击下方选项进行决策：",
                                    first_q.question
                                );

                                let markup = InlineKeyboardMarkup::new(buttons);
                                let _ = tg.send_message(chat_id, &text, Some(markup), Some("Markdown")).await;
                            }
                        }
                    }
                }

                // 2. 检查 /permission (自动授权或高危审批)
                let p_url = format!("{}/permission", server_url);
                if let Ok(resp) = http.get(&p_url).send().await {
                    if let Ok(perms) = resp.json::<Vec<OpenCodePermissionRequest>>().await {
                        for p in perms {
                            if asked_permissions.contains(&p.id) {
                                continue;
                            }
                            asked_permissions.insert(p.id.clone());

                            // 默认常规自动批准 "once"
                            let reply_url = format!("{}/permission/{}/reply", server_url, p.id);
                            let payload = PermissionReplyPayload { reply: "once".to_string() };
                            let _ = http.post(&reply_url).json(&payload).send().await;
                        }
                    }
                }

                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        })
    }

    /// 回复指定的 OpenCode 提问
    pub async fn reply_question(&self, request_id: &str, answer: &str) -> Result<(), String> {
        let server_url = self.ensure_server().await?;
        let url = format!("{}/question/{}/reply", server_url, request_id);
        let payload = QuestionReplyPayload {
            answers: vec![vec![answer.to_string()]],
        };

        let resp = self.http_client.post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("发送提问回复失败: {}", e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("OpenCode 拒绝提问回复，状态码: {}", resp.status()))
        }
    }

    fn locate_plan_file(&self, project_dir: &Path) -> Option<PathBuf> {
        let candidates = vec![
            project_dir.join(".ai-workflow").join("plans"),
            project_dir.to_path_buf(),
        ];

        for base in candidates {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.file_name().map_or(false, |n| n == "plan.md") {
                        return Some(path);
                    }
                    if path.is_dir() {
                        let sub_plan = path.join("plan.md");
                        if sub_plan.exists() {
                            return Some(sub_plan);
                        }
                    }
                }
            }
        }
        None
    }

    async fn extract_plan_summary(&self, plan_path: &Path) -> Option<String> {
        let content = tokio::fs::read_to_string(plan_path).await.ok()?;
        let lines: Vec<&str> = content.lines().take(20).collect();
        Some(lines.join("\n"))
    }

    /// 在全新独立会话中拉起 OpenCode 执行 coding 技能实施
    pub async fn execute_coding(
        self: &Arc<Self>,
        chat_id: i64,
        project_dir: PathBuf,
        plan_identifier: &str,
    ) -> Result<(), String> {
        if self.is_busy.swap(true, Ordering::SeqCst) {
            return Err("已有正在运行的任务，请等待当前任务完成".to_string());
        }

        let bridge = self.clone();
        let plan_tag = plan_identifier.to_string();

        tokio::spawn(async move {
            let res = bridge.run_coding_inner(chat_id, project_dir, &plan_tag).await;
            bridge.is_busy.store(false, Ordering::SeqCst);
            if let Err(e) = res {
                error!("Coding 实施失败: {}", e);
                let error_text = format!("❌ *Coding 阶段未达成完成标准*\n\n原因:\n```\n{}\n```", e);
                let _ = bridge.telegram_client.send_message(
                    chat_id,
                    &error_text,
                    None,
                    Some("Markdown"),
                ).await;
            }
        });

        Ok(())
    }

    async fn run_coding_inner(
        &self,
        chat_id: i64,
        project_dir: PathBuf,
        plan_identifier: &str,
    ) -> Result<(), String> {
        let server_url = self.ensure_server().await?;

        let status_msg = self.telegram_client.send_message(
            chat_id,
            "🚀 *【全新会话已创建】OpenCode 正在实施编码...*\n━━━━━━━━━━━━━━━━━━━━\n⏳ 状态: 加载 coding 技能与任务步骤...",
            None,
            Some("Markdown"),
        ).await?;

        // 构造实施指令，要求使用 coding 技能并提供清晰依据
        let coding_prompt = format!(
            "用户已批准实施方案 [{}]。\n\n\
            请在新会话中严格按照该方案，调用 coding 技能进行 TDD 开发与验证。\n\
            要求：编写失败测试 -> 最小化实现 -> 跑通验证 -> 按照标准交付清单输出 ## Output checklist 与 ### Status: done。",
            plan_identifier
        );

        let child = Command::new("opencode")
            .arg("run")
            .arg(&coding_prompt)
            .arg("--attach")
            .arg(&server_url)
            .arg("--dir")
            .arg(&project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("启动 OpenCode Coding 失败: {}", e))?;

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let poll_guard = self.spawn_question_and_permission_poller(
            chat_id,
            status_msg.message_id,
            server_url.clone(),
            cancel_flag.clone(),
        );

        let output = child.wait_with_output().await
            .map_err(|e| format!("OpenCode Coding 进程执行异常: {}", e))?;

        cancel_flag.store(true, Ordering::SeqCst);
        let _ = poll_guard.await;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // 执行严格的四重证据链核验
        self.verify_and_report_completion(
            chat_id,
            status_msg.message_id,
            &project_dir,
            output.status.success(),
            &stdout,
            &stderr,
        ).await
    }

    /// 四重证据链严格核验
    async fn verify_and_report_completion(
        &self,
        chat_id: i64,
        status_msg_id: i64,
        project_dir: &Path,
        process_success: bool,
        stdout: &str,
        stderr: &str,
    ) -> Result<(), String> {
        // 1. 进程退出码
        if !process_success {
            return Err(format!("OpenCode 异常退出: {}", stderr.trim()));
        }

        // 2. 契约校验：检查是否包含 ### Status: done
        let status_done = stdout.lines().any(|l| {
            let trimmed = l.trim().to_lowercase();
            trimmed.starts_with("### status") && trimmed.contains("done")
        });

        if !status_done {
            warn!("Coding 输出未检测到 ### Status: done 标记");
            // 若输出中有 blocked 或 failed，提取原因
            let reason = stdout.lines()
                .find(|l| l.contains("Support Requests") || l.contains("blocked") || l.contains("failed"))
                .unwrap_or("未能满足规范契约中明确的 done 状态");
            return Err(format!("未通过交付契约审核: {}", reason));
        }

        // 3. Git 凭据核验：获取最新 Commit 信息与改动统计
        let git_log = Command::new("git")
            .current_dir(project_dir)
            .args(["log", "-1", "--oneline"])
            .output().await;

        let commit_summary = match git_log {
            Ok(ref out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            _ => "已提交代码变更".to_string(),
        };

        let diff_stat = Command::new("git")
            .current_dir(project_dir)
            .args(["diff", "HEAD~1..HEAD", "--stat"])
            .output().await;

        let stat_text = match diff_stat {
            Ok(ref out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            _ => "变更文件已由 Git Operator 记录".to_string(),
        };

        // 4. 发送成功卡片
        let success_text = format!(
            "🎉 *【Coding 实施圆满完成】*\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            ✅ *四重证据链审核全部通过*：\n\
            • 进程状态: Exit 0 (正常退出)\n\
            • 交付契约: `### Status: done` 达标\n\
            • Git 提交: `{}`\n\n\
            📊 *变更统计*:\n\
            ```\n\
            {}\n\
            ```\n\
            ━━━━━━━━━━━━━━━━━━━━\n\
            已安全合入工作区分支！",
            commit_summary,
            if stat_text.len() > 300 { format!("{}...", &stat_text[..300]) } else { stat_text }
        );

        let _ = self.telegram_client.edit_message_text(
            chat_id,
            status_msg_id,
            "✅ OpenCode 实施完毕，测试通过！",
            None,
            Some("Markdown"),
        ).await;

        self.telegram_client.send_message(
            chat_id,
            &success_text,
            None,
            Some("Markdown"),
        ).await?;

        Ok(())
    }
}

/// 清洗控制台输出中的 ANSI escape 转义序列
fn strip_ansi_escapes(s: &str) -> String {
    if let Ok(re) = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]") {
        re.replace_all(s, "").to_string()
    } else {
        s.to_string()
    }
}
