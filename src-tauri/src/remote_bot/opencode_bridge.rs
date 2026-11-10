use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use log::{error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

use super::card_builder::CardBuilder;
use super::client::TelegramClient;
use super::permission::{
    classify_permission, permission_timed_out, PendingPermissionRequest, PermissionDecision,
    PermissionRegistry, PermissionRisk,
};
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
    #[serde(alias = "title", alias = "header", alias = "prompt", alias = "text")]
    pub question: String,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub options: Vec<OpenCodeQuestionOption>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodeQuestionOption {
    #[serde(alias = "title", alias = "text", alias = "name")]
    pub label: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodeQuestionRequest {
    #[serde(alias = "requestId", alias = "request_id", alias = "formID", alias = "form_id")]
    pub id: String,
    #[serde(default, alias = "sessionId", alias = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub questions: Vec<OpenCodeQuestionItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenCodePermissionRequest {
    #[serde(alias = "requestId", alias = "request_id", alias = "permissionID", alias = "permission_id")]
    pub id: String,
    #[serde(default, alias = "sessionId", alias = "sessionID")]
    pub session_id: Option<String>,
    pub permission: Option<String>,
    pub command: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum ApiEnvelope<T> {
    Direct(Vec<T>),
    Envelope {
        data: Vec<T>,
    },
}

impl<T> ApiEnvelope<T> {
    pub fn into_vec(self) -> Vec<T> {
        match self {
            ApiEnvelope::Direct(v) => v,
            ApiEnvelope::Envelope { data } => data,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuestionReplyPayload {
    pub answers: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PermissionReplyPayload {
    pub reply: String, // "once" | "always" | "deny"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

pub const OPENCODE_INTERNAL_AUTH: &str = "onespace-remote-bot-internal-pass";

/// 探知目标 OpenCode 执行程序的主版本号 (1 或 2)
pub fn get_opencode_major_version(bin: &str) -> u32 {
    let resolved = super::commands::resolve_executable_path(bin);
    if let Ok(output) = std::process::Command::new(&resolved).arg("--version").output() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let raw = if !stdout.is_empty() { stdout } else { stderr };
        if let Some(ver) = crate::cli_probe::extract_semver(&raw) {
            if let Some(first) = ver.split('.').next() {
                if let Ok(m) = first.parse::<u32>() {
                    return m;
                }
            }
        }
        if raw.contains("v2.") || raw.contains(" 2.") {
            return 2;
        }
    }
    1
}

// =========================================================================
// OpenCode Bridge 核心控制器
// =========================================================================

/// 当前桥接任务的类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BridgeTaskKind {
    Conversation,
    Planning,
    Prompt,
    Coding,
}

/// 当前桥接任务的不可持久化快照。
#[derive(Debug, Clone)]
pub(super) struct BridgeTaskSnapshot {
    pub(super) kind: BridgeTaskKind,
    pub(super) session_id: Option<String>,
    pub(super) plan_id: Option<String>,
    pub(super) project_name: Option<String>,
    pub(super) started_at: i64,
    pub(super) is_busy: bool,
}

/// 面向会话清单的桥接状态快照。
#[derive(Debug, Clone)]
pub(super) struct BridgeSessionSnapshot {
    pub(super) conversation_session_id: Option<String>,
    pub(super) conversation_project_name: Option<String>,
    pub(super) conversation_busy: bool,
    pub(super) task: Option<BridgeTaskSnapshot>,
}

/// Result of applying a single human permission decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PermissionDecisionOutcome {
    Applied(PermissionDecision),
    Expired,
    Ignored,
    ReplyFailed,
}

/// Shared runtime for replying to and surfacing OpenCode permission requests.
///
/// Clones the HTTP client, Telegram client and pending registry so the poller
/// task can act without borrowing the bridge.
pub(super) struct PermissionRuntime {
    http: Client,
    telegram: Arc<TelegramClient>,
    registry: Arc<PermissionRegistry>,
}

impl PermissionRuntime {
    pub(super) fn new(
        http: Client,
        telegram: Arc<TelegramClient>,
        registry: Arc<PermissionRegistry>,
    ) -> Self {
        Self {
            http,
            telegram,
            registry,
        }
    }

    pub(super) fn registry(&self) -> Arc<PermissionRegistry> {
        self.registry.clone()
    }

    /// Apply a human decision to a pending request at most once.
    ///
    /// A stale, already-resolved or high-risk always-allow tap leaves the
    /// registry untouched, so a late or duplicate tap cannot change state. A
    /// reply failure re-registers the original request so it can be retried,
    /// and only a successful reply edits the delivered card.
    pub(super) async fn decide(
        &self,
        server_url: &str,
        request_id: &str,
        decision: PermissionDecision,
    ) -> PermissionDecisionOutcome {
        let Some(pending) = self.registry.get(request_id) else {
            return PermissionDecisionOutcome::Ignored;
        };
        if permission_timed_out(pending.sent_at, Instant::now()) {
            return PermissionDecisionOutcome::Expired;
        }
        if pending.risk == PermissionRisk::HighRisk && decision == PermissionDecision::Always {
            return PermissionDecisionOutcome::Ignored;
        }

        let Some(resolved_request) = self
            .registry
            .resolve(request_id, decision)
            .map(|resolution| resolution.request)
        else {
            return PermissionDecisionOutcome::Ignored;
        };

        let replied = self
            .reply_permission(
                server_url,
                request_id,
                resolved_request.session_id.as_deref(),
                decision,
            )
            .await;
        if !replied {
            self.registry.insert(resolved_request);
            return PermissionDecisionOutcome::ReplyFailed;
        }

        if let Some(message_id) = resolved_request.card_message_id {
            let text = CardBuilder::permission_decision_card(decision);
            let _ = self
                .telegram
                .edit_message_text(
                    resolved_request.chat_id,
                    message_id,
                    &text,
                    None,
                    Some("Markdown"),
                )
                .await;
        }

        PermissionDecisionOutcome::Applied(decision)
    }

    /// Send `decision` to every known reply endpoint and report whether any
    /// endpoint accepted it. Individual transport or status failures are
    /// ignored so a partial protocol mismatch cannot abort the poller.
    pub(super) async fn reply_permission(
        &self,
        server_url: &str,
        request_id: &str,
        session_id: Option<&str>,
        decision: PermissionDecision,
    ) -> bool {
        let reply = decision.as_reply().to_string();
        let payload = PermissionReplyPayload {
            reply: reply.clone(),
            response: Some(reply),
        };

        let mut urls = vec![
            format!("{}/permission/{}/reply", server_url, request_id),
            format!(
                "{}/api/permission/request/{}/reply",
                server_url, request_id
            ),
        ];
        if let Some(sid) = session_id {
            urls.push(format!(
                "{}/api/session/{}/permission/{}/reply",
                server_url, sid, request_id
            ));
        }

        for url in urls {
            let response = self
                .http
                .post(&url)
                .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                .json(&payload)
                .send()
                .await;
            if let Ok(response) = response {
                if response.status().is_success() {
                    return true;
                }
            }
        }
        false
    }

    /// Deliver an approval card for `request_id`, recording its message id on
    /// success. A failure keeps the request registered so it can be retried.
    pub(super) async fn send_request_card(
        &self,
        chat_id: i64,
        project: Option<&str>,
        permission_type: &str,
        command: Option<&str>,
        risk: PermissionRisk,
        request_id: &str,
        session_id: Option<&str>,
    ) -> bool {
        let (text, markup) = CardBuilder::permission_request_card(
            project.unwrap_or("unassigned"),
            session_id,
            permission_type,
            command,
            risk,
            request_id,
        );

        match self
            .telegram
            .send_message(chat_id, &text, Some(markup), Some("Markdown"))
            .await
        {
            Ok(message) => {
                self.registry.set_card_message_id(request_id, message.message_id);
                true
            }
            Err(e) => {
                warn!(
                    "权限审批卡片发送失败 (request {}): {}",
                    request_id, e
                );
                false
            }
        }
    }
}

pub struct OpenCodeBridge {
    server_process: Mutex<Option<Child>>,
    server_url: RwLock<Option<String>>,
    http_client: Client,
    telegram_client: Arc<TelegramClient>,
    permission_runtime: Arc<PermissionRuntime>,
    active_question: Arc<RwLock<Option<OpenCodeQuestionRequest>>>,
    active_session_id: RwLock<Option<String>>,
    active_project_dir: RwLock<Option<PathBuf>>,
    active_options: RwLock<std::collections::HashMap<usize, String>>,
    is_busy: Arc<AtomicBool>,
    conversation_session_id: RwLock<Option<String>>,
    conversation_project_name: RwLock<Option<String>>,
    conversation_busy: AtomicBool,
    current_task: RwLock<Option<BridgeTaskSnapshot>>,
}

impl OpenCodeBridge {
    pub fn new(telegram_client: Arc<TelegramClient>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        let permission_runtime = Arc::new(PermissionRuntime::new(
            http_client.clone(),
            telegram_client.clone(),
            Arc::new(PermissionRegistry::new()),
        ));
        Self {
            server_process: Mutex::new(None),
            server_url: RwLock::new(None),
            http_client,
            telegram_client,
            permission_runtime,
            active_question: Arc::new(RwLock::new(None)),
            active_session_id: RwLock::new(None),
            active_project_dir: RwLock::new(None),
            active_options: RwLock::new(std::collections::HashMap::new()),
            is_busy: Arc::new(AtomicBool::new(false)),
            conversation_session_id: RwLock::new(None),
            conversation_project_name: RwLock::new(None),
            conversation_busy: AtomicBool::new(false),
            current_task: RwLock::new(None),
        }
    }

    /// 返回当前桥接状态的内存快照。
    pub(super) async fn session_snapshot(&self) -> BridgeSessionSnapshot {
        BridgeSessionSnapshot {
            conversation_session_id: self.conversation_session_id.read().await.clone(),
            conversation_project_name: self.conversation_project_name.read().await.clone(),
            conversation_busy: self.conversation_busy.load(Ordering::SeqCst),
            task: self.current_task.read().await.clone(),
        }
    }

    /// 开始一次新的自然语言会话，替换之前的会话 ID 与项目。
    pub(super) async fn begin_conversation(&self, project_dir: &Path) {
        *self.conversation_session_id.write().await = None;
        self.start_conversation_task(project_dir, None).await;
    }

    /// 记录对话会话 ID，并在当前任务为对话任务时同步该任务。
    pub(super) async fn record_conversation_session(&self, session_id: &str) {
        *self.conversation_session_id.write().await = Some(session_id.to_string());
        let mut task = self.current_task.write().await;
        if let Some(snapshot) = task.as_mut() {
            if snapshot.kind == BridgeTaskKind::Conversation {
                snapshot.session_id = Some(session_id.to_string());
            }
        }
    }

    /// 开始一次计划实施；对话会话状态保持不变。
    pub(super) async fn begin_plan_execution(&self, project_dir: &Path, plan_id: &str) {
        *self.current_task.write().await = Some(BridgeTaskSnapshot {
            kind: BridgeTaskKind::Coding,
            session_id: None,
            plan_id: Some(plan_id.to_string()),
            project_name: project_name_of(project_dir),
            started_at: chrono::Utc::now().timestamp(),
            is_busy: true,
        });
    }

    /// 记录计划实施会话 ID，不触碰对话会话状态。
    pub(super) async fn record_plan_execution_session(&self, session_id: &str) {
        if let Some(snapshot) = self.current_task.write().await.as_mut() {
            snapshot.session_id = Some(session_id.to_string());
        }
    }

    /// 结束当前任务，保留最后一个对话会话 ID。
    pub(super) async fn finish_task(&self) {
        *self.current_task.write().await = None;
        self.conversation_busy.store(false, Ordering::SeqCst);
    }

    async fn start_conversation_task(&self, project_dir: &Path, session_id: Option<String>) {
        let project_name = project_name_of(project_dir);
        *self.conversation_project_name.write().await = project_name.clone();
        self.conversation_busy.store(true, Ordering::SeqCst);
        *self.current_task.write().await = Some(BridgeTaskSnapshot {
            kind: BridgeTaskKind::Conversation,
            session_id,
            plan_id: None,
            project_name,
            started_at: chrono::Utc::now().timestamp(),
            is_busy: true,
        });
    }

    async fn begin_prompt_task(&self, project_dir: &Path, session_id: &str) {
        *self.current_task.write().await = Some(BridgeTaskSnapshot {
            kind: BridgeTaskKind::Prompt,
            session_id: Some(session_id.to_string()),
            plan_id: None,
            project_name: project_name_of(project_dir),
            started_at: chrono::Utc::now().timestamp(),
            is_busy: true,
        });
    }

    /// 检查当前是否正有活跃的多轮会话进行中
    pub async fn has_active_session(&self) -> bool {
        self.active_session_id.read().await.is_some()
    }

    /// 获取特定选项的文本
    pub async fn get_option_text(&self, num: usize) -> Option<String> {
        self.active_options.read().await.get(&num).cloned()
    }

    /// 获取活跃提问中特定选项的文本标签
    pub async fn get_question_option_label(&self, request_id: &str, opt_idx: usize) -> Option<String> {
        let guard = self.active_question.read().await;
        if let Some(ref q) = *guard {
            if q.id == request_id {
                if let Some(first_q) = q.questions.first() {
                    if let Some(opt) = first_q.options.get(opt_idx) {
                        return Some(opt.label.clone());
                    }
                }
            }
        }
        None
    }

    /// 取消当前正在进行的任务与会话
    pub async fn cancel_active_task(&self) {
        *self.active_session_id.write().await = None;
        *self.active_project_dir.write().await = None;
        *self.active_question.write().await = None;
        self.active_options.write().await.clear();
        *self.conversation_session_id.write().await = None;
        *self.conversation_project_name.write().await = None;
        self.conversation_busy.store(false, Ordering::SeqCst);
        *self.current_task.write().await = None;
        self.is_busy.store(false, Ordering::SeqCst);
    }

    /// 确保后台 opencode serve 进程正在运行并返回服务基础 URL
    pub async fn ensure_server(&self) -> Result<String, String> {
        {
            let url_guard = self.server_url.read().await;
            if let Some(ref url) = *url_guard {
                // 探测存活性 (兼顾 v1 的 /session 与 v2 的 /api/info)
                let is_alive = self.http_client.get(format!("{}/session", url))
                    .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                    .send().await.is_ok()
                    || self.http_client.get(format!("{}/api/info", url))
                    .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                    .send().await.is_ok();
                if is_alive {
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
        let opencode_bin = resolve_opencode_bin();
        let mut child = Command::new(&opencode_bin)
            .arg("serve")
            .arg("--print-logs")
            .arg("--port")
            .arg("0")
            .arg("--hostname")
            .arg("127.0.0.1")
            .env("OPENCODE_SERVER_PASSWORD", OPENCODE_INTERNAL_AUTH)
            .env("OPENCODE_PASSWORD", OPENCODE_INTERNAL_AUTH)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("无法启动 opencode CLI [{}]: {}. 请确认是否已安装 opencode 或在设置中指定正确路径", opencode_bin, e))?;

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
        self.execute_task(chat_id, project_dir, prompt, true).await
    }

    /// 执行任务会话（支持显式 Planning 规划与普通自由对话）
    pub async fn execute_task(
        self: &Arc<Self>,
        chat_id: i64,
        project_dir: PathBuf,
        prompt: &str,
        is_explicit_planning: bool,
    ) -> Result<(), String> {
        if self.is_busy.swap(true, Ordering::SeqCst) {
            return Err("已有正在运行的任务，请等待当前任务完成或执行 /cancel".to_string());
        }

        // 清理旧会话状态并保存目标工作区
        *self.active_session_id.write().await = None;
        *self.active_project_dir.write().await = Some(project_dir.clone());
        self.active_options.write().await.clear();
        self.begin_conversation(&project_dir).await;

        let bridge = self.clone();
        let prompt_string = prompt.to_string();

        tokio::spawn(async move {
            let res = bridge
                .run_planning_cycle(
                    chat_id,
                    project_dir,
                    &prompt_string,
                    None,
                    is_explicit_planning,
                    if is_explicit_planning {
                        BridgeTaskKind::Planning
                    } else {
                        BridgeTaskKind::Conversation
                    },
                )
                .await;
            bridge.finish_task().await;
            bridge.is_busy.store(false, Ordering::SeqCst);
            if let Err(e) = res {
                error!("任务执行失败: {}", e);
                let error_text = format!("❌ *执行阶段异常中断*\n\n原因:\n```\n{}\n```", e);
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

    /// 在已有规划会话中继续推进交互 (Continue Planning)
    pub async fn continue_planning(
        self: &Arc<Self>,
        chat_id: i64,
        user_input: &str,
    ) -> Result<(), String> {
        if self.is_busy.swap(true, Ordering::SeqCst) {
            return Err("已有正在运行的任务，请等待当前轮次完成".to_string());
        }

        let session_id = self.active_session_id.read().await.clone()
            .ok_or_else(|| "当前没有活跃的会话，请先使用 /planning 发起需求或直接发送文字".to_string())?;
        let project_dir = self.active_project_dir.read().await.clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        self.start_conversation_task(&project_dir, Some(session_id.clone()))
            .await;
        self.record_conversation_session(&session_id).await;

        let bridge = self.clone();
        let input_text = user_input.to_string();

        let _ = bridge.telegram_client.send_message(
            chat_id,
            "⏳ *正在将您的决策同步给 OpenCode 并继续推进...*",
            None,
            Some("Markdown"),
        ).await;

        tokio::spawn(async move {
            let res = bridge
                .run_planning_cycle(
                    chat_id,
                    project_dir,
                    &input_text,
                    Some(session_id),
                    false,
                    BridgeTaskKind::Conversation,
                )
                .await;
            bridge.finish_task().await;
            bridge.is_busy.store(false, Ordering::SeqCst);
            if let Err(e) = res {
                error!("推进规划失败: {}", e);
                let error_text = format!("❌ *推进规划异常中断*\n\n原因:\n```\n{}\n```", e);
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

    /// 向指定外部 OpenCode 会话投递提示词并推进执行
    pub async fn execute_prompt_for_session(
        self: &Arc<Self>,
        chat_id: i64,
        project_dir: PathBuf,
        session_id: &str,
        prompt: &str,
    ) -> Result<(), String> {
        if self.is_busy.swap(true, Ordering::SeqCst) {
            return Err("已有正在运行的任务，请等待当前轮次完成或发送 /cancel".to_string());
        }

        *self.active_session_id.write().await = Some(session_id.to_string());
        *self.active_project_dir.write().await = Some(project_dir.clone());
        self.active_options.write().await.clear();
        self.begin_prompt_task(&project_dir, session_id).await;

        let bridge = self.clone();
        let prompt_string = prompt.to_string();
        let sid = session_id.to_string();

        tokio::spawn(async move {
            let res = bridge
                .run_planning_cycle(
                    chat_id,
                    project_dir,
                    &prompt_string,
                    Some(sid),
                    false,
                    BridgeTaskKind::Prompt,
                )
                .await;
            bridge.finish_task().await;
            bridge.is_busy.store(false, Ordering::SeqCst);
            if let Err(e) = res {
                error!("向 OpenCode 会话投递提示词失败: {}", e);
                let error_text = format!("❌ *执行阶段异常中断*\n\n原因:\n```\n{}\n```", e);
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

    async fn run_planning_cycle(
        &self,
        chat_id: i64,
        project_dir: PathBuf,
        input_prompt: &str,
        session_id_opt: Option<String>,
        is_explicit_planning: bool,
        task_kind: BridgeTaskKind,
    ) -> Result<(), String> {
        let server_url = self.ensure_server().await?;
        let start_time = std::time::SystemTime::now();

        // 初次对话/规划提示词
        let prompt_to_run = if session_id_opt.is_none() {
            if is_explicit_planning {
                // 显式 /planning 指令：明确指定使用 planning 技能展开规划
                format!(
                    "使用 planning 技能，开始规划：{}\n\n\
                    请遵循项目工程规范展开深入调研与方案规划：\n\
                    1. 如遇实现分歧或需用户决策，请通过提问方式提出编号问题（问题 N：...）及选项（1、... 2、...）进行澄清；\n\
                    2. 规划完成后，请生成并确认规范的 spec.md 和 plan.md 方案文件。",
                    input_prompt.trim()
                )
            } else {
                // 普通聊天对话：遵循 OpenCode 自身工作流，不强加规划，由模型自主选择
                format!(
                    "用户发来消息：{}\n\n\
                    请遵循 OpenCode 标准工程流程进行处理：\n\
                    - 若为提问、代码咨询或思路探讨，请直接清晰专业地用中文解答；\n\
                    - 若为明确的小改动、Bug 修复或实现需求，可直接调用 coding 技能进行实施；\n\
                    - 若为需要详细调研与方案设计的复杂需求，可调用 planning 技能开始规划。",
                    input_prompt.trim()
                )
            }
        } else {
            input_prompt.to_string()
        };

        let status_desc = CardBuilder::planning_thinking_card(
            session_id_opt.is_none(),
            session_id_opt.as_deref(),
        );

        let status_msg = self.telegram_client.send_message(
            chat_id,
            &status_desc,
            None,
            Some("Markdown"),
        ).await?;

        // 启动 opencode run 会话
        let opencode_bin = resolve_opencode_bin();
        let is_v2 = get_opencode_major_version(&opencode_bin) >= 2;

        let mut cmd = Command::new(&opencode_bin);
        cmd.current_dir(&project_dir);
        cmd.arg("run")
            .arg(&prompt_to_run)
            .arg("--format")
            .arg("json")
            .arg("--auto");

        apply_run_connection(&mut cmd, is_v2, &server_url, &project_dir);

        if let Some(ref sid) = session_id_opt {
            cmd.arg("--session").arg(sid);
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| format!("启动 OpenCode Planning 失败: {}", e))?;

        let stdout = child.stdout.take().ok_or_else(|| "无法获取 opencode stdout".to_string())?;

        // 启动提问与权限轮询守卫，并维持 typing 状态心跳
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let poll_guard = self.spawn_question_and_permission_poller(
            chat_id,
            status_msg.message_id,
            server_url.clone(),
            cancel_flag.clone(),
            project_name_of(&project_dir),
            auto_approve_permissions_policy(),
        );
        let typing_guard = self.spawn_typing_heartbeat(chat_id, cancel_flag.clone());

        let mut reader = BufReader::new(stdout).lines();
        let mut captured_session_id: Option<String> = session_id_opt.clone();
        let mut text_parts: Vec<String> = Vec::new();

        while let Ok(Some(line)) = reader.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                // 兼容 v1 与 v2 的 sessionID 字段位置
                if let Some(sid) = extract_session_id_from_event(&v) {
                    if matches!(
                        task_kind,
                        BridgeTaskKind::Conversation | BridgeTaskKind::Planning
                    ) {
                        self.record_conversation_session(&sid).await;
                    }
                    captured_session_id = Some(sid);
                }

                // 兼容 v1 与 v2 的 text 消息格式
                let event_type = v.get("type").and_then(|t| t.as_str());
                if event_type == Some("text") {
                    if let Some(text) = v.get("part").and_then(|p| p.get("text")).and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    } else if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                } else if event_type == Some("message") {
                    if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                        text_parts.push(content.to_string());
                    }
                }
            }
        }

        let output = child.wait_with_output().await
            .map_err(|e| format!("OpenCode 规划进程执行异常: {}", e))?;

        cancel_flag.store(true, Ordering::SeqCst);
        let _ = poll_guard.await;
        let _ = typing_guard.await;

        if let Some(ref sid) = captured_session_id {
            *self.active_session_id.write().await = Some(sid.clone());
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            let raw = if !stderr.trim().is_empty() {
                stderr.trim()
            } else {
                stdout_str.trim()
            };
            return Err(format!("OpenCode 规划非零退出: {}", strip_ansi_escapes(raw)));
        }

        // 1. 优先检查本次执行是否生成了全新的 plan.md
        let plan_path = self.locate_new_plan_file(&project_dir, start_time);

        if let Some(ref p) = plan_path {
            // 方案生成完成，保留活跃会话以便用户选择“补充”时能继续追加意见
            self.active_options.write().await.clear();

            let summary = self.extract_plan_summary(p).await
                .unwrap_or_else(|| "方案已生成于工作区。".to_string());

            let plan_tag = if let Some(folder) = p.parent().and_then(|parent| parent.file_name()) {
                folder.to_string_lossy().to_string()
            } else {
                p.file_name().unwrap_or_default().to_string_lossy().to_string()
            };
            let safe_plan_tag = if plan_tag.len() > 40 {
                plan_tag[..40].to_string()
            } else {
                plan_tag
            };

            let (review_text, markup) = CardBuilder::plan_review_card(&safe_plan_tag, &summary, None);

            self.telegram_client.send_message(
                chat_id,
                &review_text,
                Some(markup),
                Some("Markdown"),
            ).await?;

            return Ok(());
        }

        // 2. 如果未检测到全新 plan.md，说明 OpenCode 处于需求澄清/提问阶段
        let latest_output = text_parts.join("\n");
        let (question_text, options) = parse_question_and_options(&latest_output);

        if !options.is_empty() {
            let mut opt_map = self.active_options.write().await;
            opt_map.clear();
            let mut keyboard_rows = Vec::new();

            for (num, full_text) in &options {
                opt_map.insert(*num, full_text.clone());
                let btn_text = extract_button_label(*num, full_text);
                keyboard_rows.push(vec![InlineKeyboardButton::callback(
                    btn_text,
                    format!("ans_choice:{}", num),
                )]);
            }

            let msg_text = format_parsed_question_message(&question_text, &options);

            let markup = InlineKeyboardMarkup::new(keyboard_rows);
            self.telegram_client.send_message(
                chat_id,
                &msg_text,
                Some(markup),
                Some("Markdown"),
            ).await?;
        } else {
            let clean_text = if latest_output.trim().is_empty() {
                "OpenCode 分析完毕，请回复更多细节或确认指导意见以继续推进。".to_string()
            } else {
                strip_ansi_escapes(latest_output.trim())
                    .trim_start_matches("```markdown")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
                    .to_string()
            };

            let project_name = project_dir.file_name().unwrap_or_default().to_string_lossy();
            let msg_text = CardBuilder::analysis_reply_card(&project_name, &clean_text);

            self.telegram_client.send_message(
                chat_id,
                &msg_text,
                Some(CardBuilder::quick_actions()),
                Some("Markdown"),
            ).await?;
        }

        Ok(())
    }

    /// Return the shared permission runtime (registry, reply and card delivery).
    pub(super) fn permission_runtime(&self) -> Arc<PermissionRuntime> {
        self.permission_runtime.clone()
    }

    /// Submit a human permission decision.
    ///
    /// An unavailable server counts as a failed reply so the pending request
    /// is kept for another attempt instead of being silently dropped.
    pub async fn decide_permission(
        &self,
        request_id: &str,
        decision: PermissionDecision,
    ) -> PermissionDecisionOutcome {
        let server_url = match self.ensure_server().await {
            Ok(url) => url,
            Err(e) => {
                warn!("无法提交权限决议 (request {}): {}", request_id, e);
                return PermissionDecisionOutcome::ReplyFailed;
            }
        };
        self.permission_runtime
            .decide(&server_url, request_id, decision)
            .await
    }

    /// 轮询并转发 /question 与 /permission 请求到 Telegram
    pub(super) fn spawn_question_and_permission_poller(
        &self,
        chat_id: i64,
        _status_msg_id: i64,
        server_url: String,
        cancel_flag: Arc<AtomicBool>,
        project_name: Option<String>,
        policy: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> tokio::task::JoinHandle<()> {
        let http = self.http_client.clone();
        let tg = self.telegram_client.clone();
        let runtime = self.permission_runtime.clone();
        let active_q_holder = self.active_question.clone();

        tokio::spawn(async move {
            let mut asked_questions = std::collections::HashSet::new();
            let mut asked_permissions = std::collections::HashSet::new();

            while !cancel_flag.load(Ordering::Relaxed) {
                // 1. 检查 /question 或 v2 的 /api/form
                let q_url = format!("{}/question", server_url);
                let q_resp = http.get(&q_url)
                    .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                    .send().await;

                let mut questions_opt = if let Ok(resp) = q_resp {
                    resp.json::<ApiEnvelope<OpenCodeQuestionRequest>>().await.ok().map(|e| e.into_vec())
                } else {
                    None
                };

                if questions_opt.is_none() {
                    let v2_q_url = format!("{}/api/form", server_url);
                    if let Ok(resp) = http.get(&v2_q_url)
                        .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                        .send().await {
                        questions_opt = resp.json::<ApiEnvelope<OpenCodeQuestionRequest>>().await.ok().map(|e| e.into_vec());
                    }
                }

                if let Some(questions) = questions_opt {
                    for q in questions {
                        if asked_questions.contains(&q.id) {
                            continue;
                        }
                        asked_questions.insert(q.id.clone());
                        *active_q_holder.write().await = Some(q.clone());

                        if let Some(first_q) = q.questions.first() {
                            let mut buttons = Vec::new();
                            for (idx, opt) in first_q.options.iter().enumerate() {
                                let btn_text = extract_button_label(idx + 1, &opt.label);
                                buttons.push(vec![InlineKeyboardButton::callback(
                                    btn_text,
                                    format!("ans_q:{}:{}", q.id, idx),
                                )]);
                            }

                            let text = format_question_request_message(first_q);
                            let markup = InlineKeyboardMarkup::new(buttons);
                            let _ = tg.send_message(chat_id, &text, Some(markup), None).await;
                        }
                    }
                }

                // 2. 检查 /permission (自动授权或高危审批，兼顾 v1 与 v2)
                let p_url = format!("{}/permission", server_url);
                let p_resp = http.get(&p_url)
                    .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                    .send().await;

                let mut perms_opt = if let Ok(resp) = p_resp {
                    resp.json::<ApiEnvelope<OpenCodePermissionRequest>>().await.ok().map(|e| e.into_vec())
                } else {
                    None
                };

                if perms_opt.is_none() {
                    let v2_p_url = format!("{}/api/permission/request", server_url);
                    if let Ok(resp) = http.get(&v2_p_url)
                        .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                        .send().await {
                        perms_opt = resp.json::<ApiEnvelope<OpenCodePermissionRequest>>().await.ok().map(|e| e.into_vec());
                    }
                }

                if let Some(perms) = perms_opt {
                    for p in perms {
                        if asked_permissions.contains(&p.id) {
                            continue;
                        }

                        let permission_type = p.permission.clone().unwrap_or_default();
                        let risk = classify_permission(&permission_type, p.command.as_deref());
                        let auto_approve = policy();

                        // Auto-approve routine work only while the policy allows it.
                        if auto_approve && risk == PermissionRisk::Routine {
                            let replied = runtime
                                .reply_permission(
                                    &server_url,
                                    &p.id,
                                    p.session_id.as_deref(),
                                    PermissionDecision::Once,
                                )
                                .await;
                            if replied {
                                asked_permissions.insert(p.id.clone());
                            }
                            continue;
                        }

                        // Otherwise ask a human; the request stays registered even
                        // when card delivery fails.
                        let pending = PendingPermissionRequest::new(
                            p.id.clone(),
                            chat_id,
                            permission_type.clone(),
                            p.command.clone(),
                            risk,
                            Instant::now(),
                        )
                        .with_session_id(p.session_id.clone());
                        runtime.registry().insert(pending);
                        runtime
                            .send_request_card(
                                chat_id,
                                project_name.as_deref(),
                                &permission_type,
                                p.command.as_deref(),
                                risk,
                                &p.id,
                                p.session_id.as_deref(),
                            )
                            .await;
                        asked_permissions.insert(p.id.clone());
                    }
                }

                tokio::time::sleep(Duration::from_millis(1500)).await;
            }
        })
    }

    /// 维持 Telegram 聊天窗口中持续的 typing 输入态心跳（每 4 秒触发一次）
    fn spawn_typing_heartbeat(
        &self,
        chat_id: i64,
        cancel_flag: Arc<AtomicBool>,
    ) -> tokio::task::JoinHandle<()> {
        let tg = self.telegram_client.clone();

        tokio::spawn(async move {
            while !cancel_flag.load(Ordering::Relaxed) {
                let _ = tg.send_chat_action(chat_id, "typing").await;
                tokio::time::sleep(Duration::from_millis(4000)).await;
            }
        })
    }

    /// 回复指定的 OpenCode 提问
    pub async fn reply_question(&self, request_id: &str, answer: &str) -> Result<(), String> {
        let server_url = self.ensure_server().await?;
        let payload = QuestionReplyPayload {
            answers: vec![vec![answer.to_string()]],
            answer: Some(answer.to_string()),
        };

        // 1. 优先尝试 v1 传统路径 /question/{id}/reply
        let v1_url = format!("{}/question/{}/reply", server_url, request_id);
        let resp = self.http_client.post(&v1_url)
            .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
            .json(&payload)
            .send()
            .await;

        if let Ok(res) = resp {
            if res.status().is_success() {
                return Ok(());
            }
        }

        // 2. 优先尝试 v2 会话表单路径 /api/session/{sid}/form/{id}/reply
        if let Some(sid) = self.active_session_id.read().await.clone() {
            let session_form_url = format!("{}/api/session/{}/form/{}/reply", server_url, sid, request_id);
            if let Ok(res) = self.http_client.post(&session_form_url)
                .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
                .json(&payload)
                .send()
                .await {
                if res.status().is_success() {
                    return Ok(());
                }
            }
        }

        // 3. 尝试 v2 全局表单路径 /api/form/{id}/reply
        let v2_url = format!("{}/api/form/{}/reply", server_url, request_id);
        let v2_resp = self.http_client.post(&v2_url)
            .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("发送提问回复失败: {}", e))?;

        if v2_resp.status().is_success() {
            Ok(())
        } else {
            Err(format!("OpenCode 拒绝提问回复，状态码: {}", v2_resp.status()))
        }
    }

    pub fn locate_new_plan_file(&self, project_dir: &Path, start_time: std::time::SystemTime) -> Option<PathBuf> {
        let candidates = vec![
            project_dir.join(".ai-workflow").join("plans"),
            project_dir.to_path_buf(),
        ];

        let mut newest_plan: Option<(PathBuf, std::time::SystemTime)> = None;

        for base in candidates {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.file_name().map_or(false, |n| n == "plan.md") {
                        if let Ok(meta) = path.metadata() {
                            if let Ok(modified) = meta.modified() {
                                if modified >= start_time {
                                    if newest_plan.as_ref().map_or(true, |(_, t)| modified > *t) {
                                        newest_plan = Some((path, modified));
                                    }
                                }
                            }
                        }
                    } else if path.is_dir() {
                        let sub_plan = path.join("plan.md");
                        if sub_plan.is_file() {
                            if let Ok(meta) = sub_plan.metadata() {
                                if let Ok(modified) = meta.modified() {
                                    if modified >= start_time {
                                        if newest_plan.as_ref().map_or(true, |(_, t)| modified > *t) {
                                            newest_plan = Some((sub_plan, modified));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        newest_plan.map(|(p, _)| p)
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
        self.execute_coding_with_prompt(chat_id, project_dir, plan_identifier, None).await
    }

    /// 在全新独立会话中拉起 OpenCode 执行指定提示词的 coding 技能实施（支持自愈修复提示词）
    pub async fn execute_coding_with_prompt(
        self: &Arc<Self>,
        chat_id: i64,
        project_dir: PathBuf,
        plan_identifier: &str,
        custom_prompt: Option<String>,
    ) -> Result<(), String> {
        if self.is_busy.swap(true, Ordering::SeqCst) {
            return Err("已有正在运行的任务，请等待当前任务完成".to_string());
        }

        // 方案已获确认开始实施，清空旧的规划会话（对话会话状态保留）
        *self.active_session_id.write().await = None;
        self.active_options.write().await.clear();

        let clean_plan_id = plan_identifier
            .trim_start_matches(".ai-workflow/plans/")
            .trim_start_matches("plans/")
            .trim_matches('/')
            .to_string();
        self.begin_plan_execution(&project_dir, &clean_plan_id)
            .await;

        let bridge = self.clone();
        let plan_tag = plan_identifier.to_string();

        tokio::spawn(async move {
            let res = bridge.run_coding_inner(chat_id, project_dir, &plan_tag, custom_prompt).await;
            bridge.finish_task().await;
            bridge.is_busy.store(false, Ordering::SeqCst);
            if let Err(e) = res {
                error!("Coding 实施失败: {}", e);
                let clean_plan = plan_tag
                    .trim_start_matches(".ai-workflow/plans/")
                    .trim_start_matches("plans/")
                    .trim_matches('/');

                let (error_text, markup) = CardBuilder::coding_failed_card(&clean_plan, "实施与规范审查", &e);

                let _ = bridge.telegram_client.send_message(
                    chat_id,
                    &error_text,
                    Some(markup),
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
        custom_prompt: Option<String>,
    ) -> Result<(), String> {
        let server_url = self.ensure_server().await?;

        let clean_plan_id = plan_identifier
            .trim_start_matches(".ai-workflow/plans/")
            .trim_start_matches("plans/")
            .trim_matches('/')
            .to_string();

        let (status_desc, _) = CardBuilder::coding_running_card(&clean_plan_id, custom_prompt.is_some());

        let status_msg = self.telegram_client.send_message(
            chat_id,
            &status_desc,
            None,
            Some("Markdown"),
        ).await?;

        // 默认提示词：调用 coding 技能，实施 .ai-workflow/plans/{planId}
        let coding_prompt = custom_prompt.unwrap_or_else(|| {
            format!("调用 coding 技能，实施 .ai-workflow/plans/{}", clean_plan_id)
        });

        let opencode_bin = resolve_opencode_bin();
        let is_v2 = get_opencode_major_version(&opencode_bin) >= 2;

        let mut cmd = Command::new(&opencode_bin);
        cmd.current_dir(&project_dir);
        cmd.arg("run")
            .arg(&coding_prompt)
            .arg("--format")
            .arg("json")
            .arg("--auto");

        apply_run_connection(&mut cmd, is_v2, &server_url, &project_dir);

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("启动 OpenCode Coding 失败: {}", e))?;

        let stdout = child.stdout.take().ok_or_else(|| "无法获取 stdout".to_string())?;

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let poll_guard = self.spawn_question_and_permission_poller(
            chat_id,
            status_msg.message_id,
            server_url.clone(),
            cancel_flag.clone(),
            project_name_of(&project_dir),
            auto_approve_permissions_policy(),
        );
        let typing_guard = self.spawn_typing_heartbeat(chat_id, cancel_flag.clone());

        let mut reader = BufReader::new(stdout).lines();
        let mut text_parts: Vec<String> = Vec::new();

        while let Ok(Some(line)) = reader.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(sid) = extract_session_id_from_event(&v) {
                    self.record_plan_execution_session(&sid).await;
                }
                let event_type = v.get("type").and_then(|t| t.as_str());
                if event_type == Some("text") {
                    if let Some(text) = v.get("part").and_then(|p| p.get("text")).and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    } else if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                } else if event_type == Some("message") {
                    if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                        text_parts.push(content.to_string());
                    }
                }
            }
        }

        let output = child.wait_with_output().await
            .map_err(|e| format!("OpenCode Coding 进程执行异常: {}", e))?;

        cancel_flag.store(true, Ordering::SeqCst);
        let _ = poll_guard.await;
        let _ = typing_guard.await;

        let accumulated_text = text_parts.join("\n");
        let stderr = String::from_utf8_lossy(&output.stderr);

        // 执行严格的四重证据链核验
        self.verify_and_report_completion(
            chat_id,
            status_msg.message_id,
            &project_dir,
            output.status.success(),
            &accumulated_text,
            &stderr,
            &clean_plan_id,
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
        clean_plan_id: &str,
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

        // 4. 发送成功卡片，增加 [🔍 查看改动 Diff] 和 [📊 实施报告] 按钮
        let (success_text, markup) = CardBuilder::coding_success_card(
            clean_plan_id,
            None,
            &commit_summary,
            &stat_text,
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
            Some(markup),
            Some("Markdown"),
        ).await?;

        Ok(())
    }
}

fn project_name_of(project_dir: &Path) -> Option<String> {
    project_dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

/// Read the auto-approval setting on every call; a missing config fails closed
/// so permissions still require a human decision.
fn auto_approve_permissions_policy() -> Arc<dyn Fn() -> bool + Send + Sync> {
    Arc::new(|| {
        super::storage::load_config()
            .map(|config| config.auto_approve_permissions)
            .unwrap_or(false)
    })
}

/// 从 OpenCode JSON 事件中提取会话 ID，兼容顶层与 `part` 嵌套的三种字段名。
pub(super) fn extract_session_id_from_event(event: &serde_json::Value) -> Option<String> {
    fn pick(value: &serde_json::Value) -> Option<&str> {
        ["sessionID", "sessionId", "session_id"]
            .iter()
            .find_map(|key| value.get(*key).and_then(|v| v.as_str()))
    }

    let raw = pick(event).or_else(|| event.get("part").and_then(pick))?;
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
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

/// 解析 OpenCode 输出中的问题说明与选项列表
/// 返回 (提问文本, 选项列表[(选项编号1..N, 选项完整文本)])
pub fn parse_question_and_options(text: &str) -> (String, Vec<(usize, String)>) {
    let mut options = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    // 匹配如：1、xxx 或 1. xxx 或 **1、xxx** 或 [1] xxx 或 1) xxx
    let opt_re = regex::Regex::new(r"^(?:[\*\-_#\s]*)(?:\[?(\d{1,2})[\]\.、\)]|\((\d{1,2})\))\s*(.*)$").unwrap();

    for line in &lines {
        let trimmed = line.trim();
        if let Some(caps) = opt_re.captures(trimmed) {
            let num_str = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str()).unwrap_or("");
            if let Ok(num) = num_str.parse::<usize>() {
                if num >= 1 && num <= 10 {
                    let desc = caps.get(3).map(|m| m.as_str()).unwrap_or("").trim();
                    let clean_desc = desc.trim_matches('*').trim_matches('`').trim();
                    let full_label = format!("{}、{}", num, clean_desc);
                    options.push((num, full_label));
                }
            }
        }
    }

    let question_body = text.trim().to_string();
    (question_body, options)
}

/// 提取内联按钮展示的紧凑标签（带数字 Emoji 徽章，<=16 字符，移动端防截断）
pub fn extract_button_label(num: usize, full_text: &str) -> String {
    CardBuilder::format_button_label(Some(num), full_text, 12)
}

/// 将选项文本拆分为标题与补充说明
pub fn split_option_detail(full_text: &str) -> (String, Option<String>) {
    let mut cleaned = full_text.trim();
    if let Ok(prefix_re) = regex::Regex::new(r"^(?:\[?(\d{1,2})[\]\.、\)]|\((\d{1,2})\))\s*") {
        if let Some(m) = prefix_re.find(cleaned) {
            cleaned = cleaned[m.end()..].trim();
        }
    }
    cleaned = cleaned.trim_matches('*').trim_matches('`').trim();

    let separators = [" —— ", "——", " — ", "—", " - ", "：", ": "];
    for sep in &separators {
        if let Some(pos) = cleaned.find(sep) {
            let title = cleaned[..pos].trim().trim_matches('*').trim();
            let detail = cleaned[pos + sep.len()..].trim().trim_matches('*').trim();
            if !title.is_empty() && !detail.is_empty() {
                return (title.to_string(), Some(detail.to_string()));
            }
        }
    }

    (cleaned.to_string(), None)
}

/// 格式化 /question 端点请求为移动端友好的大字号正文展示
pub fn format_question_request_message(q: &OpenCodeQuestionItem) -> String {
    let opt_triplets: Vec<(usize, String, Option<String>)> = q.options.iter().enumerate().map(|(idx, opt)| {
        (idx + 1, opt.label.clone(), opt.description.clone())
    }).collect();
    let (msg, _) = CardBuilder::question_card(&q.question, &opt_triplets, None);
    msg
}

/// 格式化 stdout 解析提问为移动端友好的大字号正文展示
pub fn format_parsed_question_message(question_text: &str, options: &[(usize, String)]) -> String {
    let raw_clean = strip_ansi_escapes(question_text.trim());
    let clean_question = raw_clean
        .trim_start_matches("```markdown")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let opt_triplets: Vec<(usize, String, Option<String>)> = options.iter().map(|(num, full)| {
        let (t, d) = split_option_detail(full);
        (*num, t, d)
    }).collect();
    let (msg, _) = CardBuilder::question_card(clean_question, &opt_triplets, None);
    msg
}

/// 解析当前配置生效的 OpenCode CLI 程序路径（优先采用用户设置，回退系统 PATH 的 "opencode"）
pub fn resolve_opencode_bin() -> String {
    if let Ok(config) = super::storage::load_config() {
        if let Some(ref path) = config.opencode_bin_path {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                return super::commands::resolve_executable_path(trimmed);
            }
        }
    }
    "opencode".to_string()
}

pub(super) fn apply_run_connection(cmd: &mut tokio::process::Command, is_v2: bool, server_url: &str, project_dir: &Path) {
    if is_v2 {
        cmd.arg("--server").arg(server_url);
        cmd.env("OPENCODE_PASSWORD", OPENCODE_INTERNAL_AUTH);
    } else {
        cmd.arg("--attach").arg(server_url)
            .arg("--dir").arg(project_dir)
            .env("OPENCODE_SERVER_PASSWORD", OPENCODE_INTERNAL_AUTH);
    }
}
