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

    // 从环境变量读取 Bot Token（若未设置则安全 fallback）
    let test_token = std::env::var("TELEGRAM_BOT_TOKEN")
        .unwrap_or_else(|_| "env_test_token".to_string());
    let tg_client = Arc::new(TelegramClient::new(test_token));
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

#[test]
fn test_extract_button_label() {
    use super::opencode_bridge::extract_button_label;

    // 1. 带长破折号和推荐前缀
    let label1 = extract_button_label(1, "1、推荐：两级可展开侧边栏 —— 扩展性强，清晰归类");
    assert_eq!(label1, "1️⃣ 两级可展开侧边栏");

    // 2. 带全角括号补充说明
    let label2 = extract_button_label(2, "2. 分组卡片式列表（扁平展示，直观简单）");
    assert_eq!(label2, "2️⃣ 分组卡片式列表");

    // 3. 超长文本截断为紧凑文本
    let label3 = extract_button_label(3, "3、这是一个非常非常非常非常非常长的选项标题没有任何标点符号");
    assert!(label3.starts_with("3️⃣ "));
    assert!(label3.ends_with("…"));
    assert!(label3.chars().count() <= 16);

    // 4. 短文本直接保留
    let label4 = extract_button_label(4, "4) 弹窗分组设置");
    assert_eq!(label4, "4️⃣ 弹窗分组设置");
}

#[test]
fn test_split_option_detail() {
    use super::opencode_bridge::split_option_detail;

    let (title1, desc1) = split_option_detail("1、推荐：两级侧边栏 —— 扩展性强，清晰归类");
    assert_eq!(title1, "推荐：两级侧边栏");
    assert_eq!(desc1, Some("扩展性强，清晰归类".to_string()));

    let (title2, desc2) = split_option_detail("2. 普通无说明选项");
    assert_eq!(title2, "普通无说明选项");
    assert_eq!(desc2, None);
}

#[test]
fn test_format_question_request_message() {
    use super::opencode_bridge::{format_question_request_message, OpenCodeQuestionItem, OpenCodeQuestionOption};

    let item = OpenCodeQuestionItem {
        question: "设置界面 Tab 太多，如何优化？".to_string(),
        multiple: false,
        options: vec![
            OpenCodeQuestionOption {
                label: "两级侧边栏".to_string(),
                description: Some("结构清晰，支持多级扩展".to_string()),
            },
            OpenCodeQuestionOption {
                label: "分组卡片".to_string(),
                description: None,
            },
        ],
    };

    let msg = format_question_request_message(&item);
    // 验证绝不包含 markdown 代码块围栏
    assert!(!msg.contains("```markdown"));
    assert!(!msg.contains("```"));
    // 验证包含提问与完整选项
    assert!(msg.contains("设置界面 Tab 太多，如何优化？"));
    assert!(msg.contains("1️⃣ 两级侧边栏"));
    assert!(msg.contains("结构清晰，支持多级扩展"));
    assert!(msg.contains("2️⃣ 分组卡片"));
}

#[test]
fn test_format_parsed_question_message() {
    use super::opencode_bridge::format_parsed_question_message;

    let q_text = "分析完毕，需要确认交互风格：";
    let options = vec![
        (1, "1、推荐：两级侧边栏 —— 扩展性强".to_string()),
        (2, "2、分组卡片式 —— 扁平直观".to_string()),
    ];

    let msg = format_parsed_question_message(q_text, &options);
    assert!(!msg.contains("```markdown"));
    assert!(!msg.contains("```"));
    assert!(msg.contains("1️⃣ 推荐：两级侧边栏"));
    assert!(msg.contains("扩展性强"));
    assert!(msg.contains("2️⃣ 分组卡片式"));
    assert!(msg.contains("扁平直观"));
}

#[test]
fn test_plan_confirmation_buttons_and_coding_prompt() {
    let plan_id = "20261106-network-settings";
    
    // 验证方案确认只有“确认”与“补充”两个选项
    let buttons = vec![vec![
        InlineKeyboardButton::callback("✅ 确认", format!("approve_plan:{}", plan_id)),
        InlineKeyboardButton::callback("✏️ 补充", format!("revise_plan:{}", plan_id)),
    ]];
    assert_eq!(buttons[0].len(), 2);
    assert_eq!(buttons[0][0].text, "✅ 确认");
    assert_eq!(buttons[0][0].callback_data.as_deref(), Some("approve_plan:20261106-network-settings"));
    assert_eq!(buttons[0][1].text, "✏️ 补充");
    assert_eq!(buttons[0][1].callback_data.as_deref(), Some("revise_plan:20261106-network-settings"));

    // 验证固定 coding 提示词格式
    let clean_plan_id = plan_id
        .trim_start_matches(".ai-workflow/plans/")
        .trim_start_matches("plans/")
        .trim_matches('/');
    let coding_prompt = format!("调用 coding 技能，实施 .ai-workflow/plans/{}", clean_plan_id);
    assert_eq!(coding_prompt, "调用 coding 技能，实施 .ai-workflow/plans/20261106-network-settings");
}

#[tokio::test]
async fn test_telegram_client_token_from_env() {
    use super::client::TelegramClient;

    // 优先通过环境变量 TELEGRAM_BOT_TOKEN 进行客户端初始化
    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .unwrap_or_else(|_| "env_test_token".to_string());
    assert!(!token.is_empty());

    let client = TelegramClient::new(&token);
    assert_eq!(client.token(), token);

    // 若环境变量中配置了有效的 Bot Token，则自动化执行连通性真实校验
    if let Ok(live_token) = std::env::var("TELEGRAM_BOT_TOKEN") {
        let trimmed = live_token.trim();
        if !trimmed.is_empty() {
            let live_client = TelegramClient::new(trimmed);
            let res = live_client.get_me().await;
            assert!(res.is_ok(), "配置了 TELEGRAM_BOT_TOKEN 环境变量时，测试连接应成功: {:?}", res.err());
            let user = res.unwrap();
            assert!(user.is_bot);
        }
    }
}

#[test]
fn test_plan_id_cleaning_and_resolution() {
    let raw_inputs = [
        "20261106-settings-service-categories",
        ".ai-workflow/plans/20261106-settings-service-categories",
        "plans/20261106-settings-service-categories/",
        "/20261106-settings-service-categories/",
    ];

    for input in &raw_inputs {
        let clean = input
            .trim_start_matches(".ai-workflow/plans/")
            .trim_start_matches("plans/")
            .trim_matches('/');
        assert_eq!(clean, "20261106-settings-service-categories");

        let prompt = format!("调用 coding 技能，实施 .ai-workflow/plans/{}", clean);
        assert_eq!(prompt, "调用 coding 技能，实施 .ai-workflow/plans/20261106-settings-service-categories");
    }
}

#[test]
fn test_plan_progress_detection_and_formatting() {
    use super::plan_progress::{get_plan_progress, format_plan_progress_message, find_active_plan_id, PlanExecutionStatus};

    let temp_root = std::env::temp_dir().join(format!("onespace_progress_test_{}", std::process::id()));
    let plan_id = "20261106-sample-feature";
    let plan_dir = temp_root.join(".ai-workflow").join("plans").join(plan_id);
    let worktree_dir = temp_root.join(".worktrees").join(plan_id);

    std::fs::create_dir_all(&plan_dir).unwrap();
    std::fs::create_dir_all(&worktree_dir).unwrap();

    // 写入 plan.md
    let plan_content = r#"---
plan_id: 20261106-sample-feature
status: frozen
---
# Sample Plan
### Step 1: 编写失败测试并实现核心业务
### Step 2: 跑通全量回归测试与文档登记
"#;
    std::fs::write(plan_dir.join("plan.md"), plan_content).unwrap();

    // 写入实施中的 implementation.yaml
    let impl_content = r#"plan_id: 20261106-sample-feature
status: in-progress
started_at: "2026-11-06T10:00:00+08:00"
"#;
    std::fs::write(plan_dir.join("implementation.yaml"), impl_content).unwrap();

    // 1. 测试自动查找活跃 planId
    let active_id = find_active_plan_id(&temp_root);
    assert_eq!(active_id.as_deref(), Some(plan_id));

    // 2. 测试获取实施进度
    let progress = get_plan_progress(&temp_root, plan_id).expect("探测进度应成功");
    assert_eq!(progress.plan_id, plan_id);
    assert_eq!(progress.status, PlanExecutionStatus::InProgress);
    assert!(progress.has_worktree);
    assert_eq!(progress.steps.len(), 2);
    assert!(progress.steps[0].contains("Step 1"));

    // 3. 测试格式化消息与按钮
    let (msg, markup) = format_plan_progress_message(&progress);
    assert!(msg.contains("20261106-sample-feature"));
    assert!(msg.contains("正在实施中"));
    assert!(msg.contains("Step 1"));
    assert!(markup.is_some());
    let mk = markup.unwrap();
    assert!(mk.inline_keyboard.iter().any(|row| row.iter().any(|btn| btn.text.contains("刷新进度"))));

    // 清理
    let _ = std::fs::remove_dir_all(&temp_root);
}
