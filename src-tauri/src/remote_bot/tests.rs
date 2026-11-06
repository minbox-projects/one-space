use super::auth::AuthGuard;
use super::types::{
    InlineKeyboardButton, InlineKeyboardMarkup, RemoteBotConfig, TelegramUpdate,
};

#[test]
fn test_remote_bot_config_defaults() {
    let config = RemoteBotConfig::default();
    assert!(!config.enabled);
    assert!(config.bot_token.is_empty());
    assert!(config.allowed_chat_ids.is_empty());
    assert!(config.auto_approve_permissions);
}

#[test]
fn test_auth_guard() {
    let mut config = RemoteBotConfig::default();
    assert!(AuthGuard::can_initial_bind(&config));
    assert!(!AuthGuard::is_authorized(123456, &config));

    config.allowed_chat_ids.push(123456);
    assert!(!AuthGuard::can_initial_bind(&config));
    assert!(AuthGuard::is_authorized(123456, &config));
    assert!(!AuthGuard::is_authorized(789012, &config));
}

#[test]
fn test_inline_keyboard_serialization() {
    let button1 = InlineKeyboardButton::callback("选项 A", "opt_a");
    let button2 = InlineKeyboardButton::callback("选项 B", "opt_b");
    let markup = InlineKeyboardMarkup::new(vec![vec![button1, button2]]);

    let json = serde_json::to_string(&markup).expect("序列化必须成功");
    assert!(json.contains("选项 A"));
    assert!(json.contains("opt_a"));
    assert!(json.contains("inline_keyboard"));

    let deserialized: InlineKeyboardMarkup =
        serde_json::from_str(&json).expect("反序列化必须成功");
    assert_eq!(deserialized.inline_keyboard.len(), 1);
    assert_eq!(deserialized.inline_keyboard[0].len(), 2);
    assert_eq!(deserialized.inline_keyboard[0][0].text, "选项 A");
}

#[test]
fn test_telegram_update_deserialization() {
    let raw_json = r#"{
        "update_id": 99991,
        "message": {
            "message_id": 101,
            "date": 1700000000,
            "chat": {
                "id": 88888,
                "type": "private",
                "username": "tester"
            },
            "from": {
                "id": 88888,
                "is_bot": false,
                "first_name": "TestUser"
            },
            "text": "/status"
        }
    }"#;

    let update: TelegramUpdate = serde_json::from_str(raw_json).expect("必须成功解析 Update");
    assert_eq!(update.update_id, 99991);
    let msg = update.message.expect("必须有 message");
    assert_eq!(msg.message_id, 101);
    assert_eq!(msg.chat.id, 88888);
    assert_eq!(msg.text.as_deref(), Some("/status"));
}

#[test]
fn test_opencode_question_deserialization() {
    let raw_json = r#"[
        {
            "id": "q-101",
            "session_id": "ses_test",
            "questions": [
                {
                    "question": "鉴权机制选择？",
                    "multiple": false,
                    "options": [
                        {"label": "JWT 无状态 (推荐)"},
                        {"label": "Session 状态"}
                    ]
                }
            ]
        }
    ]"#;

    let questions: Vec<super::opencode_bridge::OpenCodeQuestionRequest> =
        serde_json::from_str(raw_json).expect("必须成功解析 OpenCode 问题列表");
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].id, "q-101");
    assert_eq!(questions[0].questions[0].question, "鉴权机制选择？");
    assert_eq!(questions[0].questions[0].options.len(), 2);
    assert_eq!(questions[0].questions[0].options[0].label, "JWT 无状态 (推荐)");
}
