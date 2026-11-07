use serde::{Deserialize, Serialize};

/// Telegram Bot 运行与连接配置
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoteBotConfig {
    #[serde(default = "default_false")]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
    pub active_workspace: Option<String>,
    #[serde(default = "default_true")]
    pub auto_approve_permissions: bool,
    #[serde(default = "default_true")]
    pub enable_natural_dialogue: bool,
    #[serde(default)]
    pub opencode_bin_path: Option<String>,
    #[serde(default)]
    pub opencode_version: Option<String>,
    #[serde(default)]
    pub opencode_version_type: Option<String>,
    #[serde(default)]
    pub workspace_directories: Vec<String>,
    #[serde(default)]
    pub updated_at: i64,
}

fn default_false() -> bool {
    false
}

fn default_true() -> bool {
    true
}

impl Default for RemoteBotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_token: String::new(),
            allowed_chat_ids: Vec::new(),
            active_workspace: None,
            auto_approve_permissions: true,
            enable_natural_dialogue: true,
            opencode_bin_path: None,
            opencode_version: None,
            opencode_version_type: None,
            workspace_directories: Vec::new(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

/// 本机安装的 OpenCode 版本信息
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenCodeInstalledVersion {
    pub path: String,
    pub version: String,
    pub major_version: u32,
    pub version_type: String, // "v1" | "v2" | "unknown"
    pub is_active: bool,
}

/// 运行时状态快照（供前端监控与展示）
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RemoteBotStatus {
    pub running: bool,
    pub bot_username: Option<String>,
    pub bot_name: Option<String>,
    pub last_poll_time: Option<i64>,
    pub last_error: Option<String>,
    pub active_task_id: Option<String>,
    pub pending_question_id: Option<String>,
}

// =========================================================================
// Telegram API DTOs
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TelegramResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
    pub error_code: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TelegramUser {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub username: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TelegramChat {
    pub id: i64,
    pub r#type: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub first_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub from: Option<TelegramUser>,
    pub chat: TelegramChat,
    pub date: i64,
    pub text: Option<String>,
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TelegramCallbackQuery {
    pub id: String,
    pub from: TelegramUser,
    pub message: Option<TelegramMessage>,
    pub data: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
    pub callback_query: Option<TelegramCallbackQuery>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl InlineKeyboardButton {
    pub fn callback(text: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            callback_data: Some(data.into()),
            url: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

impl InlineKeyboardMarkup {
    pub fn new(rows: Vec<Vec<InlineKeyboardButton>>) -> Self {
        Self {
            inline_keyboard: rows,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendMessagePayload {
    pub chat_id: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EditMessageTextPayload {
    pub chat_id: i64,
    pub message_id: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnswerCallbackQueryPayload {
    pub callback_query_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_alert: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetMyCommandsPayload {
    pub commands: Vec<BotCommand>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendChatActionPayload {
    pub chat_id: i64,
    pub action: String,
}

// =========================================================================
// 外部 OpenCode 运行中会话模型 (External Running Session)
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PromptTargetSession {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub project_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenCodeTodoInfo {
    pub position: i32,
    pub status: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExternalOpenCodeSessionInfo {
    pub pid: u32,
    pub ppid: u32,
    pub elapsed: String,
    pub command: String,
    pub cwd: String,
    pub project_name: String,
    pub session_id: Option<String>,
    pub title: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub is_busy: bool,
    pub current_step: Option<u32>,
    pub last_tool: Option<String>,
    pub last_tool_status: Option<String>,
    pub last_tool_input_preview: Option<String>,
    pub last_reasoning: Option<String>,
    pub todos: Vec<OpenCodeTodoInfo>,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub tokens_reasoning: i64,
    pub tokens_cache_read: i64,
    pub updated_at: Option<i64>,
}

/// 会话托管决策模式
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SessionHostingDecisionMode {
    /// 人工决断：提问推送到 Telegram 等待用户点击选项
    Manual,
    /// 自动推荐：存在 (Recommended) 选项时自动代选并通知，其余等待人工决断
    AutoRecommended,
}

impl Default for SessionHostingDecisionMode {
    fn default() -> Self {
        SessionHostingDecisionMode::Manual
    }
}

/// 托管提问候选项
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HostedOptionItem {
    pub index: usize,
    pub label: String,
    pub description: Option<String>,
    pub is_recommended: bool,
}

/// 托管提问信息
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HostedQuestionItem {
    pub call_id: String,
    pub question_text: String,
    pub options: Vec<HostedOptionItem>,
}

/// 处于完全托管中的会话状态快照
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HostedSessionInfo {
    pub session_id: String,
    pub pid: u32,
    pub cwd: String,
    pub project_name: String,
    pub chat_id: i64,
    pub mode: SessionHostingDecisionMode,
    pub started_at: i64,
    pub last_observed_step: u32,
    pub completed_todos_count: usize,
    pub total_todos_count: usize,
    pub active_question_summary: Option<String>,
}

