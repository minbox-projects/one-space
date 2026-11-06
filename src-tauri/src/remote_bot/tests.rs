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

#[test]
fn test_parse_question_and_options() {
    use super::opencode_bridge::parse_question_and_options;

    let sample_output = r#"
正在分析设置界面...
问题 1：关于设置界面顶部 Tab 过多的优化方向，你倾向于哪种交互布局？

1、推荐：两级可展开侧边栏 —— 扩展性强，清晰归类
2、分组卡片式列表 —— 扁平展示，直观简单
3、水平滚动二级导航 —— 保持现状最小改动
4、弹窗分组设置 —— 聚焦单项
"#;

    let (question, options) = parse_question_and_options(sample_output);
    assert!(question.contains("问题 1"));
    assert_eq!(options.len(), 4);
    assert_eq!(options[0].0, 1);
    assert!(options[0].1.contains("两级可展开侧边栏"));
    assert_eq!(options[1].0, 2);
    assert!(options[1].1.contains("分组卡片式列表"));
    assert_eq!(options[2].0, 3);
    assert_eq!(options[3].0, 4);

    // 测试带加粗和符号的格式
    let bold_sample = r#"
**问题 2：是否需要保留历史配置？**
* 1. 是，自动迁移
* 2. 否，重置为默认值
"#;
    let (_, bold_options) = parse_question_and_options(bold_sample);
    assert_eq!(bold_options.len(), 2);
    assert_eq!(bold_options[0].0, 1);
    assert!(bold_options[0].1.contains("自动迁移"));
}

#[tokio::test]
async fn test_locate_new_plan_file_filter_history() {
    use std::fs;
    use std::time::{Duration, SystemTime};
    use super::client::TelegramClient;
    use super::opencode_bridge::OpenCodeBridge;
    use std::sync::Arc;

    let temp_dir = std::env::temp_dir().join(format!("onespace_test_{}", std::process::id()));
    let plans_dir = temp_dir.join(".ai-workflow").join("plans");
    fs::create_dir_all(&plans_dir).unwrap();

    // 1. 创建一个历史旧 plan.md
    let old_plan_dir = plans_dir.join("20260901-old-feature");
    fs::create_dir_all(&old_plan_dir).unwrap();
    let old_plan = old_plan_dir.join("plan.md");
    fs::write(&old_plan, "# Old Plan\n历史旧方案内容").unwrap();

    // 等待让旧文件 mtime 与 start_time 分隔开
    tokio::time::sleep(Duration::from_millis(100)).await;
    let start_time = SystemTime::now();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let tg_client = Arc::new(TelegramClient::new("dummy_token"));
    let bridge = OpenCodeBridge::new(tg_client);

    // 此时应该找不到新方案
    let located = bridge.locate_new_plan_file(&temp_dir, start_time);
    assert!(located.is_none(), "在任务启动前创建的旧方案必须被过滤掉");

    // 2. 在 start_time 之后创建全新方案
    let new_plan_dir = plans_dir.join("20261106-new-feature");
    fs::create_dir_all(&new_plan_dir).unwrap();
    let new_plan = new_plan_dir.join("plan.md");
    fs::write(&new_plan, "# New Plan\n全新方案内容").unwrap();

    let located_new = bridge.locate_new_plan_file(&temp_dir, start_time);
    assert!(located_new.is_some(), "在任务启动后创建的新方案必须被正确识别");
    assert_eq!(located_new.unwrap(), new_plan);

    // 清理临时文件
    let _ = fs::remove_dir_all(&temp_dir);
}
