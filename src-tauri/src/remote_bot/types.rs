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
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
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
