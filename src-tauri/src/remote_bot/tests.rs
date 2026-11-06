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
