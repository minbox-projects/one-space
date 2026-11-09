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

#[test]
fn test_bot_command_and_chat_action_serialization() {
    use super::types::{BotCommand, SetMyCommandsPayload, SendChatActionPayload};

    let cmd1 = BotCommand { command: "plans".to_string(), description: "查看方案".to_string() };
    let cmd2 = BotCommand { command: "diff".to_string(), description: "查看差异".to_string() };
    let payload = SetMyCommandsPayload { commands: vec![cmd1, cmd2] };

    let json = serde_json::to_string(&payload).expect("序列化 setMyCommands payload 成功");
    assert!(json.contains("plans"));
    assert!(json.contains("diff"));

    let action_payload = SendChatActionPayload {
        chat_id: 1234567,
        action: "typing".to_string(),
    };
    let action_json = serde_json::to_string(&action_payload).expect("序列化 sendChatAction payload 成功");
    assert!(action_json.contains("1234567"));
    assert!(action_json.contains("typing"));
}

#[test]
fn test_diff_snippet_truncation() {
    let raw_diff = "diff --git a/file.rs b/file.rs\n+ line 1\n".repeat(100);
    let limit = 1200;
    let snippet = if raw_diff.len() > limit {
        format!("{}\n\n...（差异较大，已截取前 1200 字符）...", &raw_diff[..limit])
    } else {
        raw_diff.clone()
    };

    assert!(snippet.len() > limit);
    assert!(snippet.contains("差异较大，已截取前 1200 字符"));
}

#[test]
fn test_failure_log_tail_extraction() {
    let sample_logs = (1..=50).map(|i| format!("Line {}", i)).collect::<Vec<_>>().join("\n");
    let lines: Vec<&str> = sample_logs.lines().collect();
    let tail_lines = if lines.len() > 30 {
        lines[lines.len() - 30..].to_vec()
    } else {
        lines
    };

    assert_eq!(tail_lines.len(), 30);
    assert_eq!(tail_lines[0], "Line 21");
    assert_eq!(tail_lines[29], "Line 50");
}

#[test]
fn test_card_builder_cards() {
    use super::card_builder::CardBuilder;

    // 1. 欢迎卡片包含原生引用与 Chat ID
    let welcome = CardBuilder::welcome_card(999888, "MacBook-Test.local");
    assert!(welcome.contains("999888"));
    assert!(welcome.contains("> ├─ 管理员 ID: `999888`"));
    assert!(welcome.contains("> └─ 通信状态: `🟢 活跃连接 (Long-Polling)`"));

    // 2. 状态卡片包含树形分支与操作按钮
    let (status, markup) = CardBuilder::status_card(
        "MacBook.local",
        "2026-11-06 17:40:00",
        "onespace-app",
        "main",
        "干净 Clean",
        "a1b2c3d",
        true,
    );
    assert!(status.contains("> ├─ 设备主机: `MacBook.local`"));
    assert!(status.contains("⚡ 自动确认 (Auto-Approve)"));
    assert!(markup.is_some());
    let mk = markup.unwrap();
    assert_eq!(mk.inline_keyboard[0].len(), 2);

    // 3. 规划完成卡片包含方案摘要与确认按钮
    let (review, review_markup) = CardBuilder::plan_review_card(
        "20261106-sample-feature",
        "1. 编写测试\n2. 实施业务",
        Some(2),
    );
    assert!(review.contains("20261106-sample-feature"));
    assert!(review.contains("> 1. 编写测试"));
    assert_eq!(review_markup.inline_keyboard[0].len(), 2);
    assert_eq!(review_markup.inline_keyboard[0][0].text, "✅ 确认启动实施");

    // 4. 四重审核成功卡片包含四重证据项与统计
    let (success, success_mk) = CardBuilder::coding_success_card(
        "20261106-sample-feature",
        Some("3 分 12 秒"),
        "c8d9e0f feat: sample",
        "1 file changed, 10 insertions(+)",
    );
    assert!(success.contains("✅ *四重证据链严格核验全部达标*："));
    assert!(success.contains("交付耗时: `3 分 12 秒`"));
    assert_eq!(success_mk.inline_keyboard[0].len(), 2);
    assert_eq!(success_mk.inline_keyboard[0][0].text, "🔍 查看改动 Diff");
}

#[test]
fn test_card_builder_button_length_safety() {
    use super::card_builder::CardBuilder;

    // 短文本 (7 <= 8) 完整保留，不截断
    let label1 = CardBuilder::format_button_label(Some(1), "1、推荐：两级可展开侧边栏 —— 扩展性强", 8);
    assert_eq!(label1, "1️⃣ 两级可展开侧边栏");
    assert!(label1.chars().count() <= 12);

    // 超长文本截断 (max_chars = 6, 取前 5 字 + 省略号)
    let label2 = CardBuilder::format_button_label(Some(2), "2. 两级可展开侧边栏", 6);
    assert_eq!(label2, "2️⃣ 两级可展开…");

    // 单列按钮超长截断 (超过 14 字，max_chars = 14)
    let label3 = CardBuilder::format_button_label(Some(3), "3. 这是一个非常非常非常非常非常长的选项标题没有任何标点符号", 14);
    assert!(label3.starts_with("3️⃣ "));
    assert!(label3.ends_with("…"));
    assert!(label3.chars().count() <= 18);
}

#[test]
fn test_tier1_config_serialization_and_backward_compatibility() {
    // 1. 默认值验证
    let config = RemoteBotConfig::default();
    assert!(config.enable_natural_dialogue);
    assert!(config.opencode_bin_path.is_none());
    assert!(config.workspace_directories.is_empty());

    // 2. 向后兼容性测试：旧版本 JSON 中缺少第一梯队的新增字段
    let old_json = r#"{
        "enabled": true,
        "bot_token": "123:ABC",
        "allowed_chat_ids": [111, 222],
        "active_workspace": "onespace-app",
        "auto_approve_permissions": false,
        "updated_at": 1700000000
    }"#;

    let deserialized: RemoteBotConfig = serde_json::from_str(old_json).expect("旧配置必须能成功反序列化");
    assert_eq!(deserialized.bot_token, "123:ABC");
    assert_eq!(deserialized.allowed_chat_ids, vec![111, 222]);
    // 缺省时自动补充默认值
    assert!(deserialized.enable_natural_dialogue);
    assert!(deserialized.opencode_bin_path.is_none());
    assert!(deserialized.workspace_directories.is_empty());

    // 3. 完整序列化与反序列化
    let mut new_config = RemoteBotConfig::default();
    new_config.enable_natural_dialogue = false;
    new_config.opencode_bin_path = Some("/opt/homebrew/bin/opencode".to_string());
    new_config.workspace_directories = vec!["/Users/alice/projects/app-a".to_string(), "/Users/alice/projects/app-b".to_string()];

    let serialized = serde_json::to_string(&new_config).expect("新配置序列化必须成功");
    let roundtrip: RemoteBotConfig = serde_json::from_str(&serialized).expect("新配置反序列化必须成功");
    assert!(!roundtrip.enable_natural_dialogue);
    assert_eq!(roundtrip.opencode_bin_path.as_deref(), Some("/opt/homebrew/bin/opencode"));
    assert_eq!(roundtrip.workspace_directories.len(), 2);
    assert_eq!(roundtrip.workspace_directories[0], "/Users/alice/projects/app-a");
}

#[test]
fn test_resolve_executable_path() {
    use super::commands::resolve_executable_path;

    // 1. 普通绝对路径保持不变
    assert_eq!(resolve_executable_path("/usr/local/bin/opencode"), "/usr/local/bin/opencode");

    // 2. 命令别名保持不变
    assert_eq!(resolve_executable_path("opencode"), "opencode");

    // 3. 波浪号展开
    let tilde_path = "~/bin/opencode";
    let resolved = resolve_executable_path(tilde_path);
    if let Some(home) = dirs::home_dir() {
        assert_eq!(resolved, home.join("bin/opencode").to_string_lossy().to_string());
    } else {
        assert_eq!(resolved, tilde_path);
    }
}

#[test]
fn test_resolve_project_dir_multi_workspaces() {
    use super::router::{resolve_project_dir, collect_all_workspaces, collect_featured_projects};
    use std::env;

    let current_dir = env::current_dir().unwrap_or_default();
    let canon_current = std::fs::canonicalize(&current_dir).unwrap_or_else(|_| current_dir.clone());
    let current_str = canon_current.to_string_lossy().to_string();
    let folder_name = canon_current.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

    let mut config = RemoteBotConfig::default();
    config.workspace_directories = vec![current_str.clone()];

    // 1. active_workspace 命中 workspace_directories 中的文件夹名
    if !folder_name.is_empty() {
        config.active_workspace = Some(folder_name.clone());
        let resolved = resolve_project_dir(&config);
        assert_eq!(resolved, canon_current);
    }

    // 2. active_workspace 直接是绝对路径
    config.active_workspace = Some(current_str.clone());
    let resolved = resolve_project_dir(&config);
    assert_eq!(resolved, canon_current);

    // 3. active_workspace 为 None 时，自动使用 workspace_directories 中的第一个有效目录
    config.active_workspace = None;
    let resolved = resolve_project_dir(&config);
    assert_eq!(resolved, canon_current);

    // 4. 验证 collect_all_workspaces 包含当前工作目录
    let all = collect_all_workspaces(&config);
    assert!(all.iter().any(|it| it.path == canon_current));

    // 5. 验证 collect_featured_projects 数量受控（上限不超过 8）
    let featured = collect_featured_projects(&config);
    assert!(featured.len() <= 8);
    assert!(featured.iter().any(|it| it.path == canon_current));
}

#[test]
fn test_projects_card_compact_layout() {
    use super::card_builder::CardBuilder;

    let candidates = vec!["OneSpace", "ShopifyApp", "PaymentService", "AdminDashboard"];
    let (text, markup) = CardBuilder::projects_card("OneSpace", "/Users/tester/onespace-app", &candidates);

    // 验证包含美化引用块与提示信息
    assert!(text.contains("> ├─ 当前活跃: `OneSpace`"));
    assert!(text.contains("💡 *小提示：输入 `/projects <名称>` 可快速搜索全量系统工作空间。*"));

    // 验证按钮排版：当前选中项 OneSpace 独占一行，其余 3 个按双列紧凑排列
    assert!(markup.inline_keyboard.len() >= 2);
    // 第一行是当前选中项
    assert_eq!(markup.inline_keyboard[0].len(), 1);
    assert!(markup.inline_keyboard[0][0].text.contains("OneSpace (当前选中)"));
    // 第二行双列
    assert_eq!(markup.inline_keyboard[1].len(), 2);
}

#[test]
fn test_planning_command_and_dialogue_cards() {
    use super::card_builder::CardBuilder;

    // 1. 帮助与欢迎卡片包含 /planning 指令
    let help = CardBuilder::help_card();
    assert!(help.contains("/planning <需求>"));
    assert!(!help.contains("/newfeat"));

    let welcome = CardBuilder::welcome_card(123456, "MacBook.local");
    assert!(welcome.contains("/planning <需求>"));
    assert!(!welcome.contains("/newfeat"));

    // 2. 显式 /planning 发起的卡片模式
    let (plan_text, _) = CardBuilder::new_feature_ack_card("onespace-app", "支持暗黑主题切换", true);
    assert!(plan_text.contains("已开启需求规划流程 (Planning)"));
    assert!(plan_text.contains("方案调研与规划 (Planning 技能)"));
    assert!(plan_text.contains("📝 *规划目标*："));

    // 3. 普通自然语言对话的卡片模式
    let (chat_text, _) = CardBuilder::new_feature_ack_card("onespace-app", "帮我查一下为什么构建失败", false);
    assert!(chat_text.contains("已接收对话消息"));
    assert!(chat_text.contains("自然对话 (遵循 OpenCode 流程)"));
    assert!(chat_text.contains("📝 *消息内容*："));
}

#[test]
fn test_external_opencode_inspector_and_token_formatting() {
    use super::opencode_inspector::format_token_count;

    assert_eq!(format_token_count(950), "950");
    assert_eq!(format_token_count(1_500), "1.5K");
    assert_eq!(format_token_count(125_000), "125.0K");
    assert_eq!(format_token_count(1_830_000), "1.83M");
    assert_eq!(format_token_count(133_374_080), "133.37M");
}

#[test]
fn test_external_session_cards_rendering() {
    use super::card_builder::CardBuilder;
    use super::types::{ExternalOpenCodeSessionInfo, OpenCodeTodoInfo};

    let session = ExternalOpenCodeSessionInfo {
        pid: 45652,
        ppid: 1234,
        elapsed: "11:34:15".to_string(),
        command: "opencode --yolo".to_string(),
        cwd: "/Users/tester/Work/isp-repos".to_string(),
        project_name: "isp-repos".to_string(),
        session_id: Some("ses_e4f5239b5ffe3ngM8wNMo9E367".to_string()),
        title: Some("本地联调测试剩余项执行与对账".to_string()),
        model: Some("deepseek-v4.1-flash".to_string()),
        agent: Some("build".to_string()),
        is_busy: true,
        current_step: Some(379),
        last_tool: Some("bash".to_string()),
        last_tool_status: Some("completed".to_string()),
        last_tool_input_preview: Some("for p in 17043 77275; do kill $p; done".to_string()),
        last_reasoning: Some("Need stop all. Note the command output exposed...".to_string()),
        todos: vec![
            OpenCodeTodoInfo {
                position: 0,
                status: "completed".to_string(),
                content: "数据中心文档中文重命名".to_string(),
            },
            OpenCodeTodoInfo {
                position: 1,
                status: "in_progress".to_string(),
                content: "清理后台占用端口".to_string(),
            },
        ],
        tokens_input: 1_829_949,
        tokens_output: 136_820,
        tokens_reasoning: 126_397,
        tokens_cache_read: 133_374_080,
        updated_at: Some(1794007043904),
    };

    // 1. 验证单会话卡片渲染
    let (card_text, markup_opt) = CardBuilder::external_session_card(&session);
    assert!(card_text.contains("🚀 *外部 OpenCode 运行中会话*"));
    assert!(card_text.contains("PID 45652"));
    assert!(card_text.contains("opencode --yolo"));
    assert!(card_text.contains("本地联调测试剩余项执行与对账"));
    assert!(card_text.contains("🟢 `正在处理 (Busy)` · Step 379"));
    assert!(card_text.contains("deepseek-v4.1-flash"));
    assert!(card_text.contains("🛠 工具调用: `bash` (状态: `completed`)"));
    assert!(card_text.contains("💭 推理思考: Need stop all."));
    assert!(card_text.contains("▫️ *任务清单 (Todos)*: 1/2 完成"));
    assert!(card_text.contains("✅ 数据中心文档中文重命名"));
    assert!(card_text.contains("▶️ 清理后台占用端口"));
    assert!(card_text.contains("1.83M"));

    let markup = markup_opt.expect("必须包含操作按钮");
    assert_eq!(markup.inline_keyboard.len(), 3);
    assert!(markup.inline_keyboard[0][0].text.contains("刷新进度"));
    assert_eq!(markup.inline_keyboard[0][0].callback_data.as_deref(), Some("refresh_ext_session:45652"));
    assert!(markup.inline_keyboard[0][1].text.contains("发送提示词"));
    assert_eq!(markup.inline_keyboard[0][1].callback_data.as_deref(), Some("prompt_ext_session:45652"));
    assert!(markup.inline_keyboard[2][0].text.contains("完全托管"));
    assert_eq!(markup.inline_keyboard[2][0].callback_data.as_deref(), Some("start_host:45652"));
    assert_eq!(markup.inline_keyboard[2][1].callback_data.as_deref(), Some("switch_ext_ws:45652"));

    // 2. 验证多会话列表卡片
    let session2 = ExternalOpenCodeSessionInfo {
        pid: 88888,
        ppid: 1234,
        elapsed: "05:20".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/other-repo".to_string(),
        project_name: "other-repo".to_string(),
        session_id: Some("ses_123".to_string()),
        title: Some("修复网络路由".to_string()),
        model: Some("claude-3-7-sonnet".to_string()),
        agent: Some("code".to_string()),
        is_busy: false,
        current_step: Some(5),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 10_000,
        tokens_output: 500,
        tokens_reasoning: 0,
        tokens_cache_read: 20_000,
        updated_at: None,
    };

    let (multi_text, multi_markup) = CardBuilder::multiple_external_sessions_card(&[session.clone(), session2]);
    assert!(multi_text.contains("检测到多个终端正在运行 OpenCode 会话"));
    assert!(multi_text.contains("1️⃣ *isp-repos*"));
    assert!(multi_text.contains("2️⃣ *other-repo*"));
    assert_eq!(multi_markup.inline_keyboard.len(), 3); // 2 个会话 + 1 个刷新列表

    // 3. 验证无会话提示卡片
    let (empty_text, empty_markup) = CardBuilder::no_external_session_card("MacBook.local");
    assert!(empty_text.contains("当前未检测到正在运行的外部 OpenCode 会话"));
    assert!(empty_text.contains("MacBook.local"));
    assert!(empty_markup.is_some());

    // 4. 验证详细日志卡片
    let (log_text, log_markup) = CardBuilder::external_session_logs_card(&session);
    assert!(log_text.contains("📝 *外部会话详细动态与思考*"));
    assert!(log_text.contains("工具: bash"));
    assert!(log_text.contains("for p in 17043"));
    assert!(log_markup.inline_keyboard[0][0].text.contains("返回会话进度"));

    // 5. 验证主机状态卡片带外部会话探测
    let (status_text_with_sess, status_markup) = CardBuilder::status_card_with_sessions(
        "MacBook.local",
        "2026-11-07 07:00:00",
        "OneSpace",
        "main",
        "Clean",
        "abc1234 - update",
        true,
        1,
    );
    assert!(status_text_with_sess.contains("终端 OpenCode: `🚀 1 个会话运行中`"));
    let btns = status_markup.unwrap().inline_keyboard[0].clone();
    assert!(btns.iter().any(|b| b.text.contains("运行中会话")));
}

#[test]
fn test_prompt_input_and_dispatch_cards() {
    use super::card_builder::CardBuilder;
    use super::types::{ExternalOpenCodeSessionInfo, PromptTargetSession};

    let session = ExternalOpenCodeSessionInfo {
        pid: 54321,
        ppid: 1000,
        elapsed: "10:00".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/my-project".to_string(),
        project_name: "my-project".to_string(),
        session_id: Some("ses_abc123".to_string()),
        title: Some("重构认证中间件".to_string()),
        model: Some("claude-3-7-sonnet".to_string()),
        agent: Some("code".to_string()),
        is_busy: false,
        current_step: Some(12),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 1000,
        tokens_output: 100,
        tokens_reasoning: 0,
        tokens_cache_read: 2000,
        updated_at: None,
    };

    // 1. 测试提示词输入请求引导卡片
    let (req_text, markup) = CardBuilder::prompt_input_request_card(&session);
    assert!(req_text.contains("向正在运行的 OpenCode 会话发送提示词"));
    assert!(req_text.contains("my-project"));
    assert!(req_text.contains("PID: 54321"));
    assert!(req_text.contains("重构认证中间件"));
    assert!(req_text.contains("ses_abc123"));
    assert_eq!(markup.inline_keyboard.len(), 1);
    assert_eq!(markup.inline_keyboard[0].len(), 2);
    assert_eq!(markup.inline_keyboard[0][0].text, "❌ 取消发送");
    assert_eq!(markup.inline_keyboard[0][0].callback_data.as_deref(), Some("cancel_prompt_target"));
    assert_eq!(markup.inline_keyboard[0][1].text, "⬅️ 返回进度卡片");
    assert_eq!(markup.inline_keyboard[0][1].callback_data.as_deref(), Some("select_ext_session:54321"));

    // 2. 测试提示词投递确认卡片
    let target = PromptTargetSession {
        pid: 54321,
        session_id: "ses_abc123".to_string(),
        cwd: "/Users/tester/Work/my-project".to_string(),
        project_name: "my-project".to_string(),
    };
    let ack_text = CardBuilder::prompt_dispatched_ack_card(&target, "请增加针对密码强度的校验逻辑并跑通测试");
    assert!(ack_text.contains("提示词已投递给 OpenCode 会话"));
    assert!(ack_text.contains("`my-project` (PID: 54321)"));
    assert!(ack_text.contains("ses_abc123"));
    assert!(ack_text.contains("请增加针对密码强度的校验逻辑并跑通测试"));
}

#[test]
fn test_filter_idle_and_subagent_sessions() {
    use super::types::ExternalOpenCodeSessionInfo;

    // 1. 主代理会话 - 正在进行 (Busy) -> 应该被保留
    let main_busy = ExternalOpenCodeSessionInfo {
        pid: 101,
        ppid: 1,
        elapsed: "01:00".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/main-repo".to_string(),
        project_name: "main-repo".to_string(),
        session_id: Some("ses_main_1".to_string()),
        title: Some("实施主功能模块开发".to_string()),
        model: Some("claude-3-7-sonnet".to_string()),
        agent: Some("code".to_string()),
        is_busy: true,
        current_step: Some(10),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 100,
        tokens_output: 100,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        updated_at: Some(1000),
    };

    // 2. 主代理会话 - 已空闲 (Idle) -> 应该被过滤掉
    let main_idle = ExternalOpenCodeSessionInfo {
        pid: 102,
        ppid: 1,
        elapsed: "10:00".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/idle-repo".to_string(),
        project_name: "idle-repo".to_string(),
        session_id: Some("ses_main_2".to_string()),
        title: Some("已完工任务".to_string()),
        model: Some("claude-3-7-sonnet".to_string()),
        agent: Some("code".to_string()),
        is_busy: false,
        current_step: Some(20),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 100,
        tokens_output: 100,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        updated_at: Some(900),
    };

    // 3. 子代理会话 (subagent) - 正在进行 (Busy) -> 应该被过滤掉（仅允许主代理）
    let subagent_busy = ExternalOpenCodeSessionInfo {
        pid: 103,
        ppid: 101,
        elapsed: "00:30".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/main-repo".to_string(),
        project_name: "main-repo".to_string(),
        session_id: Some("ses_sub_1".to_string()),
        title: Some("Commit root workspace changes (@git-operator subagent)".to_string()),
        model: Some("claude-3-7-sonnet".to_string()),
        agent: Some("git-operator".to_string()),
        is_busy: true,
        current_step: Some(2),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 50,
        tokens_output: 50,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        updated_at: Some(1010),
    };

    // 4. 无有效 session_id 的悬空进程 -> 应该被过滤掉
    let no_session_busy = ExternalOpenCodeSessionInfo {
        pid: 104,
        ppid: 1,
        elapsed: "00:05".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/temp-repo".to_string(),
        project_name: "temp-repo".to_string(),
        session_id: None,
        title: None,
        model: None,
        agent: None,
        is_busy: true,
        current_step: None,
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 0,
        tokens_output: 0,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        updated_at: None,
    };

    let mut sessions = vec![main_busy, main_idle, subagent_busy, no_session_busy];

    // 执行同款过滤规则
    sessions.retain(|sess| sess.is_busy);
    sessions.retain(|sess| {
        if sess.session_id.is_none() {
            return false;
        }
        if let Some(ref t) = sess.title {
            if t.contains("(@") && t.contains("subagent)") {
                return false;
            }
        }
        true
    });

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, 101);
    assert_eq!(sessions[0].session_id.as_deref(), Some("ses_main_1"));
    assert_eq!(sessions[0].title.as_deref(), Some("实施主功能模块开发"));
}

#[test]
fn test_session_hosting_cards_rendering() {
    use super::card_builder::CardBuilder;
    use super::types::{HostedOptionItem, SessionHostingDecisionMode};

    // 1. 验证启动完全托管卡片
    let (started_text, started_markup) = CardBuilder::hosted_session_started_card(
        "isp-repos",
        45652,
        "ses_e4f5239b5ffe",
        &SessionHostingDecisionMode::Manual,
    );
    assert!(started_text.contains("会话已成功交由 OneSpace + Telegram 完全托管"));
    assert!(started_text.contains("isp-repos"));
    assert!(started_text.contains("PID: 45652"));
    assert!(started_text.contains("ses_e4f5239b5ffe"));
    assert!(started_text.contains("人工决断优先 (Manual)"));
    assert_eq!(started_markup.inline_keyboard[0][0].callback_data.as_deref(), Some("toggle_host_mode:ses_e4f5239b5ffe"));
    assert_eq!(started_markup.inline_keyboard[0][1].callback_data.as_deref(), Some("stop_host:ses_e4f5239b5ffe"));

    // 2. 验证待办完成通知
    let notice = CardBuilder::hosted_todo_completed_notice("isp-repos", "优化数据库索引并迁移", 3, 5);
    assert!(notice.contains("任务待办已达成"));
    assert!(notice.contains("isp-repos"));
    assert!(notice.contains("优化数据库索引并迁移"));
    assert!(notice.contains("`3/5` 项已完成"));

    // 3. 验证提问与决策卡片
    let options = vec![
        HostedOptionItem {
            index: 1,
            label: "方案 A：采用 JWT 认证中间件 (Recommended)".to_string(),
            description: Some("轻量无状态，扩展性好".to_string()),
            is_recommended: true,
        },
        HostedOptionItem {
            index: 2,
            label: "方案 B：采用 Redis Session".to_string(),
            description: Some("集中式会话，可主动吊销".to_string()),
            is_recommended: false,
        },
    ];
    let (q_text, q_markup) = CardBuilder::hosted_question_card(
        "isp-repos",
        "ses_e4f5239b5ffe",
        "请确认接下来采用的身份认证方案：",
        &options,
    );
    assert!(q_text.contains("OpenCode 提问与决策确认"));
    assert!(q_text.contains("请确认接下来采用的身份认证方案"));
    assert!(q_text.contains("方案 A：采用 JWT 认证中间件"));
    assert_eq!(q_markup.inline_keyboard.len(), 3);
    assert_eq!(q_markup.inline_keyboard[0][0].callback_data.as_deref(), Some("host_ans:ses_e4f5239b5ffe:1"));
    assert_eq!(q_markup.inline_keyboard[1][0].callback_data.as_deref(), Some("host_ans:ses_e4f5239b5ffe:2"));
    assert_eq!(q_markup.inline_keyboard[2][0].callback_data.as_deref(), Some("stop_host:ses_e4f5239b5ffe"));

    // 4. 验证托管自主决策通知
    let auto_text = CardBuilder::hosted_auto_decision_card(
        "isp-repos",
        "请选择数据库驱动",
        "方案 A (Recommended)",
    );
    assert!(auto_text.contains("托管决策代执行 (Auto-Approve)"));
    assert!(auto_text.contains("方案 A (Recommended)"));

    // 5. 验证托管会话圆满交付卡片
    let (done_text, done_markup) = CardBuilder::hosted_session_completed_card(
        "isp-repos",
        45652,
        "ses_e4f5239b5ffe",
        "5 分 12 秒",
        18,
        150_000,
        12_000,
    );
    assert!(done_text.contains("外部 OpenCode 会话圆满交付"));
    assert!(done_text.contains("交付耗时: `5 分 12 秒`"));
    assert!(done_text.contains("总执行步数: `Step 18`"));
    assert!(done_text.contains("150.0K"));
    assert_eq!(done_markup.inline_keyboard[0][0].callback_data.as_deref(), Some("diff_ext_session:45652"));
}

#[test]
fn test_opencode_process_filter_and_project_detection() {
    use super::opencode_inspector::{detect_meaningful_project_name, is_opencode_executable_process};

    // 1. 过滤 serve / daemon 服务守护进程
    assert!(!is_opencode_executable_process("opencode serve --print-logs --port 0 --hostname 127.0.0.1"));
    assert!(!is_opencode_executable_process("/Users/yuqiyu/.opencode/bin/opencode serve --hostname 127.0.0.1 --port 4101"));
    assert!(!is_opencode_executable_process("opencode daemon --start"));
    assert!(!is_opencode_executable_process("/opt/opencode/opencode --port 8080 --hostname 0.0.0.0"));
    assert!(!is_opencode_executable_process("grep opencode"));
    assert!(!is_opencode_executable_process("node runner.js"));

    // 2. 认可合法前台/批处理交互式任务
    assert!(is_opencode_executable_process("opencode --yolo"));
    assert!(is_opencode_executable_process("/usr/local/bin/opencode -m gpt-4o"));
    assert!(is_opencode_executable_process("opencode run 'fix bugs'"));

    // 3. 有意义项目名称推导：子目录优先解析到 Git 仓库根目录
    let cwd_sub = env!("CARGO_MANIFEST_DIR"); // .../onespace-app/src-tauri
    let proj_name = detect_meaningful_project_name(cwd_sub);
    assert_eq!(proj_name, "onespace-app");
}

#[test]
fn test_session_deduplication() {
    use super::types::ExternalOpenCodeSessionInfo;

    let s1 = ExternalOpenCodeSessionInfo {
        pid: 101,
        ppid: 1,
        elapsed: "01:00".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/main-repo".to_string(),
        project_name: "main-repo".to_string(),
        session_id: Some("ses_dup".to_string()),
        title: Some("主功能开发".to_string()),
        model: Some("claude-3-7-sonnet".to_string()),
        agent: Some("code".to_string()),
        is_busy: true,
        current_step: Some(10),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 100,
        tokens_output: 100,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        updated_at: Some(2000),
    };

    let s2 = ExternalOpenCodeSessionInfo {
        pid: 102,
        ppid: 1,
        elapsed: "00:10".to_string(),
        command: "opencode".to_string(),
        cwd: "/Users/tester/Work/main-repo".to_string(),
        project_name: "main-repo".to_string(),
        session_id: Some("ses_dup".to_string()),
        title: Some("主功能开发".to_string()),
        model: Some("claude-3-7-sonnet".to_string()),
        agent: Some("code".to_string()),
        is_busy: true,
        current_step: Some(10),
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 100,
        tokens_output: 100,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        updated_at: Some(1000),
    };

    let mut sessions = vec![s1, s2];

    sessions.sort_by(|a, b| {
        b.updated_at
            .unwrap_or_default()
            .cmp(&a.updated_at.unwrap_or_default())
            .then_with(|| b.pid.cmp(&a.pid))
    });

    let mut seen_sids = std::collections::HashSet::new();
    sessions.retain(|sess| {
        if let Some(ref sid) = sess.session_id {
            seen_sids.insert(sid.clone())
        } else {
            true
        }
    });

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pid, 101);
}

#[test]
fn test_opencode_v2_question_and_permission_deserialization() {
    use super::opencode_bridge::{OpenCodePermissionRequest, OpenCodeQuestionRequest};

    // OpenCode v2 风格的 question JSON（包含 header, requestId, sessionId, title）
    let v2_question_json = r#"[
        {
            "requestId": "req-v2-001",
            "sessionId": "ses_v2_abc",
            "questions": [
                {
                    "header": "请选择构建目标",
                    "multiple": false,
                    "options": [
                        {"title": "macOS aarch64", "description": "Apple Silicon"},
                        {"title": "linux x86_64", "description": "标准 Linux"}
                    ]
                }
            ]
        }
    ]"#;

    let questions: Vec<OpenCodeQuestionRequest> =
        serde_json::from_str(v2_question_json).expect("必须成功解析 OpenCode v2 问题格式");
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].id, "req-v2-001");
    assert_eq!(questions[0].session_id.as_deref(), Some("ses_v2_abc"));
    assert_eq!(questions[0].questions[0].question, "请选择构建目标");
    assert_eq!(questions[0].questions[0].options[0].label, "macOS aarch64");

    // OpenCode v2 风格的 permission JSON
    let v2_permission_json = r#"[
        {
            "requestId": "perm-v2-001",
            "sessionId": "ses_v2_abc",
            "permission": "bash",
            "command": "npm run build"
        }
    ]"#;

    let perms: Vec<OpenCodePermissionRequest> =
        serde_json::from_str(v2_permission_json).expect("必须成功解析 OpenCode v2 权限格式");
    assert_eq!(perms.len(), 1);
    assert_eq!(perms[0].id, "perm-v2-001");
    assert_eq!(perms[0].session_id.as_deref(), Some("ses_v2_abc"));
    assert_eq!(perms[0].command.as_deref(), Some("npm run build"));
}

#[test]
fn test_opencode_reply_payloads_serialization() {
    use super::opencode_bridge::{PermissionReplyPayload, QuestionReplyPayload};

    let q_payload = QuestionReplyPayload {
        answers: vec![vec!["JWT 无状态".to_string()]],
        answer: Some("JWT 无状态".to_string()),
    };
    let q_json = serde_json::to_string(&q_payload).expect("必须序列化成功");
    assert!(q_json.contains("answers"));
    assert!(q_json.contains("answer"));

    let p_payload = PermissionReplyPayload {
        reply: "once".to_string(),
        response: Some("once".to_string()),
    };
    let p_json = serde_json::to_string(&p_payload).expect("必须序列化成功");
    assert!(p_json.contains("once"));
    assert!(p_json.contains("response"));
}

#[tokio::test]
async fn test_remote_bot_save_config_guard_requires_opencode_version() {
    use super::commands::remote_bot_save_config;
    use super::types::RemoteBotConfig;

    let mut invalid_config = RemoteBotConfig::default();
    invalid_config.enabled = true;
    invalid_config.opencode_bin_path = None;

    let res = remote_bot_save_config(invalid_config).await;
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(err.contains("必须选择一个具体的 OpenCode 版本"));
}

#[test]
fn test_opencode_v2_api_envelope_deserialization() {
    use super::opencode_bridge::{ApiEnvelope, OpenCodeQuestionRequest, OpenCodePermissionRequest};

    // 1. v1 传统裸数组格式
    let raw_v1_json = r#"[{"id": "q-v1", "questions": []}]"#;
    let v1_res: ApiEnvelope<OpenCodeQuestionRequest> = serde_json::from_str(raw_v1_json).expect("必须成功解析裸数组");
    let v1_list = v1_res.into_vec();
    assert_eq!(v1_list.len(), 1);
    assert_eq!(v1_list[0].id, "q-v1");

    // 2. v2 对象包装格式 {"location": {...}, "data": [...]}
    let raw_v2_json = r#"{
        "location": {"directory": "/tmp/test"},
        "data": [
            {
                "id": "form-v2-1",
                "sessionId": "ses_v2_test",
                "questions": [
                    {
                        "title": "选择模式",
                        "options": [{"title": "标准模式"}]
                    }
                ]
            }
        ]
    }"#;
    let v2_res: ApiEnvelope<OpenCodeQuestionRequest> = serde_json::from_str(raw_v2_json).expect("必须成功解析 v2 data 对象包装");
    let v2_list = v2_res.into_vec();
    assert_eq!(v2_list.len(), 1);
    assert_eq!(v2_list[0].id, "form-v2-1");
    assert_eq!(v2_list[0].session_id.as_deref(), Some("ses_v2_test"));
    assert_eq!(v2_list[0].questions[0].question, "选择模式");
    assert_eq!(v2_list[0].questions[0].options[0].label, "标准模式");

    // 3. v2 权限包装格式
    let raw_v2_perm = r#"{
        "location": {"directory": "/tmp/test"},
        "data": [
            {
                "requestId": "perm-v2-99",
                "sessionId": "ses_v2_test",
                "permission": "bash",
                "command": "cargo test"
            }
        ]
    }"#;
    let v2_perm_res: ApiEnvelope<OpenCodePermissionRequest> = serde_json::from_str(raw_v2_perm).expect("必须成功解析 v2 perm 包装");
    let v2_perm_list = v2_perm_res.into_vec();
    assert_eq!(v2_perm_list.len(), 1);
    assert_eq!(v2_perm_list[0].id, "perm-v2-99");
    assert_eq!(v2_perm_list[0].session_id.as_deref(), Some("ses_v2_test"));
    assert_eq!(v2_perm_list[0].command.as_deref(), Some("cargo test"));
}

#[tokio::test]
async fn test_opencode_v2_real_env_e2e() {
    use std::path::Path;
    use std::sync::Arc;
    use super::client::TelegramClient;
    use super::opencode_bridge::{get_opencode_major_version, OpenCodeBridge, ApiEnvelope, OpenCodeQuestionRequest, OPENCODE_INTERNAL_AUTH};

    let v2_bin_path = "/private/tmp/test-opencode-v2/node_modules/.bin/opencode";
    if !Path::new(v2_bin_path).exists() {
        println!("跳过真实 v2 E2E 测试：{} 不存在", v2_bin_path);
        return;
    }

    // 1. 验证版本检测逻辑
    let major_ver = get_opencode_major_version(v2_bin_path);
    assert_eq!(major_ver, 2, "真实 OpenCode v2 的主版本号必须为 2");

    // 2. 将临时配置指向该真实 v2 二进制
    let mut config = super::storage::load_config().unwrap_or_default();
    let original_path = config.opencode_bin_path.clone();
    config.opencode_bin_path = Some(v2_bin_path.to_string());
    let _ = super::storage::save_config(&config);

    // 3. 构造 bridge 并真实启动 opencode serve
    let tg_client = Arc::new(TelegramClient::new("dummy-test-token".to_string()));
    let bridge = Arc::new(OpenCodeBridge::new(tg_client));

    let server_url_res = bridge.ensure_server().await;
    assert!(server_url_res.is_ok(), "ensure_server 必须成功启动真实 v2 服务并解析监听端口: {:?}", server_url_res.err());
    let server_url = server_url_res.unwrap();
    assert!(server_url.starts_with("http://127.0.0.1:"), "监听地址必须为 loopback: {}", server_url);

    // 4. HTTP client 携带 Basic Auth 访问 /api/info
    let client = reqwest::Client::new();
    let info_resp = client.get(format!("{}/api/info", server_url))
        .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
        .send().await
        .expect("请求真实 v2 /api/info 必须连通");
    assert_eq!(info_resp.status(), 200, "/api/info 必须返回 200 OK");
    let info_json: serde_json::Value = info_resp.json().await.expect("必须解析 /api/info 为 JSON");
    let ver_str = info_json.get("version").and_then(|v| v.as_str()).unwrap_or("");
    assert!(ver_str.starts_with("2."), "版本号必须以 2. 开头: {}", ver_str);

    // 5. 真实访问 /api/form 和 /api/permission/request，验证 ApiEnvelope 解析
    let form_resp = client.get(format!("{}/api/form", server_url))
        .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
        .send().await
        .expect("请求 /api/form 连通");
    assert_eq!(form_resp.status(), 200);
    let form_env: ApiEnvelope<OpenCodeQuestionRequest> = form_resp.json().await.expect("必须能通过 ApiEnvelope 解析 /api/form");
    let _ = form_env.into_vec();

    // 6. 清理现场
    bridge.cancel_active_task().await;
    config.opencode_bin_path = original_path;
    let _ = super::storage::save_config(&config);
}

#[tokio::test]
async fn test_opencode_v1_real_env_e2e() {
    use std::sync::Arc;
    use super::client::TelegramClient;
    use super::opencode_bridge::{get_opencode_major_version, OpenCodeBridge, ApiEnvelope, OpenCodeQuestionRequest, OpenCodePermissionRequest, OPENCODE_INTERNAL_AUTH};

    // 优先使用本机默认安装的 OpenCode v1（如 ~/.opencode/bin/opencode）
    let v1_bin_candidates = vec![
        "/Users/yuqiyu/.opencode/bin/opencode".to_string(),
        "opencode".to_string(),
    ];

    let mut found_v1_bin = None;
    for cand in v1_bin_candidates {
        if let Ok(output) = std::process::Command::new(&cand).arg("--version").output() {
            let ver = String::from_utf8_lossy(&output.stdout);
            if ver.contains("1.") || ver.contains("v1.") {
                found_v1_bin = Some(cand);
                break;
            }
        }
    }

    let v1_bin_path = match found_v1_bin {
        Some(bin) => bin,
        None => {
            println!("跳过真实 v1 E2E 测试：未找到可用的 OpenCode v1");
            return;
        }
    };

    // 1. 验证版本检测逻辑必须判定为 1
    let major_ver = get_opencode_major_version(&v1_bin_path);
    assert_eq!(major_ver, 1, "真实 OpenCode v1 的主版本号必须为 1");

    // 2. 将临时配置指向该真实 v1 二进制
    let mut config = super::storage::load_config().unwrap_or_default();
    let original_path = config.opencode_bin_path.clone();
    config.opencode_bin_path = Some(v1_bin_path.clone());
    let _ = super::storage::save_config(&config);

    // 3. 构造 bridge 并真实启动 opencode serve
    let tg_client = Arc::new(TelegramClient::new("dummy-test-token".to_string()));
    let bridge = Arc::new(OpenCodeBridge::new(tg_client));

    let server_url_res = bridge.ensure_server().await;
    assert!(server_url_res.is_ok(), "ensure_server 必须成功启动真实 v1 服务并解析监听端口: {:?}", server_url_res.err());
    let server_url = server_url_res.unwrap();
    assert!(server_url.starts_with("http://127.0.0.1:"), "监听地址必须为 loopback: {}", server_url);

    // 4. HTTP client 访问 /session 探测存活性
    let client = reqwest::Client::new();
    let session_resp = client.get(format!("{}/session", server_url))
        .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
        .send().await
        .expect("请求真实 v1 /session 必须连通");
    assert_eq!(session_resp.status(), 200, "/session 必须返回 200 OK");

    // 5. 真实访问 /question 和 /permission，验证 ApiEnvelope 兼容裸数组格式
    let q_resp = client.get(format!("{}/question", server_url))
        .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
        .send().await
        .expect("请求真实 v1 /question 连通");
    assert_eq!(q_resp.status(), 200);
    let q_env: ApiEnvelope<OpenCodeQuestionRequest> = q_resp.json().await.expect("必须能通过 ApiEnvelope 解析 v1 裸数组 /question");
    let _ = q_env.into_vec();

    let p_resp = client.get(format!("{}/permission", server_url))
        .basic_auth("opencode", Some(OPENCODE_INTERNAL_AUTH))
        .send().await
        .expect("请求真实 v1 /permission 连通");
    assert_eq!(p_resp.status(), 200);
    let p_env: ApiEnvelope<OpenCodePermissionRequest> = p_resp.json().await.expect("必须能通过 ApiEnvelope 解析 v1 裸数组 /permission");
    let _ = p_env.into_vec();

    // 6. 清理现场
    bridge.cancel_active_task().await;
    config.opencode_bin_path = original_path;
    let _ = super::storage::save_config(&config);
}

#[test]
fn test_attached_v1_run_command_carries_internal_server_password() {
    let mut cmd = tokio::process::Command::new("opencode");
    super::opencode_bridge::apply_run_connection(&mut cmd, false, "http://127.0.0.1:4096", std::path::Path::new("/tmp/onespace-project"));
    let std_cmd = cmd.as_std();
    assert!(std_cmd.get_args().any(|a| a == "--attach"));
    assert!(
        std_cmd.get_envs().any(|(k, v)| k == "OPENCODE_SERVER_PASSWORD" && v == Some(std::ffi::OsStr::new(super::opencode_bridge::OPENCODE_INTERNAL_AUTH))),
        "the v1 attached run must authenticate to the password-protected internal OpenCode server"
    );
}

fn make_external(session_id: Option<&str>, pid: u32, is_busy: bool) -> super::types::ExternalOpenCodeSessionInfo {
    super::types::ExternalOpenCodeSessionInfo {
        pid,
        ppid: 1,
        elapsed: "01:00".to_string(),
        command: "opencode".to_string(),
        cwd: format!("/tmp/proj-{pid}"),
        project_name: format!("proj-{pid}"),
        session_id: session_id.map(|s| s.to_string()),
        title: Some("Work".to_string()),
        model: None,
        agent: None,
        is_busy,
        current_step: None,
        last_tool: None,
        last_tool_status: None,
        last_tool_input_preview: None,
        last_reasoning: None,
        todos: Vec::new(),
        tokens_input: 0,
        tokens_output: 0,
        tokens_reasoning: 0,
        tokens_cache_read: 0,
        updated_at: None,
    }
}

fn make_hosted(session_id: &str, pid: u32, project_name: &str) -> super::types::HostedSessionInfo {
    super::types::HostedSessionInfo {
        session_id: session_id.to_string(),
        pid,
        cwd: "/tmp/hosted".to_string(),
        project_name: project_name.to_string(),
        chat_id: 1,
        mode: super::types::SessionHostingDecisionMode::Manual,
        started_at: 0,
        last_observed_step: 0,
        completed_todos_count: 0,
        total_todos_count: 0,
        active_question_summary: None,
    }
}

#[test]
fn session_inventory_external_terminal_entries() {
    use super::session_inventory::{
        build_session_entries, SessionEntryKind, SessionInventorySources,
        SESSION_ID_UNAVAILABLE,
    };
    let sources = SessionInventorySources {
        external: vec![
            make_external(Some("ses_busy"), 111, true),
            make_external(Some("ses_idle"), 222, false),
            make_external(None, 333, true),
        ],
        ..Default::default()
    };
    let entries = build_session_entries(&sources);
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|e| e.primary_kind == SessionEntryKind::Terminal));
    let busy = entries.iter().find(|e| e.session_id.as_deref() == Some("ses_busy")).unwrap();
    assert_eq!(busy.status_label, "Busy");
    assert!(busy.is_busy);
    let idle = entries.iter().find(|e| e.session_id.as_deref() == Some("ses_idle")).unwrap();
    assert!(!idle.is_busy);
    assert_eq!(idle.status_label, "Idle");
    let no_id = entries.iter().find(|e| e.pid == Some(333)).unwrap();
    assert!(no_id.session_id.is_none());
    assert_eq!(no_id.session_id_marker, SESSION_ID_UNAVAILABLE);
}

#[test]
fn session_inventory_hosted_dedup() {
    use super::session_inventory::{
        build_session_entries, SessionEntryKind, SessionInventorySources,
    };
    let sources = SessionInventorySources {
        external: vec![make_external(Some("ses_hosted"), 444, true)],
        hosted: vec![make_hosted("ses_hosted", 444, "hosted-proj")],
        ..Default::default()
    };
    let entries = build_session_entries(&sources);
    let hosted_entries: Vec<_> = entries.iter().filter(|e| e.session_id.as_deref() == Some("ses_hosted")).collect();
    assert_eq!(hosted_entries.len(), 1);
    let entry = hosted_entries[0];
    assert_eq!(entry.primary_kind, SessionEntryKind::Hosted);
    assert!(entry.kinds.contains(&SessionEntryKind::Hosted));
    assert!(entry.kinds.contains(&SessionEntryKind::Terminal));
    assert_eq!(entry.project_name, "hosted-proj");
}

#[test]
fn session_inventory_conversation_and_plan() {
    use super::session_inventory::{
        build_session_entries, ConversationSessionState, PlanExecutionState,
        SessionEntryKind, SessionInventorySources, SESSION_ID_CAPTURING,
    };
    let sources = SessionInventorySources {
        conversation: Some(ConversationSessionState {
            session_id: Some("ses_conv".to_string()),
            project_name: Some("conv-proj".to_string()),
            is_busy: true,
        }),
        plan_execution: Some(PlanExecutionState {
            plan_id: "plan-1".to_string(),
            session_id: None,
            project_name: "plan-proj".to_string(),
            is_busy: true,
        }),
        ..Default::default()
    };
    let entries = build_session_entries(&sources);
    let conv = entries.iter().find(|e| e.primary_kind == SessionEntryKind::TelegramConversation).unwrap();
    assert_eq!(conv.session_id_marker, "ses_conv");
    let plan = entries.iter().find(|e| e.primary_kind == SessionEntryKind::PlanExecution).unwrap();
    assert_eq!(plan.plan_id.as_deref(), Some("plan-1"));
    assert_eq!(plan.session_id_marker, SESSION_ID_CAPTURING);

    let rebuilt = SessionInventorySources {
        conversation: sources.conversation.clone(),
        plan_execution: Some(PlanExecutionState {
            plan_id: "plan-1".to_string(),
            session_id: Some("ses_plan".to_string()),
            project_name: "plan-proj".to_string(),
            is_busy: true,
        }),
        ..Default::default()
    };
    let entries2 = build_session_entries(&rebuilt);
    let plans: Vec<_> = entries2.iter().filter(|e| e.plan_id.as_deref() == Some("plan-1")).collect();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].session_id_marker, "ses_plan");
}

#[test]
fn session_inventory_conversation_unassigned() {
    use super::session_inventory::{
        build_session_entries, ConversationSessionState, SessionInventorySources,
        SESSION_ID_UNASSIGNED,
    };
    let sources = SessionInventorySources {
        conversation: Some(ConversationSessionState {
            session_id: None,
            project_name: Some("conv-proj".to_string()),
            is_busy: false,
        }),
        ..Default::default()
    };
    let entries = build_session_entries(&sources);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].session_id_marker, SESSION_ID_UNASSIGNED);
}

#[test]
fn session_inventory_empty() {
    use super::session_inventory::{build_session_entries, SessionInventorySources};
    let entries = build_session_entries(&SessionInventorySources::default());
    assert!(entries.is_empty());
}

#[test]
fn session_inventory_is_subagent() {
    use super::session_inventory::is_subagent_session;
    assert!(is_subagent_session(Some("Normal title"), Some("ses_parent")));
    assert!(is_subagent_session(Some("Do work (@git-operator subagent)"), Some("")));
    assert!(is_subagent_session(Some("Do work (@git-operator subagent)"), None));
    assert!(!is_subagent_session(Some("Normal title"), Some("")));
    assert!(!is_subagent_session(Some("Normal title"), None));
}

fn make_entry(
    primary_kind: super::session_inventory::SessionEntryKind,
    session_id: Option<&str>,
    marker: &str,
    plan_id: Option<&str>,
    pid: Option<u32>,
    project_name: &str,
    status_label: &str,
) -> super::session_inventory::SessionEntry {
    super::session_inventory::SessionEntry {
        primary_kind,
        kinds: vec![primary_kind],
        session_id: session_id.map(|s| s.to_string()),
        session_id_marker: marker.to_string(),
        plan_id: plan_id.map(|s| s.to_string()),
        pid,
        project_name: project_name.to_string(),
        status_label: status_label.to_string(),
        is_busy: status_label == "Busy" || status_label == "Running",
    }
}

#[test]
fn session_inventory_overview_empty() {
    use super::session_inventory::format_sessions_overview;
    let (text, _markup) = format_sessions_overview(&[]);
    assert!(text.contains("当前没有活跃会话"));
    assert!(text.contains("/sessions"));
    assert!(!text.contains("ses_"));
    assert!(!text.contains("PID:"));
    assert!(!text.contains("Plan:"));
}

#[test]
fn session_inventory_overview_mixed() {
    use super::session_inventory::{
        format_sessions_overview, SessionEntryKind,
    };
    let entries = vec![
        make_entry(SessionEntryKind::Terminal, Some("ses_term"), "ses_term", None, Some(111), "proj-term", "Busy"),
        make_entry(SessionEntryKind::Hosted, Some("ses_host"), "ses_host", None, Some(222), "proj-host", "Hosted"),
        make_entry(SessionEntryKind::TelegramConversation, Some("ses_conv"), "ses_conv", None, None, "proj-conv", "Idle"),
        make_entry(SessionEntryKind::PlanExecution, Some("ses_plan"), "ses_plan", Some("20261109-plan-x"), None, "proj-plan", "Running"),
        make_entry(SessionEntryKind::Terminal, None, "unavailable", None, Some(333), "proj-noid", "Idle"),
    ];
    let (text, _markup) = format_sessions_overview(&entries);
    for label in ["Terminal", "Hosted", "Conversation", "Plan Execution"] {
        assert!(text.contains(label), "missing group label {label}: {text}");
    }
    for expected in ["proj-term", "ses_term", "111", "Busy", "proj-host", "ses_host", "222", "Hosted", "proj-conv", "ses_conv", "Idle", "proj-plan", "ses_plan", "20261109-plan-x", "Running", "proj-noid", "unavailable", "333"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    let only_terminal = vec![make_entry(SessionEntryKind::Terminal, Some("ses_only"), "ses_only", None, Some(999), "proj-only", "Busy")];
    let (text2, _markup2) = format_sessions_overview(&only_terminal);
    assert!(text2.contains("Terminal"));
    assert!(text2.contains("proj-only"));
    assert!(text2.contains("ses_only"));
    assert!(!text2.contains("Hosted"), "empty Hosted group must be omitted: {text2}");
    assert!(!text2.contains("Conversation"), "empty Conversation group must be omitted: {text2}");
    assert!(!text2.contains("Plan Execution"), "empty Plan Execution group must be omitted: {text2}");
}

#[test]
fn session_inventory_overview_bounded() {
    use super::session_inventory::{
        format_sessions_overview, SessionEntryKind, SESSION_OVERVIEW_MAX_CHARS,
        SESSION_OVERVIEW_MAX_ENTRIES,
    };
    let entries: Vec<_> = (1..=12)
        .map(|i| {
            let id = format!("ses_overview_{i:02}");
            make_entry(SessionEntryKind::Terminal, Some(&id), &id, None, Some(1000 + i as u32), &format!("proj-overview-{i:02}"), "Busy")
        })
        .collect();
    let (text, _markup) = format_sessions_overview(&entries);
    assert!(text.chars().count() <= SESSION_OVERVIEW_MAX_CHARS, "too long: {}", text.chars().count());
    assert!(text.contains("还有"));
    assert!(text.contains("未展示"));
    let rendered = text.matches("PID:").count();
    assert!(rendered <= SESSION_OVERVIEW_MAX_ENTRIES, "rendered {rendered} entries");
    assert_eq!(rendered, SESSION_OVERVIEW_MAX_ENTRIES.min(entries.len()));
    assert!(text.contains("ses_overview_01"));
    for i in (SESSION_OVERVIEW_MAX_ENTRIES + 1)..=12 {
        let id = format!("ses_overview_{i:02}");
        assert!(text.contains(&id), "omitted {id} must appear in truncation notice: {text}");
    }
}

#[test]
fn session_inventory_detail() {
    use rusqlite::Connection;
    use std::path::PathBuf;
    use super::session_inventory::{SessionProgressDetail, lookup_session_progress};
    use super::types::OpenCodeTodoInfo;

    let db_path: PathBuf = std::env::temp_dir().join(format!(
        "onespace_session_detail_{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open temp db");
        conn.execute_batch(
            "CREATE TABLE part(session_id TEXT, data TEXT, time_created INTEGER);
             CREATE TABLE todo(session_id TEXT, position INTEGER, status TEXT, content TEXT);",
        )
        .expect("create tables");

        conn.execute(
            "INSERT INTO part(session_id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params!["ses_detail", r#"{"type":"step-start"}"#, 1],
        )
        .expect("insert step 1");
        conn.execute(
            "INSERT INTO part(session_id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params!["ses_detail", r#"{"type":"step_start"}"#, 2],
        )
        .expect("insert step 2");
        conn.execute(
            "INSERT INTO part(session_id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "ses_detail",
                r#"{"type":"tool","tool":"bash","state":{"status":"completed","input":{"command":"cargo test"}}}"#,
                3
            ],
        )
        .expect("insert tool");
        conn.execute(
            "INSERT INTO todo(session_id, position, status, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["ses_detail", 0i32, "completed", "first todo"],
        )
        .expect("insert todo 1");
        conn.execute(
            "INSERT INTO todo(session_id, position, status, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["ses_detail", 1i32, "in_progress", "second todo"],
        )
        .expect("insert todo 2");
    }

    let detail: SessionProgressDetail = lookup_session_progress("ses_detail", &db_path)
        .expect("detail should exist");
    assert_eq!(detail.current_step, Some(2));
    assert_eq!(detail.last_tool, Some("bash".to_string()));
    assert_eq!(detail.last_tool_status, Some("completed".to_string()));
    assert!(detail
        .last_tool_input_preview
        .as_deref()
        .unwrap_or("")
        .contains("cargo test"));
    assert_eq!(detail.todos.len(), 2);
    assert_eq!(
        detail.todos[0],
        OpenCodeTodoInfo {
            position: 0,
            status: "completed".to_string(),
            content: "first todo".to_string()
        }
    );
    assert_eq!(
        detail.todos[1],
        OpenCodeTodoInfo {
            position: 1,
            status: "in_progress".to_string(),
            content: "second todo".to_string()
        }
    );

    assert!(lookup_session_progress("ses_missing", &db_path).is_none());

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn session_inventory_identifier() {
    use super::session_inventory::{IdentifierResolution, SessionEntry, resolve_session_identifier};

    let entry_a = SessionEntry {
        primary_kind: super::session_inventory::SessionEntryKind::Terminal,
        kinds: vec![super::session_inventory::SessionEntryKind::Terminal],
        session_id: Some("ses_a".to_string()),
        session_id_marker: "ses_a".to_string(),
        plan_id: None,
        pid: Some(111),
        project_name: "proj-a".to_string(),
        status_label: "Idle".to_string(),
        is_busy: false,
    };

    let entry_pid = SessionEntry {
        primary_kind: super::session_inventory::SessionEntryKind::Terminal,
        kinds: vec![super::session_inventory::SessionEntryKind::Terminal],
        session_id: Some("ses_pid".to_string()),
        session_id_marker: "ses_pid".to_string(),
        plan_id: None,
        pid: Some(4321),
        project_name: "proj-pid".to_string(),
        status_label: "Idle".to_string(),
        is_busy: false,
    };

    let plan_entry = SessionEntry {
        primary_kind: super::session_inventory::SessionEntryKind::PlanExecution,
        kinds: vec![super::session_inventory::SessionEntryKind::PlanExecution],
        session_id: Some("ses_plan".to_string()),
        session_id_marker: "ses_plan".to_string(),
        plan_id: Some("plan-x".to_string()),
        pid: None,
        project_name: "proj-plan".to_string(),
        status_label: "Running".to_string(),
        is_busy: true,
    };

    let amb_one = SessionEntry {
        primary_kind: super::session_inventory::SessionEntryKind::Terminal,
        kinds: vec![super::session_inventory::SessionEntryKind::Terminal],
        session_id: Some("ses_one".to_string()),
        session_id_marker: "ses_one".to_string(),
        plan_id: None,
        pid: Some(777),
        project_name: "proj-one".to_string(),
        status_label: "Idle".to_string(),
        is_busy: false,
    };
    let amb_two = SessionEntry {
        primary_kind: super::session_inventory::SessionEntryKind::Terminal,
        kinds: vec![super::session_inventory::SessionEntryKind::Terminal],
        session_id: Some("ses_two".to_string()),
        session_id_marker: "ses_two".to_string(),
        plan_id: None,
        pid: Some(777),
        project_name: "proj-two".to_string(),
        status_label: "Idle".to_string(),
        is_busy: false,
    };

    let entries = vec![entry_a.clone(), entry_pid.clone(), plan_entry.clone(), amb_one.clone(), amb_two.clone()];

    // trimmed session id
    match resolve_session_identifier(&entries, "  ses_a  ") {
        IdentifierResolution::Session(e) => assert_eq!(e.session_id, Some("ses_a".to_string())),
        other => panic!("expected Session, got {:?}", other),
    }

    // numeric pid match
    match resolve_session_identifier(&entries, "4321") {
        IdentifierResolution::Session(e) => assert_eq!(e.pid, Some(4321)),
        other => panic!("expected Session, got {:?}", other),
    }

    // numeric no pid match -> Unknown
    assert!(matches!(resolve_session_identifier(&entries, "9999"), IdentifierResolution::Unknown));

    // plan id match
    match resolve_session_identifier(&entries, "plan-x") {
        IdentifierResolution::Plan { plan_id, entry } => {
            assert_eq!(plan_id, "plan-x");
            assert!(entry.is_some());
        }
        other => panic!("expected Plan, got {:?}", other),
    }

    // unknown non-numeric -> Plan candidate None
    match resolve_session_identifier(&entries, "plan-unknown") {
        IdentifierResolution::Plan { plan_id, entry } => {
            assert_eq!(plan_id, "plan-unknown");
            assert!(entry.is_none());
        }
        other => panic!("expected Plan, got {:?}", other),
    }

    // ambiguous pid
    match resolve_session_identifier(&entries, "777") {
        IdentifierResolution::Ambiguous(v) => {
            assert!(v.contains(&"ses_one".to_string()));
            assert!(v.contains(&"ses_two".to_string()));
        }
        other => panic!("expected Ambiguous, got {:?}", other),
    }

    // case-sensitive: SES_A not session id -> Plan candidate None
    match resolve_session_identifier(&entries, "SES_A") {
        IdentifierResolution::Plan { plan_id, entry } => {
            assert_eq!(plan_id, "SES_A");
            assert!(entry.is_none());
        }
        other => panic!("expected Plan, got {:?}", other),
    }
}

#[test]
fn session_inventory_plan_execution() {
    use super::session_inventory::{PlanExecutionState, PlanExecutionSummary, plan_execution_summary};

    let matching_none = PlanExecutionState {
        plan_id: "plan-a".to_string(),
        session_id: None,
        project_name: "proj".to_string(),
        is_busy: true,
    };
    let summary_none = plan_execution_summary("plan-a", Some(&matching_none));
    assert_eq!(summary_none.session_id_marker, "capturing");
    assert!(summary_none.is_executing);
    assert!(!summary_none.execution_status.is_empty());

    let matching_some = PlanExecutionState {
        plan_id: "plan-a".to_string(),
        session_id: Some("ses_plan".to_string()),
        project_name: "proj".to_string(),
        is_busy: true,
    };
    let summary_some = plan_execution_summary("plan-a", Some(&matching_some));
    assert_eq!(summary_some.session_id_marker, "ses_plan");
    assert!(summary_some.is_executing);
    assert!(!summary_some.execution_status.is_empty());

    let summary_absent = plan_execution_summary("plan-a", None);
    assert_eq!(summary_absent.session_id_marker, "unavailable");
    assert!(!summary_absent.is_executing);
    assert!(!summary_absent.execution_status.is_empty());

    let different = PlanExecutionState {
        plan_id: "plan-b".to_string(),
        session_id: Some("ses_other".to_string()),
        project_name: "proj".to_string(),
        is_busy: true,
    };
    let summary_diff = plan_execution_summary("plan-a", Some(&different));
    assert_eq!(summary_diff.session_id_marker, "unavailable");
    assert!(!summary_diff.is_executing);
    assert!(!summary_diff.execution_status.is_empty());
}

#[tokio::test]
async fn bridge_conversation_replacement_and_cleanup() {
    use std::path::Path;
    use std::sync::Arc;
    use super::client::TelegramClient;
    use super::opencode_bridge::{
        BridgeTaskKind, OpenCodeBridge, extract_session_id_from_event,
    };

    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));

    // Fresh bridge: no conversation id and no task.
    let snap = bridge.session_snapshot().await;
    assert!(snap.conversation_session_id.is_none());
    assert!(snap.task.is_none());

    // begin_conversation clears conversation id, sets Conversation task.
    bridge.begin_conversation(Path::new("/tmp/proj-a")).await;
    let snap = bridge.session_snapshot().await;
    assert!(snap.conversation_session_id.is_none());
    let task = snap.task.expect("task must exist");
    assert_eq!(task.kind, BridgeTaskKind::Conversation);
    assert!(task.is_busy);
    assert_eq!(snap.conversation_project_name, Some("proj-a".to_string()));
    assert_eq!(task.project_name, Some("proj-a".to_string()));

    // record_conversation_session sets both ids.
    bridge.record_conversation_session("ses_conv_1").await;
    let snap = bridge.session_snapshot().await;
    assert_eq!(snap.conversation_session_id, Some("ses_conv_1".to_string()));
    let task = snap.task.expect("task must exist");
    assert_eq!(task.session_id, Some("ses_conv_1".to_string()));

    // begin_plan_execution sets Coding task, keeps conversation id.
    bridge.begin_plan_execution(Path::new("/tmp/proj-b"), "plan-1").await;
    let snap = bridge.session_snapshot().await;
    assert_eq!(snap.conversation_session_id, Some("ses_conv_1".to_string()));
    let task = snap.task.expect("task must exist");
    assert_eq!(task.kind, BridgeTaskKind::Coding);
    assert_eq!(task.plan_id, Some("plan-1".to_string()));
    assert!(task.session_id.is_none());
    assert!(task.is_busy);

    // record_plan_execution_session sets task session id.
    bridge.record_plan_execution_session("ses_plan_1").await;
    let snap = bridge.session_snapshot().await;
    let task = snap.task.expect("task must exist");
    assert_eq!(task.session_id, Some("ses_plan_1".to_string()));

    // finish_task removes task, conversation not busy, retains id.
    bridge.finish_task().await;
    let snap = bridge.session_snapshot().await;
    assert!(snap.task.is_none());
    assert_eq!(snap.conversation_session_id, Some("ses_conv_1".to_string()));
    assert!(!snap.conversation_busy);

    // later begin_conversation replaces conversation.
    bridge.begin_conversation(Path::new("/tmp/proj-c")).await;
    let snap = bridge.session_snapshot().await;
    assert!(snap.conversation_session_id.is_none());
    assert_eq!(snap.conversation_project_name, Some("proj-c".to_string()));
    let task = snap.task.expect("task must exist");
    assert_eq!(task.kind, BridgeTaskKind::Conversation);

    bridge.record_conversation_session("ses_conv_2").await;
    bridge.cancel_active_task().await;
    let snap = bridge.session_snapshot().await;
    assert!(snap.conversation_session_id.is_none());
    assert!(snap.task.is_none());
}

#[tokio::test]
async fn bridge_plan_only_does_not_clear_conversation() {
    use std::path::Path;
    use std::sync::Arc;
    use super::client::TelegramClient;
    use super::opencode_bridge::OpenCodeBridge;

    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));

    bridge.begin_conversation(Path::new("/tmp/proj-a")).await;
    bridge.record_conversation_session("ses_conv_1").await;

    let before = bridge.session_snapshot().await;
    bridge.begin_plan_execution(Path::new("/tmp/proj-b"), "plan-1").await;
    let after = bridge.session_snapshot().await;

    assert_eq!(before.conversation_session_id, after.conversation_session_id);
    assert_eq!(before.conversation_project_name, after.conversation_project_name);
    assert_eq!(before.conversation_busy, after.conversation_busy);

    bridge.finish_task().await;
    let snap = bridge.session_snapshot().await;
    assert_eq!(snap.conversation_session_id, Some("ses_conv_1".to_string()));
}

#[test]
fn bridge_extract_session_id_from_events() {
    use super::opencode_bridge::extract_session_id_from_event;
    use serde_json::json;

    assert_eq!(
        extract_session_id_from_event(&json!({"sessionID": "ses_v1"})),
        Some("ses_v1".to_string())
    );
    assert_eq!(
        extract_session_id_from_event(&json!({"part": {"sessionId": "ses_v2"}})),
        Some("ses_v2".to_string())
    );
    assert_eq!(
        extract_session_id_from_event(&json!({"part": {"session_id": "ses_shadow"}})),
        Some("ses_shadow".to_string())
    );
    assert_eq!(
        extract_session_id_from_event(&json!({"type": "text", "part": {"text": "hi"}})),
        None
    );
    assert_eq!(extract_session_id_from_event(&json!({})), None);
}

#[test]
fn hosted_progress_paths_probe_opencode_database() {
    let (db_path, log_path) = super::session_supervisor::hosted_progress_paths();
    assert_eq!(db_path, super::opencode_inspector::resolve_opencode_db_path());
    assert_eq!(log_path, super::opencode_inspector::resolve_opencode_log_path());
    assert!(db_path.to_string_lossy().contains(".local/share/opencode/opencode.db"));
    assert!(log_path.to_string_lossy().contains(".local/share/opencode/log/opencode.log"));
}

#[test]
fn hosted_progress_reads_step_and_todos_by_session_id() {
    use rusqlite::Connection;

    let base = std::env::temp_dir().join(format!(
        "onespace_hosted_progress_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let db_path = base.join("opencode.db");
    let _ = std::fs::remove_file(&db_path);

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE part(session_id TEXT, data TEXT, time_created INTEGER);
             CREATE TABLE todo(session_id TEXT, position INTEGER, status TEXT, content TEXT);
             CREATE TABLE session(id TEXT, tokens_input INTEGER, tokens_output INTEGER);
             CREATE TABLE session_v2(id TEXT, tokens_input INTEGER, tokens_output INTEGER);",
        )
        .unwrap();

        for i in 0..3 {
            conn.execute(
                "INSERT INTO part(session_id, data, time_created) VALUES (?1, ?2, ?3)",
                rusqlite::params!["ses_hosted_progress", r#"{"type":"step-start"}"#, i],
            )
            .unwrap();
        }

        conn.execute(
            "INSERT INTO todo(session_id, position, status, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["ses_hosted_progress", 0i32, "completed", "完成审计"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO todo(session_id, position, status, content) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["ses_hosted_progress", 1i32, "in_progress", "补齐测试"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO session(id, tokens_input, tokens_output) VALUES (?1, ?2, ?3)",
            rusqlite::params!["ses_hosted_progress", 1200i64, 300i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_v2(id, tokens_input, tokens_output) VALUES (?1, ?2, ?3)",
            rusqlite::params!["ses_hosted_progress", 1200i64, 300i64],
        )
        .unwrap();
    }

    let snap = super::session_supervisor::inspect_session_db(&db_path, "ses_hosted_progress")
        .expect("snapshot should exist");
    assert_eq!(snap.cur_step, 3);
    assert_eq!(
        snap.todos,
        vec![
            ("完成审计".to_string(), "completed".to_string()),
            ("补齐测试".to_string(), "in_progress".to_string()),
        ]
    );
    assert_eq!(snap.tokens, (1200, 300));

    let missing = super::session_supervisor::inspect_session_db(&db_path, "ses_missing");
    if let Some(s) = missing {
        assert_eq!(s.cur_step, 0);
        assert!(s.todos.is_empty());
    }

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn router_parse_sessions_command() {
    use super::router::{parse_sessions_command, SessionsCommand};
    assert_eq!(parse_sessions_command("/sessions"), Some(SessionsCommand::Overview));
    assert_eq!(
        parse_sessions_command("/sessions ses_abc"),
        Some(SessionsCommand::Detail("ses_abc".to_string()))
    );
    assert_eq!(
        parse_sessions_command("/sessions   ses_abc  "),
        Some(SessionsCommand::Detail("ses_abc".to_string()))
    );
    assert_eq!(parse_sessions_command("/sessionsfoo"), None);
    assert_eq!(parse_sessions_command("/prompt"), None);
}

#[test]
fn card_builder_help_consolidates_sessions() {
    let help = super::card_builder::CardBuilder::help_card();
    assert!(help.contains("/sessions"));
    assert!(help.contains("/plans"));
    assert!(!help.contains("/running"));
    assert!(!help.contains("/progress"));
}

#[test]
fn card_builder_session_redirect_texts() {
    let s = super::card_builder::CardBuilder::sessions_redirect_text();
    assert!(s.contains("/sessions"));
    let p = super::card_builder::CardBuilder::progress_redirect_text(Some("plan-1"));
    assert!(p.contains("/sessions plan-1"));
    let pn = super::card_builder::CardBuilder::progress_redirect_text(None);
    assert!(pn.contains("/sessions"));
    assert!(pn.contains("/plans"));
    let mentions_plan_id = pn.contains("plan ID")
        || pn.contains("plan id")
        || pn.contains("plan_id")
        || pn.contains("planId")
        || pn.to_lowercase().contains("id");
    assert!(mentions_plan_id);
    let pr = super::card_builder::CardBuilder::prompt_sessions_redirect_text();
    assert!(pr.contains("/sessions"));
}

#[test]
fn plans_list_card_preserved() {
    let (text, markup) = super::card_builder::CardBuilder::plans_list_card(
        "proj",
        &[("plan-1".to_string(), "🟡 `实施中`".to_string(), false)],
    );
    assert!(text.contains("plan-1"));
    assert!(text.contains("🟡 `实施中`"));
    let mut has_progress = false;
    for row in &markup.inline_keyboard {
        for btn in row {
            if let Some(cd) = &btn.callback_data {
                if cd == "check_progress:plan-1" {
                    has_progress = true;
                }
            }
        }
    }
    assert!(has_progress);
}

#[test]
fn card_builder_session_detail() {
    use super::card_builder::CardBuilder;
    use super::session_inventory::{SessionEntry, SessionEntryKind, SessionProgressDetail};
    use super::types::OpenCodeTodoInfo;

    let entry = SessionEntry {
        primary_kind: SessionEntryKind::Terminal,
        kinds: vec![SessionEntryKind::Terminal],
        session_id: Some("ses_x".to_string()),
        session_id_marker: "ses_x".to_string(),
        plan_id: None,
        pid: Some(4321),
        project_name: "proj-a".to_string(),
        status_label: "Busy".to_string(),
        is_busy: true,
    };

    let (text, markup) = CardBuilder::session_detail_card(&entry, None);
    assert!(text.contains("ses_x"));
    assert!(text.contains("4321"));
    assert!(text.contains("proj-a"));
    assert!(text.contains("unavailable"));

    let progress = SessionProgressDetail {
        current_step: Some(7),
        last_tool: Some("bash".to_string()),
        last_tool_status: Some("completed".to_string()),
        last_tool_input_preview: Some("cargo test".to_string()),
        todos: vec![OpenCodeTodoInfo {
            position: 0,
            status: "completed".into(),
            content: "完成审计".into(),
        }],
    };
    let (text2, _) = CardBuilder::session_detail_card(&entry, Some(&progress));
    assert!(text2.contains("7"));
    assert!(text2.contains("bash"));
    assert!(text2.contains("完成审计"));

    let markup = markup.unwrap();
    let mut has_refresh = false;
    let mut has_prompt = false;
    for row in &markup.inline_keyboard {
        for btn in row {
            if let Some(cd) = &btn.callback_data {
                if cd == "refresh_ext_session:4321" {
                    has_refresh = true;
                }
                if cd == "prompt_ext_session:4321" {
                    has_prompt = true;
                }
            }
        }
    }
    assert!(has_refresh);
    assert!(has_prompt);
}

#[test]
fn card_builder_plan_session_detail() {
    use super::card_builder::CardBuilder;
    use super::plan_progress::{PlanProgressInfo, PlanExecutionStatus};
    use super::session_inventory::PlanExecutionState;

    let info = PlanProgressInfo {
        plan_id: "plan-1".to_string(),
        status: PlanExecutionStatus::InProgress,
        started_at: Some("2026-11-09T10:00:00+08:00".to_string()),
        completed_at: None,
        elapsed_desc: Some("1 分 2 秒".to_string()),
        commit_hash: Some("abc123".to_string()),
        has_worktree: true,
        worktree_branch: Some("ai-workflow/plan-1".to_string()),
        steps: vec!["Step 1: 实施".to_string()],
        report_files: vec![("step1.log".to_string(), "10:00:00".to_string())],
    };

    let (text, markup) = CardBuilder::plan_session_detail_card(&info, None);
    assert!(text.contains("plan-1"));
    assert!(text.contains("Step 1: 实施"));
    assert!(text.contains("abc123"));
    assert!(text.contains("ai-workflow/plan-1"));
    assert!(text.contains("unavailable"));
    assert!(!text.contains("ses_"));

    let exec_none = PlanExecutionState {
        plan_id: "plan-1".to_string(),
        session_id: None,
        project_name: "proj".to_string(),
        is_busy: true,
    };
    let (text2, _) = CardBuilder::plan_session_detail_card(&info, Some(&exec_none));
    assert!(text2.contains("capturing"));

    let exec_some = PlanExecutionState {
        plan_id: "plan-1".to_string(),
        session_id: Some("ses_plan".to_string()),
        project_name: "proj".to_string(),
        is_busy: true,
    };
    let (text3, _) = CardBuilder::plan_session_detail_card(&info, Some(&exec_some));
    assert!(text3.contains("ses_plan"));

    let markup = markup.unwrap();
    let mut has_check = false;
    for row in &markup.inline_keyboard {
        for btn in row {
            if let Some(cd) = &btn.callback_data {
                if cd == "check_progress:plan-1" {
                    has_check = true;
                }
            }
        }
    }
    assert!(has_check);
}

#[test]
fn card_builder_identifier_error() {
    use super::card_builder::CardBuilder;
    use super::session_inventory::IdentifierResolution;

    let t1 = CardBuilder::identifier_error_text("nope", &IdentifierResolution::Unknown);
    assert!(t1.contains("nope"));
    assert!(!t1.contains("ses_"));

    let t2 = CardBuilder::identifier_error_text(
        "dup",
        &IdentifierResolution::Ambiguous(vec!["ses_a".to_string(), "ses_b".to_string()]),
    );
    assert!(t2.contains("ses_a"));
    assert!(t2.contains("ses_b"));
}
