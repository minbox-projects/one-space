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
    use std::time::{SystemTime, UNIX_EPOCH};

    // 使用进程内唯一的临时目录，避免依赖机器上的全局工作空间记录或仓库目录名。
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp_path = std::env::temp_dir().join(format!(
        "onespace-remote-bot-workspace-{}-{}",
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&temp_path).expect("临时工作目录必须创建成功");
    let canon_temp = std::fs::canonicalize(&temp_path).expect("临时工作目录必须可规范化");
    let temp_str = canon_temp.to_string_lossy().to_string();
    let folder_name = canon_temp
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut config = RemoteBotConfig::default();
    config.workspace_directories = vec![temp_str.clone()];

    // 1. active_workspace 命中 workspace_directories 中的文件夹名
    assert!(!folder_name.is_empty());
    config.active_workspace = Some(folder_name.clone());
    let resolved = resolve_project_dir(&config);
    assert_eq!(resolved, canon_temp);

    // 2. active_workspace 直接是绝对路径
    config.active_workspace = Some(temp_str.clone());
    let resolved = resolve_project_dir(&config);
    assert_eq!(resolved, canon_temp);

    // 3. active_workspace 为 None 时，自动使用 workspace_directories 中的第一个有效目录
    config.active_workspace = None;
    let resolved = resolve_project_dir(&config);
    assert_eq!(resolved, canon_temp);

    // 4. 验证 collect_all_workspaces 包含临时工作目录
    let all = collect_all_workspaces(&config);
    assert!(all.iter().any(|it| it.path == canon_temp));

    // 5. 验证 collect_featured_projects 数量受控（上限不超过 8）
    let featured = collect_featured_projects(&config);
    assert!(featured.len() <= 8);
    assert!(featured.iter().any(|it| it.path == canon_temp));

    let _ = std::fs::remove_dir_all(&temp_path);
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
    let cwd_sub = env!("CARGO_MANIFEST_DIR"); // .../<仓库根目录>/src-tauri
    let repo_root_name = std::path::Path::new(cwd_sub)
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .expect("src-tauri 必须位于仓库根目录下");
    let proj_name = detect_meaningful_project_name(cwd_sub);
    assert_eq!(proj_name, repo_root_name);
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
    assert!(s.contains("按钮"));
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

#[test]
fn card_builder_session_detail_idle_has_no_dead_actions() {
    use super::card_builder::CardBuilder;
    use super::session_inventory::{SessionEntry, SessionEntryKind};

    let entry = SessionEntry {
        primary_kind: SessionEntryKind::Terminal,
        kinds: vec![SessionEntryKind::Terminal],
        session_id: Some("ses_idle".to_string()),
        session_id_marker: "ses_idle".to_string(),
        plan_id: None,
        pid: Some(222),
        project_name: "proj-idle".to_string(),
        status_label: "Idle".to_string(),
        is_busy: false,
    };

    let (_text, markup) = CardBuilder::session_detail_card(&entry, None);
    if let Some(mk) = markup {
        for row in &mk.inline_keyboard {
            for btn in row {
                if let Some(cd) = &btn.callback_data {
                    assert!(!cd.contains("refresh_ext_session:"), "dead callback: {cd}");
                    assert!(!cd.contains("prompt_ext_session:"), "dead callback: {cd}");
                    assert!(!cd.contains("diff_ext_session:"), "dead callback: {cd}");
                    assert!(!cd.contains("start_host:"), "dead callback: {cd}");
                }
            }
        }
    }
}

#[test]
fn card_builder_session_detail_hosted_actions() {
    use super::card_builder::CardBuilder;
    use super::session_inventory::{SessionEntry, SessionEntryKind};

    let entry = SessionEntry {
        primary_kind: SessionEntryKind::Hosted,
        kinds: vec![SessionEntryKind::Hosted, SessionEntryKind::Terminal],
        session_id: Some("ses_hosted".to_string()),
        session_id_marker: "ses_hosted".to_string(),
        plan_id: None,
        pid: Some(444),
        project_name: "proj-hosted".to_string(),
        status_label: "Hosted".to_string(),
        is_busy: false,
    };

    let (_text, markup) = CardBuilder::session_detail_card(&entry, None);
    let markup = markup.expect("hosted detail must have markup");
    let mut has_toggle = false;
    let mut has_stop = false;
    let mut has_start = false;
    for row in &markup.inline_keyboard {
        for btn in row {
            if let Some(cd) = &btn.callback_data {
                if cd.contains("toggle_host_mode:ses_hosted") {
                    has_toggle = true;
                }
                if cd.contains("stop_host:ses_hosted") {
                    has_stop = true;
                }
                if cd.contains("start_host:") {
                    has_start = true;
                }
            }
        }
    }
    assert!(has_toggle, "missing toggle_host_mode:ses_hosted");
    assert!(has_stop, "missing stop_host:ses_hosted");
    assert!(!has_start, "must not contain start_host:");
}

#[test]
fn test_default_bot_commands_match_current_command_surface() {
    let commands = super::default_bot_commands();

    let names: Vec<String> = commands.iter().map(|c| c.command.clone()).collect();

    // No duplicates.
    let mut unique = std::collections::BTreeSet::new();
    for name in &names {
        assert!(unique.insert(name.clone()), "duplicate command: {}", name);
    }

    let expected: std::collections::BTreeSet<&str> = [
        "sessions", "host", "unhost", "prompt", "projects", "planning", "plans",
        "coding", "diff", "cancel", "status", "help",
    ]
    .iter()
    .cloned()
    .collect();
    let actual: std::collections::BTreeSet<&str> = names.iter().map(|s| s.as_str()).collect();
    assert_eq!(actual, expected, "advertised command set mismatch");

    // Deprecated entries must be absent.
    assert!(!names.iter().any(|n| n == "running"), "deprecated 'running' present");
    assert!(!names.iter().any(|n| n == "progress"), "deprecated 'progress' present");

    // Telegram constraints for every command.
    for cmd in &commands {
        let name_len = cmd.command.chars().count();
        assert!(
            (1..=32).contains(&name_len),
            "command name length out of range: {}",
            cmd.command
        );
        assert!(
            cmd.command
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "command name has invalid chars: {}",
            cmd.command
        );
        assert!(!cmd.description.is_empty(), "empty description for {}", cmd.command);
        let desc_len = cmd.description.chars().count();
        assert!(
            (3..=256).contains(&desc_len),
            "description length out of range for {}: {}",
            cmd.command,
            desc_len
        );
    }
}

#[test]
fn session_inventory_overview_is_actionable() {
    use super::session_inventory::{format_sessions_overview, SessionEntryKind};

    let entries = vec![
        make_entry(SessionEntryKind::Terminal, Some("ses_term"), "ses_term", None, Some(111), "proj-term", "Busy"),
        make_entry(SessionEntryKind::Terminal, None, "unavailable", None, Some(222), "proj-noid", "Idle"),
        make_entry(SessionEntryKind::PlanExecution, None, "capturing", Some("plan-1"), None, "proj-plan", "Running"),
        make_entry(SessionEntryKind::TelegramConversation, None, "unassigned", None, None, "proj-conv", "Idle"),
    ];

    let (text, markup) = format_sessions_overview(&entries);
    let markup = markup.expect("overview markup must be Some");
    let callbacks: Vec<String> = markup
        .inline_keyboard
        .iter()
        .flat_map(|row| row.iter())
        .filter_map(|btn| btn.callback_data.clone())
        .collect();

    assert!(callbacks.contains(&"open_session:ses_term".to_string()));
    assert!(callbacks.contains(&"open_session:222".to_string()));
    assert!(callbacks.contains(&"open_session:plan-1".to_string()));
    assert!(callbacks.contains(&"open_sessions".to_string()));
    assert!(callbacks.iter().all(|c| c.len() <= 64));
    // unresolvable conversation entry must have no button
    assert_eq!(
        callbacks.iter().filter(|c| c.starts_with("open_session:")).count(),
        3
    );
    let _ = text;
}

#[test]
fn session_inventory_overview_mobile_lines() {
    use super::session_inventory::{
        format_sessions_overview, SessionEntryKind, SESSION_OVERVIEW_MAX_CHARS,
        SESSION_OVERVIEW_MAX_LINE_CHARS,
    };

    let entries: Vec<_> = (1..=9)
        .map(|i| {
            let project_name = format!("PROJ-{}", format!("p{}", i).repeat(60));
            let session_id = format!("ses_{}", format!("s{}", i).repeat(20));
            make_entry(
                SessionEntryKind::Terminal,
                Some(&session_id),
                &session_id,
                None,
                Some(1000 + i as u32),
                &project_name,
                "Busy",
            )
        })
        .collect();

    let (text, _markup) = format_sessions_overview(&entries);
    for line in text.lines() {
        assert!(
            line.chars().count() <= SESSION_OVERVIEW_MAX_LINE_CHARS,
            "line too long ({}): {}",
            line.chars().count(),
            line
        );
    }
    assert!(text.chars().count() <= SESSION_OVERVIEW_MAX_CHARS);
    assert!(text.contains("还有"));
    assert!(text.contains("未展示"));

    // project and identifier are reported on separate lines
    let first_project_line = text.lines().find(|l| l.contains("PROJ-")).unwrap();
    let first_id_line = text.lines().find(|l| l.contains("ses_")).unwrap();
    assert!(!first_project_line.contains("ses_"));
    assert!(!first_id_line.contains("PROJ-"));
}

#[test]
fn router_parse_session_callback() {
    use super::router::{parse_session_callback, SessionCallback};

    assert_eq!(parse_session_callback("open_sessions"), Some(SessionCallback::Overview));
    assert_eq!(
        parse_session_callback("open_session:abc"),
        Some(SessionCallback::Detail("abc".to_string()))
    );
    assert_eq!(parse_session_callback("open_session:"), None);
    assert_eq!(parse_session_callback("anything-else"), None);
}

#[test]
fn router_parse_prompt_command_with_target() {
    use super::router::{parse_prompt_command, ParsedPromptCommand};

    let tokens: Vec<String> = vec!["ses_a".to_string(), "4321".to_string()];
    let empty = ParsedPromptCommand { target: None, content: "".to_string() };

    // Exact command boundaries and empty content.
    assert_eq!(parse_prompt_command("/prompt", &tokens), Some(empty.clone()));
    assert_eq!(parse_prompt_command("/send", &tokens), Some(empty.clone()));
    assert_eq!(parse_prompt_command("/prompt   ", &tokens), Some(empty.clone()));
    assert_eq!(parse_prompt_command("/send  ", &tokens), Some(empty));

    // Unrelated / lookalike commands are rejected.
    assert_eq!(parse_prompt_command("/promptfoo", &tokens), None);
    assert_eq!(parse_prompt_command("/promptfoo hello", &tokens), None);
    assert_eq!(parse_prompt_command("/sendfoo hello", &tokens), None);
    assert_eq!(parse_prompt_command("/status", &tokens), None);
    assert_eq!(parse_prompt_command("hello /prompt ses_a hi", &tokens), None);
    assert_eq!(parse_prompt_command("", &tokens), None);

    // Matching session-id target.
    assert_eq!(
        parse_prompt_command("/prompt ses_a 请补充测试", &tokens),
        Some(ParsedPromptCommand { target: Some("ses_a".to_string()), content: "请补充测试".to_string() })
    );
    // Matching PID target.
    assert_eq!(
        parse_prompt_command("/prompt 4321 继续", &tokens),
        Some(ParsedPromptCommand { target: Some("4321".to_string()), content: "继续".to_string() })
    );
    // /send alias with a target.
    assert_eq!(
        parse_prompt_command("/send ses_a hello", &tokens),
        Some(ParsedPromptCommand { target: Some("ses_a".to_string()), content: "hello".to_string() })
    );
    // Unmatched first token stays as content.
    assert_eq!(
        parse_prompt_command("/prompt 请补充测试", &tokens),
        Some(ParsedPromptCommand { target: None, content: "请补充测试".to_string() })
    );
    assert_eq!(
        parse_prompt_command("/prompt ses_unknown hello", &tokens),
        Some(ParsedPromptCommand { target: None, content: "ses_unknown hello".to_string() })
    );
}

#[test]
fn router_parse_prompt_choice_callback() {
    use super::router::parse_prompt_choice_callback;

    assert_eq!(parse_prompt_choice_callback("prompt_choice:0"), Some(0));
    assert_eq!(parse_prompt_choice_callback("prompt_choice:12"), Some(12));
    assert_eq!(parse_prompt_choice_callback("prompt_choice:"), None);
    assert_eq!(parse_prompt_choice_callback("prompt_choice:x"), None);
    assert_eq!(parse_prompt_choice_callback("prompt_choice:-1"), None);
    assert_eq!(parse_prompt_choice_callback("cancel_prompt_choice"), None);
    assert_eq!(parse_prompt_choice_callback("prompt_choice"), None);
    assert_eq!(parse_prompt_choice_callback("anything-else"), None);
    assert_eq!(parse_prompt_choice_callback(""), None);
}

#[test]
fn card_builder_prompt_target_selection() {
    use super::card_builder::CardBuilder;

    let sessions = vec![
        make_external(Some("ses_a"), 111, true),
        make_external(Some("ses_b"), 222, true),
    ];

    // With content: text and button contract.
    let (text, markup) = CardBuilder::prompt_target_selection_card("请补充\n测试", &sessions);
    assert!(text.contains("选择目标会话"), "text: {text}");
    assert!(text.contains('2'), "text must mention session count: {text}");
    // Line breaks in content collapse to spaces in the preview.
    assert!(text.contains("请补充 测试"), "preview must collapse newlines: {text}");
    assert!(!text.contains("请补充\n测试"), "preview must not keep newline: {text}");
    // Bounded text.
    assert!(text.chars().count() <= 1200, "text too long: {}", text.chars().count());
    for line in text.lines() {
        assert!(line.chars().count() <= 100, "line too long: {line}");
    }
    // Buttons: one row per session plus a final cancel row.
    assert_eq!(markup.inline_keyboard.len(), 3);
    assert_eq!(markup.inline_keyboard[0].len(), 1);
    assert_eq!(markup.inline_keyboard[1].len(), 1);
    assert_eq!(markup.inline_keyboard[0][0].callback_data.as_deref(), Some("prompt_choice:0"));
    assert_eq!(markup.inline_keyboard[1][0].callback_data.as_deref(), Some("prompt_choice:1"));
    assert_eq!(markup.inline_keyboard[2].len(), 1);
    assert_eq!(markup.inline_keyboard[2][0].callback_data.as_deref(), Some("cancel_prompt_choice"));
    for (idx, session) in sessions.iter().enumerate() {
        let btn = &markup.inline_keyboard[idx][0];
        assert!(btn.text.contains(&session.project_name), "button must contain project: {}", btn.text);
        assert!(btn.text.contains(&session.pid.to_string()), "button must contain PID: {}", btn.text);
        assert!(btn.text.chars().count() <= 40, "button too long: {}", btn.text);
    }

    // Long content is truncated to a compact preview (at most 80 chars, no newlines).
    let long = "啊".repeat(200);
    let (long_text, _) = CardBuilder::prompt_target_selection_card(&long, &sessions);
    assert!(!long_text.contains(&long), "long content must be truncated in preview");
    assert!(long_text.chars().count() <= 1200);
    for line in long_text.lines() {
        assert!(line.chars().count() <= 100, "line too long: {line}");
    }

    // Without content: text tells the user to enter the prompt after choosing.
    let (empty_text, empty_markup) = CardBuilder::prompt_target_selection_card("", &sessions);
    assert!(empty_text.contains("选择目标会话"), "text: {empty_text}");
    assert!(empty_text.contains('2'), "text: {empty_text}");
    assert!(
        empty_text.contains("输入") || empty_text.contains("提示"),
        "empty content must say the prompt is entered after choosing: {empty_text}"
    );
    assert!(empty_text.chars().count() <= 1200);
    for line in empty_text.lines() {
        assert!(line.chars().count() <= 100, "line too long: {line}");
    }
    assert_eq!(empty_markup.inline_keyboard.len(), 3);
    assert_eq!(empty_markup.inline_keyboard[0][0].callback_data.as_deref(), Some("prompt_choice:0"));
    assert_eq!(empty_markup.inline_keyboard[1][0].callback_data.as_deref(), Some("prompt_choice:1"));
    assert_eq!(empty_markup.inline_keyboard[2][0].callback_data.as_deref(), Some("cancel_prompt_choice"));
}

#[test]
fn card_builder_quick_actions_expose_next_steps() {
    use super::card_builder::CardBuilder;
    use super::types::InlineKeyboardMarkup;

    let quick = CardBuilder::quick_actions();
    assert_eq!(quick.inline_keyboard.len(), 1);
    let quick_cbs: Vec<&str> = quick.inline_keyboard[0]
        .iter()
        .filter_map(|b| b.callback_data.as_deref())
        .collect();
    assert_eq!(quick_cbs, vec!["open_sessions", "open_status", "open_help"]);

    let sessions = CardBuilder::sessions_actions();
    let sessions_cbs: Vec<&str> = sessions.inline_keyboard[0]
        .iter()
        .filter_map(|b| b.callback_data.as_deref())
        .collect();
    assert_eq!(sessions_cbs, vec!["open_sessions", "open_plans"]);

    let projects = CardBuilder::projects_actions();
    let projects_cbs: Vec<&str> = projects.inline_keyboard[0]
        .iter()
        .filter_map(|b| b.callback_data.as_deref())
        .collect();
    assert_eq!(projects_cbs, vec!["open_projects", "open_sessions"]);

    let ack = CardBuilder::prompt_ack_actions();
    let ack_cbs: Vec<&str> = ack.inline_keyboard[0]
        .iter()
        .filter_map(|b| b.callback_data.as_deref())
        .collect();
    assert_eq!(ack_cbs, vec!["cancel_task", "open_sessions"]);
}

#[test]
fn card_builder_diff_preview_is_mobile_bounded() {
    use super::card_builder::CardBuilder;

    let mut diff = String::new();
    for _ in 0..2000 {
        diff.push('x');
    }
    diff.push_str("UNIQUE_MARKER_END");
    let out = CardBuilder::diff_preview_card("plan", "main", "1 file", &diff);
    assert!(out.contains("已截取"));
    assert!(out.contains("600"));
    assert!(!out.contains("UNIQUE_MARKER_END"));
    assert!(out.chars().count() < 2000);
}

#[test]
fn card_builder_session_detail_truncates_long_preview() {
    use super::card_builder::CardBuilder;
    use super::session_inventory::{SessionEntry, SessionEntryKind, SessionProgressDetail};

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

    let long_preview = "a".repeat(300);
    let progress = SessionProgressDetail {
        current_step: Some(7),
        last_tool: Some("bash".to_string()),
        last_tool_status: Some("completed".to_string()),
        last_tool_input_preview: Some(long_preview.clone()),
        todos: vec![],
    };
    let (text, _) = CardBuilder::session_detail_card(&entry, Some(&progress));
    assert!(!text.contains(&long_preview));
    let tool_line = text
        .lines()
        .find(|l| l.contains("工具入参"))
        .expect("missing tool line");
    assert!(tool_line.chars().count() <= 120, "line too long: {tool_line}");
    assert!(tool_line.ends_with('…'), "line: {tool_line}");
}

#[test]
fn card_builder_cancel_copy_is_truthful() {
    use super::card_builder::CardBuilder;

    let text = CardBuilder::task_cancelled_card();
    assert!(!text.contains("无悬挂进程"));
    assert!(text.contains("已请求"));
    assert!(text.contains("/sessions"));
}

#[test]
fn card_builder_help_mentions_prompt_targets() {
    use super::card_builder::CardBuilder;

    let help = CardBuilder::help_card();
    assert!(help.contains("/prompt"));
    assert!(help.contains("/sessions"));
    assert!(help.contains("会话ID") || help.contains("PID"));
}

#[test]
fn card_builder_welcome_mentions_sessions() {
    use super::card_builder::CardBuilder;

    let welcome = CardBuilder::welcome_card(1, "host");
    assert!(welcome.contains("/sessions"));
    assert!(welcome.contains("/prompt"));
}

#[test]
fn card_builder_analysis_reply_is_mobile_bounded() {
    // 5,000-char content with unique end marker; reply must clamp ≤3200,
    // show truncation notice, drop the end marker, keep workspace + next-step hint.
    let end_marker = "UNIQUE_ANALYSIS_END_MARKER_XYZ";
    let mut content = "a".repeat(5000 - end_marker.len());
    content.push_str(end_marker);
    assert_eq!(content.chars().count(), 5000);

    let out = super::card_builder::CardBuilder::analysis_reply_card("workspace", &content);

    assert!(out.chars().count() <= 3200, "output too long: {}", out.chars().count());
    assert!(out.contains("已截断") || out.contains("已省略"), "missing truncation notice: {}", out);
    assert!(!out.contains(end_marker), "end marker must be truncated away");
    assert!(out.contains("workspace"), "missing workspace name: {}", out);
    assert!(out.contains("/sessions") || out.contains("/cancel"), "missing next-step hint: {}", out);
}

// --- Permission risk classifier ---
// The classifier is deterministic: allowlisted routine commands stay Routine,
// while destructive, unknown, compound, substituted or redirecting shell
// commands fail closed as HighRisk.

use super::permission::{classify_permission, PermissionRisk};

fn expect_routine(permission_type: &str, command: Option<&str>) {
    let actual = classify_permission(permission_type, command);
    assert_eq!(
        actual,
        PermissionRisk::Routine,
        "expected Routine for permission_type {:?} and command {:?}",
        permission_type,
        command
    );
}

fn expect_high_risk(permission_type: &str, command: Option<&str>) {
    let actual = classify_permission(permission_type, command);
    assert_eq!(
        actual,
        PermissionRisk::HighRisk,
        "expected HighRisk for permission_type {:?} and command {:?}",
        permission_type,
        command
    );
}

#[test]
fn permission_classify_routine_allowlisted_commands() {
    for command in [
        "pwd",
        "ls -la",
        "cat Cargo.toml",
        "rg foo src",
        "sed -n '1,40p' file",
        "cargo test --lib",
        "cargo build",
        "npm run build",
        "npx tsc --noEmit",
        "git status",
        "git diff --stat",
        "git log --oneline -5",
        "git branch",
        "git remote -v",
        "git rev-parse HEAD",
        "git tag v1.0.0",
        "git commit -m \"msg\"",
        "git checkout -b feature",
        "git switch -c other",
        "git stash push",
        "git stash pop",
        "mkdir -p tmp",
        "touch tmp/x",
        "cd src",
        "sleep 5",
        "find . -name '*.rs'",
    ] {
        expect_routine("bash", Some(command));
    }
}

#[test]
fn permission_classify_high_risk_commands() {
    for command in [
        "rm -rf target",
        "cp a b",
        "mv a b",
        "chmod 777 x",
        "sudo ls",
        "git push --force",
        "git reset --hard",
        "git clean -fdx",
        "git rebase main",
        "git cherry-pick abc",
        "git revert abc",
        "git branch -D old",
        "git commit --amend",
        "npm install",
        "npm publish",
        "cargo publish",
        "cargo add serde",
        "cargo install ripgrep",
        "pip install requests",
        "curl https://example.com",
        "wget https://example.com",
        "ssh host",
        "awk '{print $1}' file",
        "kill 123",
        "pkill node",
        "shutdown -h now",
        "bash -c 'ls'",
        "sh -c 'ls'",
        "eval ls",
        "exec ls",
        "source script.sh",
        "osascript -e 'beep'",
    ] {
        expect_high_risk("bash", Some(command));
    }
}

#[test]
fn permission_classify_sed_read_only_versus_write_and_exec() {
    expect_routine("bash", Some("sed -n '1,10p' file"));
    expect_high_risk("bash", Some("sed -i 's/a/b/' file"));
    expect_high_risk("bash", Some("sed -n '1,10w out.txt' file"));
    expect_high_risk("bash", Some("sed -n '1,10e touch x' file"));
}

#[test]
fn permission_classify_find_destructive_flags() {
    expect_high_risk("bash", Some("find . -delete"));
    expect_high_risk("bash", Some("find . -exec rm {} ;"));
    expect_high_risk("bash", Some("find . -ok rm {} ;"));
}

#[test]
fn permission_classify_compound_command_segments() {
    expect_high_risk("bash", Some("cargo test && rm -rf target"));
    expect_routine("bash", Some("ls; pwd"));
    expect_routine("bash", Some("ls || true"));
    expect_routine("bash", Some("ls\npwd"));
    expect_high_risk("bash", Some("cargo test\nrm -rf target"));
}

#[test]
fn permission_classify_substitution_and_redirection_fail_closed() {
    expect_high_risk("bash", Some("echo $(rm -rf x)"));
    expect_high_risk("bash", Some("echo `rm -rf x`"));
    expect_high_risk("bash", Some("cat <(rm -rf x)"));
    expect_high_risk("bash", Some("ls >(cat)"));
    expect_high_risk("bash", Some("ls | bash"));
    expect_high_risk("bash", Some("cargo test > out.log"));
    expect_high_risk("bash", Some("cargo test 2> err.log"));
    expect_high_risk("bash", Some("cat < input.txt"));
    expect_high_risk("bash", Some("cargo test >> out.log"));
}

#[test]
fn permission_classify_wrappers_and_env_assignments() {
    expect_high_risk("bash", Some("FOO=bar cargo test"));
    expect_high_risk("bash", Some("A=1 B=2 ls"));
    expect_routine("bash", Some("timeout 60 cargo test"));
    expect_routine("bash", Some("nice cargo build"));
    expect_routine("bash", Some("command ls"));
    expect_routine("bash", Some("builtin cd"));
    expect_high_risk("bash", Some("timeout 60 rm -rf x"));
    expect_high_risk("bash", Some("FOO=$(rm -rf x) ls"));
}

#[test]
fn permission_classify_environment_assignments_fail_closed() {
    for command in [
        "PATH=/tmp/evil ls",
        "LD_PRELOAD=/tmp/evil.dylib cargo test",
        "DYLD_INSERT_LIBRARIES=/tmp/evil.dylib ls",
        "BASH_ENV=/tmp/evil.sh ls",
        "ENV=/tmp/evil.sh ls",
        "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=/tmp/evil git status",
        "NODE_OPTIONS=--require=/tmp/evil.js npm run build",
        "PYTHONPATH=/tmp/x python -m pytest",
        "RUSTC_WRAPPER=/tmp/x cargo test",
        "CARGO_BUILD_RUSTC=/tmp/x cargo build",
        "CARGO_HOME=/tmp/x cargo test",
        "FOO=bar cargo test",
        "A=1 B=2 ls",
        "SAFE_LITERAL=1 ls",
    ] {
        expect_high_risk("bash", Some(command));
    }
}

#[test]
fn permission_classify_non_shell_permission_types() {
    for permission_type in [
        "edit",
        "write",
        "read",
        "patch",
        "apply_patch",
        "list",
        "glob",
        "grep",
    ] {
        expect_routine(permission_type, None);
    }
    for permission_type in ["webfetch", "websearch", "some_new_tool"] {
        expect_high_risk(permission_type, None);
    }
}

#[test]
fn permission_classify_missing_or_unknown_shell_input() {
    expect_high_risk("bash", None);
    expect_high_risk("bash", Some(""));
    expect_high_risk("bash", Some("   "));
    expect_high_risk("bash", Some("frobnicate --now"));
    expect_high_risk("", None);
}

#[test]
fn permission_classify_language_and_build_tools() {
    expect_routine("bash", Some("python -m pytest"));
    expect_routine("bash", Some("go test ./..."));
    expect_routine("bash", Some("make test"));
    expect_routine("bash", Some("prettier --check ."));
    expect_routine("bash", Some("ruff check ."));
    expect_high_risk("bash", Some("python script.py"));
    expect_high_risk("bash", Some("cargo run"));
}

// --- Pending permission registry, decision and timeout ---
// The registry is shared across async tasks, so it must be Send + Sync, must
// store at most one pending request per id, and must make each decision
// single-use. The 30-minute timeout boundary is absolute: 29m59s is still
// pending, exactly 30m is expired.

use super::permission::{
    permission_timed_out, PendingPermissionRequest, PermissionDecision, PermissionRegistry,
    PERMISSION_TIMEOUT,
};
use std::time::{Duration, Instant};

fn permission_assert_send_sync<T: Send + Sync>() {}

#[test]
fn permission_registry_is_send_and_sync() {
    permission_assert_send_sync::<PermissionRegistry>();
}

#[test]
fn permission_registry_insert_get_and_duplicate() {
    let registry = PermissionRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    let sent_at = Instant::now();
    let first = PendingPermissionRequest::new(
        "req-1",
        42,
        "bash",
        Some("cargo test".to_string()),
        PermissionRisk::Routine,
        sent_at,
    );
    assert!(registry.insert(first).is_none());
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());

    let stored = registry.get("req-1").expect("inserted request must be retrievable");
    assert_eq!(stored.request_id, "req-1");
    assert_eq!(stored.chat_id, 42);
    assert_eq!(stored.permission_type, "bash");
    assert_eq!(stored.command.as_deref(), Some("cargo test"));
    assert_eq!(stored.risk, PermissionRisk::Routine);
    assert_eq!(stored.card_message_id, None);
    assert_eq!(stored.sent_at, sent_at);

    let other = PendingPermissionRequest::new(
        "req-2",
        43,
        "edit",
        None,
        PermissionRisk::HighRisk,
        sent_at,
    );
    assert!(registry.insert(other).is_none());
    assert_eq!(registry.len(), 2);
    assert_eq!(
        registry.get("req-2").expect("second request must coexist").chat_id,
        43
    );

    let newest = PendingPermissionRequest::new(
        "req-1",
        99,
        "write",
        Some("npm run build".to_string()),
        PermissionRisk::HighRisk,
        sent_at,
    );
    let replaced = registry
        .insert(newest)
        .expect("duplicate id must return the previous request");
    assert_eq!(replaced.request_id, "req-1");
    assert_eq!(replaced.chat_id, 42);
    assert_eq!(replaced.permission_type, "bash");
    assert_eq!(replaced.command.as_deref(), Some("cargo test"));
    assert_eq!(registry.len(), 2);

    let updated = registry.get("req-1").expect("newest request must remain");
    assert_eq!(updated.chat_id, 99);
    assert_eq!(updated.permission_type, "write");
    assert_eq!(updated.command.as_deref(), Some("npm run build"));
    assert_eq!(updated.risk, PermissionRisk::HighRisk);
    assert_eq!(registry.get("req-2").expect("unrelated id must remain").chat_id, 43);
}

#[test]
fn permission_registry_resolve_is_single_use() {
    let registry = PermissionRegistry::new();
    let sent_at = Instant::now();
    registry.insert(PendingPermissionRequest::new(
        "req-a",
        11,
        "bash",
        Some("ls".to_string()),
        PermissionRisk::Routine,
        sent_at,
    ));
    registry.insert(PendingPermissionRequest::new(
        "req-b",
        12,
        "bash",
        Some("rm -rf target".to_string()),
        PermissionRisk::HighRisk,
        sent_at,
    ));

    let resolution = registry
        .resolve("req-a", PermissionDecision::Once)
        .expect("pending request must resolve");
    assert_eq!(resolution.request.request_id, "req-a");
    assert_eq!(resolution.request.chat_id, 11);
    assert_eq!(resolution.decision, PermissionDecision::Once);
    assert_eq!(registry.len(), 1);
    assert!(registry.get("req-a").is_none());

    assert!(
        registry.resolve("req-a", PermissionDecision::Once).is_none(),
        "the second decision for the same id must be a no-op"
    );
    assert!(registry.resolve("unknown-id", PermissionDecision::Deny).is_none());
    assert_eq!(registry.len(), 1);

    let other = registry
        .resolve("req-b", PermissionDecision::Always)
        .expect("resolving one id must leave others pending");
    assert_eq!(other.request.request_id, "req-b");
    assert_eq!(other.request.chat_id, 12);
    assert_eq!(other.decision, PermissionDecision::Always);
    assert!(registry.is_empty());
}

#[test]
fn permission_decision_as_reply_values() {
    assert_eq!(PermissionDecision::Once.as_reply(), "once");
    assert_eq!(PermissionDecision::Always.as_reply(), "always");
    assert_eq!(PermissionDecision::Deny.as_reply(), "deny");
}

#[test]
fn permission_timeout_boundary() {
    assert_eq!(PERMISSION_TIMEOUT, Duration::from_secs(30 * 60));

    let sent_at = Instant::now();
    assert!(!permission_timed_out(
        sent_at,
        sent_at + PERMISSION_TIMEOUT - Duration::from_secs(1)
    ));
    assert!(permission_timed_out(sent_at, sent_at + PERMISSION_TIMEOUT));
    assert!(permission_timed_out(
        sent_at,
        sent_at + PERMISSION_TIMEOUT + Duration::from_secs(1)
    ));
}

#[test]
fn permission_registry_expire_respects_timeout_boundary() {
    let registry = PermissionRegistry::new();
    let sent_at = Instant::now();
    registry.insert(PendingPermissionRequest::new(
        "req-expire",
        7,
        "bash",
        Some("ls".to_string()),
        PermissionRisk::Routine,
        sent_at,
    ));

    assert!(
        registry
            .expire("req-expire", sent_at + Duration::from_secs(29 * 60 + 59))
            .is_none(),
        "a request pending for 29m59s must not expire"
    );
    assert!(registry.get("req-expire").is_some());
    assert_eq!(registry.len(), 1);

    let expired = registry
        .expire("req-expire", sent_at + PERMISSION_TIMEOUT)
        .expect("exactly 30m must expire the request");
    assert_eq!(expired.request_id, "req-expire");
    assert_eq!(expired.chat_id, 7);
    assert!(registry.get("req-expire").is_none());
    assert!(registry.is_empty());
    assert!(registry
        .expire("req-expire", sent_at + PERMISSION_TIMEOUT + Duration::from_secs(1))
        .is_none());

    let resolved_registry = PermissionRegistry::new();
    resolved_registry.insert(PendingPermissionRequest::new(
        "req-resolved",
        8,
        "edit",
        None,
        PermissionRisk::Routine,
        sent_at,
    ));
    assert!(resolved_registry
        .resolve("req-resolved", PermissionDecision::Deny)
        .is_some());
    assert!(resolved_registry
        .expire("req-resolved", sent_at + PERMISSION_TIMEOUT)
        .is_none(),
        "an already resolved request leaves nothing to expire");
    assert!(resolved_registry
        .resolve("req-resolved", PermissionDecision::Deny)
        .is_none());
    assert!(resolved_registry.is_empty());
}

#[test]
fn permission_registry_take_expired_clear_and_remove() {
    let registry = PermissionRegistry::new();
    let now = Instant::now();
    let expired_sent_at = now - PERMISSION_TIMEOUT - Duration::from_secs(1);
    let fresh_sent_at = now - Duration::from_secs(1);

    registry.insert(PendingPermissionRequest::new(
        "req-old-a",
        21,
        "bash",
        Some("cat file".to_string()),
        PermissionRisk::Routine,
        expired_sent_at,
    ));
    registry.insert(PendingPermissionRequest::new(
        "req-old-b",
        22,
        "bash",
        Some("ls".to_string()),
        PermissionRisk::Routine,
        expired_sent_at,
    ));
    registry.insert(PendingPermissionRequest::new(
        "req-fresh",
        23,
        "edit",
        None,
        PermissionRisk::Routine,
        fresh_sent_at,
    ));

    let mut expired_ids: Vec<String> = registry
        .take_expired(now)
        .into_iter()
        .map(|request| request.request_id)
        .collect();
    expired_ids.sort();
    assert_eq!(
        expired_ids,
        vec!["req-old-a".to_string(), "req-old-b".to_string()],
        "take_expired must remove exactly the timed-out requests"
    );
    assert_eq!(registry.len(), 1);
    assert_eq!(
        registry.get("req-fresh").expect("fresh request must remain").chat_id,
        23
    );

    let cleared = registry.clear();
    assert_eq!(cleared.len(), 1);
    assert_eq!(cleared[0].request_id, "req-fresh");
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    let removal_registry = PermissionRegistry::new();
    removal_registry.insert(PendingPermissionRequest::new(
        "req-remove",
        31,
        "bash",
        Some("ls".to_string()),
        PermissionRisk::Routine,
        now,
    ));
    removal_registry.insert(PendingPermissionRequest::new(
        "req-keep",
        32,
        "bash",
        Some("ls".to_string()),
        PermissionRisk::Routine,
        now,
    ));

    let removed = removal_registry
        .remove("req-remove")
        .expect("named request must be removed");
    assert_eq!(removed.request_id, "req-remove");
    assert_eq!(removed.chat_id, 31);
    assert!(removal_registry.remove("req-remove").is_none());
    assert_eq!(removal_registry.len(), 1);
    assert_eq!(
        removal_registry.get("req-keep").expect("unnamed request must remain").chat_id,
        32
    );
}

#[test]
fn permission_risk_labels_are_stable() {
    use super::permission::PermissionRisk;

    assert_eq!(PermissionRisk::Routine.label(), "常规");
    assert_eq!(PermissionRisk::HighRisk.label(), "高危");
}

#[test]
fn permission_card_high_risk_offers_once_and_deny_only() {
    use super::card_builder::CardBuilder;
    use super::permission::PermissionRisk;

    let request_id = "per_1234567890abcdef1234567890abcdef";
    let (text, markup) = CardBuilder::permission_request_card(
        "onespace-app",
        Some("ses_x"),
        "bash",
        Some("rm -rf /tmp/demo"),
        PermissionRisk::HighRisk,
        request_id,
    );

    assert!(text.contains("onespace-app"), "text: {text}");
    assert!(text.contains("ses_x"), "text: {text}");
    assert!(text.contains("bash"), "text: {text}");
    assert!(text.contains("高危"), "text: {text}");
    assert!(text.contains("rm -rf /tmp/demo"), "text: {text}");

    let buttons: Vec<(String, String)> = markup
        .inline_keyboard
        .iter()
        .flat_map(|row| row.iter())
        .map(|button| {
            (
                button.text.clone(),
                button.callback_data.clone().unwrap_or_default(),
            )
        })
        .collect();
    let callbacks: Vec<&str> = buttons.iter().map(|(_, data)| data.as_str()).collect();
    let once_cb = format!("perm_once:{request_id}");
    let deny_cb = format!("perm_deny:{request_id}");
    assert_eq!(callbacks, vec![once_cb.as_str(), deny_cb.as_str()]);
    assert!(buttons.iter().any(|(label, _)| label.contains("允许一次")));
    assert!(buttons.iter().any(|(label, _)| label.contains("拒绝")));
    assert!(
        !callbacks.iter().any(|data| data.starts_with("perm_always:")),
        "high-risk card must not offer always-allow: {callbacks:?}"
    );
}

#[test]
fn permission_card_routine_offers_all_three_choices() {
    use super::card_builder::CardBuilder;
    use super::permission::PermissionRisk;

    let request_id = "per_routine";
    let (text, markup) = CardBuilder::permission_request_card(
        "project-a",
        Some("ses_routine"),
        "read",
        Some("cat Cargo.toml"),
        PermissionRisk::Routine,
        request_id,
    );

    assert!(text.contains("project-a"), "text: {text}");
    assert!(text.contains("ses_routine"), "text: {text}");
    assert!(text.contains("read"), "text: {text}");
    assert!(text.contains("常规"), "text: {text}");
    assert!(text.contains("cat Cargo.toml"), "text: {text}");

    let buttons: Vec<(String, String)> = markup
        .inline_keyboard
        .iter()
        .flat_map(|row| row.iter())
        .map(|button| {
            (
                button.text.clone(),
                button.callback_data.clone().unwrap_or_default(),
            )
        })
        .collect();
    let callbacks: Vec<&str> = buttons.iter().map(|(_, data)| data.as_str()).collect();
    let once_cb = format!("perm_once:{request_id}");
    let always_cb = format!("perm_always:{request_id}");
    let deny_cb = format!("perm_deny:{request_id}");
    assert_eq!(
        callbacks,
        vec![once_cb.as_str(), always_cb.as_str(), deny_cb.as_str()]
    );
    assert!(buttons.iter().any(|(label, _)| label.contains("允许一次")));
    assert!(buttons.iter().any(|(label, _)| label.contains("始终允许")));
    assert!(buttons.iter().any(|(label, _)| label.contains("拒绝")));
}

#[test]
fn permission_card_unassigned_session_is_labelled() {
    use super::card_builder::CardBuilder;
    use super::permission::PermissionRisk;

    let (text, _) = CardBuilder::permission_request_card(
        "project-b",
        None,
        "write",
        Some("touch notes.md"),
        PermissionRisk::HighRisk,
        "per_no_session",
    );
    assert!(text.contains("unassigned"), "text: {text}");
    assert!(!text.contains("ses_x"), "text: {text}");
}

#[test]
fn permission_card_without_command_says_none() {
    use super::card_builder::CardBuilder;
    use super::permission::PermissionRisk;

    let (text, _) = CardBuilder::permission_request_card(
        "project-c",
        Some("ses_y"),
        "bash",
        None,
        PermissionRisk::HighRisk,
        "per_no_command",
    );
    assert!(text.contains("（无命令）"), "text: {text}");
}

#[test]
fn permission_card_bounds_long_command_preview() {
    use super::card_builder::CardBuilder;
    use super::permission::PermissionRisk;

    let raw = format!("{}TAILMARKER{}", "a".repeat(300), "b".repeat(5000));
    let (text, _) = CardBuilder::permission_request_card(
        "project-d",
        Some("ses_long"),
        "bash",
        Some(&raw),
        PermissionRisk::HighRisk,
        "per_long",
    );

    assert!(
        text.chars().count() < raw.chars().count(),
        "card text must be shorter than the raw command"
    );
    assert!(text.ends_with('…'), "text must end with an ellipsis: {text}");
    assert!(
        !text.contains("TAILMARKER"),
        "preview must not exceed 300 command code points: {text}"
    );
    assert!(
        text.chars().count() <= 600,
        "card text is unexpectedly large: {}",
        text.chars().count()
    );
}

#[test]
fn permission_callback_data_stays_within_telegram_limit() {
    use super::card_builder::CardBuilder;
    use super::permission::PermissionRisk;

    let request_id = "per_1234567890abcdef1234567890abcdef";
    let (_, markup) = CardBuilder::permission_request_card(
        "project-e",
        Some("ses_limit"),
        "read",
        Some("ls"),
        PermissionRisk::Routine,
        request_id,
    );
    for row in &markup.inline_keyboard {
        for button in row {
            let data = button.callback_data.as_deref().unwrap_or_default();
            assert!(
                data.len() <= 64,
                "callback data must fit Telegram's 64-byte limit: {data}"
            );
        }
    }
}

#[test]
fn permission_callback_parses_all_variants() {
    use super::permission::{parse_permission_callback, PermissionCallback};

    assert_eq!(
        parse_permission_callback("perm_once:id-1"),
        Some(PermissionCallback::Once("id-1".to_string()))
    );
    assert_eq!(
        parse_permission_callback("perm_always:id-2"),
        Some(PermissionCallback::Always("id-2".to_string()))
    );
    assert_eq!(
        parse_permission_callback("perm_deny:id-3"),
        Some(PermissionCallback::Deny("id-3".to_string()))
    );
}

#[test]
fn permission_callback_keeps_colons_in_identifier() {
    use super::permission::{parse_permission_callback, PermissionCallback};

    assert_eq!(
        parse_permission_callback("perm_once:per:a:b:c"),
        Some(PermissionCallback::Once("per:a:b:c".to_string()))
    );
    assert_eq!(
        parse_permission_callback("perm_always:req:123"),
        Some(PermissionCallback::Always("req:123".to_string()))
    );
}

#[test]
fn permission_callback_rejects_malformed_data() {
    use super::permission::parse_permission_callback;

    for data in [
        "",
        "perm_once",
        "perm_once:",
        "perm_always:",
        "perm_deny:",
        "perm_maybe:x",
        "once:id",
        "perm_once:   ",
        "perm_once:\t\n",
    ] {
        assert_eq!(
            parse_permission_callback(data),
            None,
            "malformed callback must be rejected: {data:?}"
        );
    }
}

// --- Permission policy, auto-approval and card delivery ---
// These tests drive the real poller against a loopback HTTP server so the
// observable contract is the exact reply bytes and Telegram payloads sent over
// the wire. The server records every request and can serve configured
// permission payloads or fail Telegram sendMessage delivery.

use super::client::TelegramClient;
use super::opencode_bridge::OpenCodeBridge;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Debug)]
struct RecordedRequest {
    method: String,
    path: String,
    body: String,
}

struct FakeServerState {
    requests: Mutex<Vec<RecordedRequest>>,
    permissions_json: Mutex<String>,
    fail_send_message: AtomicBool,
    fail_permission_reply: AtomicBool,
}

struct FakeHttpServer {
    addr: SocketAddr,
    state: Arc<FakeServerState>,
    accept_handle: tokio::task::JoinHandle<()>,
}

impl FakeHttpServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake HTTP server");
        let addr = listener.local_addr().expect("fake HTTP server address");
        let state = Arc::new(FakeServerState {
            requests: Mutex::new(Vec::new()),
            permissions_json: Mutex::new("[]".to_string()),
            fail_send_message: AtomicBool::new(false),
            fail_permission_reply: AtomicBool::new(false),
        });
        let accept_state = state.clone();
        let accept_handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let connection_state = accept_state.clone();
                tokio::spawn(async move {
                    fake_handle_connection(stream, connection_state).await;
                });
            }
        });
        Self {
            addr,
            state,
            accept_handle,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn telegram_base_url(&self) -> String {
        format!("http://{}/botTEST", self.addr)
    }

    fn set_permissions_json(&self, json_array: &str) {
        *self.state.permissions_json.lock().unwrap() = json_array.to_string();
    }

    fn fail_send_message(&self, fail: bool) {
        self.state.fail_send_message.store(fail, Ordering::SeqCst);
    }

    /// Make every `/reply` POST answer HTTP 500 so a decision cannot be
    /// forwarded while Telegram card delivery stays unaffected.
    fn fail_permission_reply(&self, fail: bool) {
        self.state
            .fail_permission_reply
            .store(fail, Ordering::SeqCst);
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.state.requests.lock().unwrap().clone()
    }

    fn shutdown(&self) {
        self.accept_handle.abort();
    }
}

fn fake_find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

async fn fake_handle_connection(mut stream: TcpStream, state: Arc<FakeServerState>) {
    let mut received = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        let Ok(read) = stream.read(&mut chunk).await else {
            return;
        };
        if read == 0 {
            return;
        }
        received.extend_from_slice(&chunk[..read]);
        if let Some(head_end) = fake_find_header_end(&received) {
            break head_end;
        }
        if received.len() > 64 * 1024 {
            return;
        }
    };

    let head = String::from_utf8_lossy(&received[..head_end]).to_string();
    let mut head_lines = head.split("\r\n");
    let request_line = head_lines.next().unwrap_or_default();
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();
    let content_length = head_lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .filter_map(|(_, value)| value.trim().parse::<usize>().ok())
        .next()
        .unwrap_or(0);

    let mut body = received[head_end..].to_vec();
    while body.len() < content_length {
        let Ok(read) = stream.read(&mut chunk).await else {
            return;
        };
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    state.requests.lock().unwrap().push(RecordedRequest {
        method: method.clone(),
        path: path.clone(),
        body: String::from_utf8_lossy(&body).to_string(),
    });

    let (status, response_body) = fake_response(&state, &method, &path);
    let status_text = if status == 200 {
        "OK"
    } else {
        "Internal Server Error"
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn fake_response(state: &FakeServerState, method: &str, path: &str) -> (u16, String) {
    let telegram_success = "{\"ok\":true,\"result\":{\"message_id\":777,\"chat\":{\"id\":42,\"type\":\"private\"},\"date\":1,\"text\":\"ok\"}}";
    if method == "GET" && (path == "/question" || path == "/api/form") {
        (200, "{\"data\":[]}".to_string())
    } else if method == "GET" && (path == "/permission" || path == "/api/permission/request") {
        let permissions = state.permissions_json.lock().unwrap().clone();
        (200, format!("{{\"data\":{permissions}}}"))
    } else if method == "POST" && path.ends_with("/reply") {
        if state.fail_permission_reply.load(Ordering::SeqCst) {
            (500, "{\"ok\":false}".to_string())
        } else {
            (200, "{\"ok\":true}".to_string())
        }
    } else if method == "POST" && path.ends_with("/sendMessage") {
        if state.fail_send_message.load(Ordering::SeqCst) {
            (
                500,
                "{\"ok\":false,\"description\":\"delivery failed\"}".to_string(),
            )
        } else {
            (200, telegram_success.to_string())
        }
    } else if method == "POST" && path.ends_with("/editMessageText") {
        (200, telegram_success.to_string())
    } else {
        (404, "{\"ok\":false,\"description\":\"not found\"}".to_string())
    }
}

async fn permission_wait_for_request(
    server: &FakeHttpServer,
    predicate: impl Fn(&RecordedRequest) -> bool,
    label: &str,
) -> RecordedRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(found) = server.requests().into_iter().find(|request| predicate(request)) {
            return found;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {label}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn permission_wait_until(mut predicate: impl FnMut() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if predicate() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {label}");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn permission_test_bridge(server: &FakeHttpServer) -> OpenCodeBridge {
    OpenCodeBridge::new(Arc::new(TelegramClient::with_base_url(
        "test-token",
        server.telegram_base_url(),
    )))
}

fn permission_reply_posts(requests: Vec<RecordedRequest>) -> Vec<RecordedRequest> {
    requests
        .into_iter()
        .filter(|request| request.method == "POST" && request.path.ends_with("/reply"))
        .collect()
}

fn permission_send_messages(requests: Vec<RecordedRequest>) -> Vec<RecordedRequest> {
    requests
        .into_iter()
        .filter(|request| request.method == "POST" && request.path.ends_with("/sendMessage"))
        .collect()
}

fn permission_edit_messages(requests: Vec<RecordedRequest>) -> Vec<RecordedRequest> {
    requests
        .into_iter()
        .filter(|request| request.method == "POST" && request.path.ends_with("/editMessageText"))
        .collect()
}

#[tokio::test]
async fn permission_policy_auto_approves_routine_without_card() {
    let server = FakeHttpServer::start().await;
    server.set_permissions_json(
        r#"[{"id":"per_auto_1","permission":"bash","command":"cargo test"}]"#,
    );
    let bridge = permission_test_bridge(&server);
    let registry = bridge.permission_runtime().registry();
    let cancel = Arc::new(AtomicBool::new(false));
    let policy: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(|| true);
    let handle = bridge.spawn_question_and_permission_poller(
        42,
        1,
        server.url(),
        cancel.clone(),
        Some("proj".to_string()),
        policy,
    );

    let reply = permission_wait_for_request(
        &server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/reply")
                && request.body.contains("\"reply\":\"once\"")
                && request.body.contains("\"response\":\"once\"")
        },
        "auto-approved routine permission reply",
    )
    .await;
    assert!(
        reply.path.contains("per_auto_1"),
        "reply must target the routine request: {}",
        reply.path
    );

    cancel.store(true, Ordering::SeqCst);
    let _ = handle.await;

    let cards = permission_send_messages(server.requests());
    assert!(
        cards.is_empty(),
        "routine auto-approval must not send a permission card: {cards:?}"
    );
    assert!(
        registry.get("per_auto_1").is_none(),
        "auto-approved request must not stay pending"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_poller_high_risk_enabled_requests_approval() {
    let server = FakeHttpServer::start().await;
    server.set_permissions_json(
        r#"[{"id":"per_high_1","permission":"bash","command":"rm -rf target"}]"#,
    );
    let bridge = permission_test_bridge(&server);
    let cancel = Arc::new(AtomicBool::new(false));
    let policy: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(|| true);
    let handle = bridge.spawn_question_and_permission_poller(
        42,
        1,
        server.url(),
        cancel.clone(),
        Some("proj".to_string()),
        policy,
    );

    let card = permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/sendMessage"),
        "high-risk approval card",
    )
    .await;
    assert!(
        card.body.contains("perm_deny:per_high_1"),
        "high-risk card must offer deny: {}",
        card.body
    );
    assert!(
        !card.body.contains("perm_always:per_high_1"),
        "high-risk card must not offer always-allow: {}",
        card.body
    );

    cancel.store(true, Ordering::SeqCst);
    let _ = handle.await;

    let replies = permission_reply_posts(server.requests());
    assert!(
        replies.is_empty(),
        "high-risk requests must wait for a human decision, not auto-reply: {replies:?}"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_policy_disabled_routine_requests_approval() {
    let server = FakeHttpServer::start().await;
    server.set_permissions_json(
        r#"[{"id":"per_routine_1","permission":"bash","command":"cargo test"}]"#,
    );
    let bridge = permission_test_bridge(&server);
    let cancel = Arc::new(AtomicBool::new(false));
    let policy: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(|| false);
    let handle = bridge.spawn_question_and_permission_poller(
        42,
        1,
        server.url(),
        cancel.clone(),
        Some("proj".to_string()),
        policy,
    );

    let card = permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/sendMessage"),
        "routine approval card",
    )
    .await;
    assert!(
        card.body.contains("perm_always:per_routine_1"),
        "routine card must offer always-allow: {}",
        card.body
    );

    cancel.store(true, Ordering::SeqCst);
    let _ = handle.await;

    let replies = permission_reply_posts(server.requests());
    assert!(
        replies.is_empty(),
        "disabled auto-approval must not reply for a routine request: {replies:?}"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_card_delivery_failure_keeps_request_pending() {
    let server = FakeHttpServer::start().await;
    server.set_permissions_json(
        r#"[{"id":"per_fail_1","permission":"bash","command":"rm -rf target"}]"#,
    );
    server.fail_send_message(true);
    let bridge = permission_test_bridge(&server);
    let registry = bridge.permission_runtime().registry();
    let cancel = Arc::new(AtomicBool::new(false));
    let policy: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(|| true);
    let handle = bridge.spawn_question_and_permission_poller(
        42,
        1,
        server.url(),
        cancel.clone(),
        Some("proj".to_string()),
        policy,
    );

    permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/sendMessage"),
        "failed delivery attempt",
    )
    .await;
    permission_wait_until(
        || registry.get("per_fail_1").is_some(),
        "failed permission to stay pending",
    )
    .await;

    cancel.store(true, Ordering::SeqCst);
    let _ = handle.await;

    let replies = permission_reply_posts(server.requests());
    assert!(
        replies.is_empty(),
        "a failed card delivery must not auto-reply: {replies:?}"
    );
    assert!(
        registry.get("per_fail_1").is_some(),
        "the undelivered request must remain pending"
    );
    server.shutdown();
}

// --- Permission decisions ---
// A decision is single-use: exactly one reply reaches the agent, the delivered
// card is edited in place to show the outcome, and any reply failure keeps the
// request pending so the operator can tap again.

use super::opencode_bridge::PermissionDecisionOutcome;

fn permission_pending(
    request_id: &str,
    risk: PermissionRisk,
    sent_at: Instant,
) -> PendingPermissionRequest {
    PendingPermissionRequest::new(
        request_id,
        42,
        "bash",
        Some("cargo test".to_string()),
        risk,
        sent_at,
    )
    .with_card_message_id(Some(777))
    .with_session_id(None)
}

#[tokio::test]
async fn permission_decide_once_replies_once_and_edits_card() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    registry.insert(permission_pending(
        "per_once_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));

    let outcome = runtime
        .decide(&server.url(), "per_once_1", PermissionDecision::Once)
        .await;
    assert_eq!(
        outcome,
        PermissionDecisionOutcome::Applied(PermissionDecision::Once)
    );

    let reply = permission_wait_for_request(
        &server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/reply")
                && request.path.contains("per_once_1")
        },
        "once permission reply",
    )
    .await;
    assert!(
        reply.body.contains("\"reply\":\"once\""),
        "reply payload must forward once: {}",
        reply.body
    );
    assert!(
        reply.body.contains("\"response\":\"once\""),
        "reply payload must forward once as response: {}",
        reply.body
    );

    permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/editMessageText"),
        "card edit after once decision",
    )
    .await;
    let edits = permission_edit_messages(server.requests());
    assert_eq!(edits.len(), 1, "exactly one card edit: {edits:?}");
    assert!(
        edits[0].body.contains("已允许一次"),
        "edited card must show the once outcome: {}",
        edits[0].body
    );
    assert!(
        registry.get("per_once_1").is_none(),
        "a resolved request must leave the registry"
    );

    let outcome = runtime
        .decide(&server.url(), "per_once_1", PermissionDecision::Once)
        .await;
    assert_eq!(outcome, PermissionDecisionOutcome::Ignored);

    tokio::time::sleep(Duration::from_millis(100)).await;
    let replies = permission_reply_posts(server.requests());
    assert_eq!(replies.len(), 1, "single-use decision sends one reply: {replies:?}");
    let edits = permission_edit_messages(server.requests());
    assert_eq!(edits.len(), 1, "single-use decision edits one card: {edits:?}");
    server.shutdown();
}

#[tokio::test]
async fn permission_decide_always_forwards_always() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    registry.insert(permission_pending(
        "per_always_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));

    let outcome = runtime
        .decide(&server.url(), "per_always_1", PermissionDecision::Always)
        .await;
    assert_eq!(
        outcome,
        PermissionDecisionOutcome::Applied(PermissionDecision::Always)
    );

    let reply = permission_wait_for_request(
        &server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/reply")
                && request.path.contains("per_always_1")
        },
        "always permission reply",
    )
    .await;
    assert!(
        reply.body.contains("\"reply\":\"always\""),
        "reply payload must forward always: {}",
        reply.body
    );

    let edit = permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/editMessageText"),
        "card edit after always decision",
    )
    .await;
    assert!(
        edit.body.contains("已始终允许"),
        "edited card must show the always outcome: {}",
        edit.body
    );
    assert!(
        registry.get("per_always_1").is_none(),
        "a resolved request must leave the registry"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_decide_deny_forwards_deny() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    registry.insert(permission_pending(
        "per_deny_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));

    let outcome = runtime
        .decide(&server.url(), "per_deny_1", PermissionDecision::Deny)
        .await;
    assert_eq!(
        outcome,
        PermissionDecisionOutcome::Applied(PermissionDecision::Deny)
    );

    let reply = permission_wait_for_request(
        &server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/reply")
                && request.path.contains("per_deny_1")
        },
        "deny permission reply",
    )
    .await;
    assert!(
        reply.body.contains("\"reply\":\"deny\""),
        "reply payload must forward deny: {}",
        reply.body
    );

    let edit = permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/editMessageText"),
        "card edit after deny decision",
    )
    .await;
    assert!(
        edit.body.contains("已拒绝"),
        "edited card must show the deny outcome: {}",
        edit.body
    );
    assert!(
        registry.get("per_deny_1").is_none(),
        "a resolved request must leave the registry"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_decide_reply_failure_keeps_pending() {
    let server = FakeHttpServer::start().await;
    server.fail_permission_reply(true);
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    registry.insert(permission_pending(
        "per_fail_reply_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));

    let outcome = runtime
        .decide(&server.url(), "per_fail_reply_1", PermissionDecision::Once)
        .await;
    assert_eq!(outcome, PermissionDecisionOutcome::ReplyFailed);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !permission_reply_posts(server.requests()).is_empty(),
        "the decision must have been attempted"
    );
    assert!(
        permission_edit_messages(server.requests()).is_empty(),
        "a failed reply must not claim the card was decided"
    );
    assert!(
        registry.get("per_fail_reply_1").is_some(),
        "a failed reply keeps the request pending"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_decide_ignores_high_risk_always() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    registry.insert(permission_pending(
        "per_high_always_1",
        PermissionRisk::HighRisk,
        Instant::now(),
    ));

    let outcome = runtime
        .decide(
            &server.url(),
            "per_high_always_1",
            PermissionDecision::Always,
        )
        .await;
    assert_eq!(outcome, PermissionDecisionOutcome::Ignored);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        permission_reply_posts(server.requests()).is_empty(),
        "a high-risk request must never honor always"
    );
    assert!(
        permission_edit_messages(server.requests()).is_empty(),
        "an ignored decision must not edit the card"
    );
    assert!(
        registry.get("per_high_always_1").is_some(),
        "an ignored decision keeps the request pending"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_decide_expired_tap_is_ignored() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    let sent_at = Instant::now()
        .checked_sub(PERMISSION_TIMEOUT + Duration::from_secs(1))
        .expect("clock");
    registry.insert(permission_pending(
        "per_expired_1",
        PermissionRisk::Routine,
        sent_at,
    ));

    let outcome = runtime
        .decide(&server.url(), "per_expired_1", PermissionDecision::Once)
        .await;
    assert_eq!(outcome, PermissionDecisionOutcome::Expired);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        permission_reply_posts(server.requests()).is_empty(),
        "an expired request must not reply"
    );
    assert!(
        registry.get("per_expired_1").is_some(),
        "an expired tap leaves the stale request untouched"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_decide_unknown_is_ignored() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();

    let outcome = runtime
        .decide(&server.url(), "per_unknown_1", PermissionDecision::Once)
        .await;
    assert_eq!(outcome, PermissionDecisionOutcome::Ignored);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        server.requests().is_empty(),
        "an unknown request must not produce traffic: {:?}",
        server.requests()
    );
    server.shutdown();
}

// --- Pending permission timeout sweep ---
// An unanswered request is denied exactly once when the 30-minute deadline
// passes: the agent receives `deny`, the delivered card is replaced with the
// timeout result and the chat is notified. The poller performs the same sweep,
// so a request that vanished from the agent listing still expires, and ending
// or cancelling a task drops every pending request.

#[tokio::test]
async fn permission_timeout_denies_and_notifies() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    let now = Instant::now();
    registry.insert(permission_pending(
        "per_to_1",
        PermissionRisk::Routine,
        now,
    ));

    let expired = runtime
        .expire_due(&server.url(), now + PERMISSION_TIMEOUT)
        .await;
    assert_eq!(expired, 1, "one request must be swept");

    let reply = permission_wait_for_request(
        &server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/reply")
                && request.path.contains("per_to_1")
        },
        "timeout deny reply",
    )
    .await;
    assert!(
        reply.body.contains("\"reply\":\"deny\""),
        "timeout must reply deny: {}",
        reply.body
    );

    let edit = permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/editMessageText"),
        "timeout card edit",
    )
    .await;
    assert!(
        edit.body.contains("已超时") && edit.body.contains("自动拒绝"),
        "timeout card must show both timeout and auto-deny: {}",
        edit.body
    );

    let notification = permission_wait_for_request(
        &server,
        |request| request.method == "POST" && request.path.ends_with("/sendMessage"),
        "timeout chat notification",
    )
    .await;
    assert!(
        notification.body.contains("自动拒绝"),
        "timeout notification must mention auto-deny: {}",
        notification.body
    );

    assert!(
        registry.get("per_to_1").is_none(),
        "a swept request must leave the registry"
    );

    let again = runtime
        .expire_due(&server.url(), now + PERMISSION_TIMEOUT * 2)
        .await;
    assert_eq!(again, 0, "a repeated sweep must not expire anything");

    tokio::time::sleep(Duration::from_millis(100)).await;
    let replies = permission_reply_posts(server.requests());
    assert_eq!(
        replies.len(),
        1,
        "a timed-out request must be denied exactly once: {replies:?}"
    );
    let edits = permission_edit_messages(server.requests());
    assert_eq!(edits.len(), 1, "exactly one timeout card edit: {edits:?}");
    server.shutdown();
}

#[tokio::test]
async fn permission_timeout_reply_failure_retries_before_commit() {
    let failing_server = FakeHttpServer::start().await;
    failing_server.fail_permission_reply(true);
    let succeeding_server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&failing_server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    let now = Instant::now();
    registry.insert(permission_pending(
        "per_tmo_fail_1",
        PermissionRisk::Routine,
        now,
    ));

    let expired = runtime
        .expire_due(&failing_server.url(), now + PERMISSION_TIMEOUT)
        .await;
    assert_eq!(
        expired, 0,
        "a failed deny reply must not commit the timeout sweep"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        registry.get("per_tmo_fail_1").is_some(),
        "a failed deny reply keeps the request registered for retry"
    );
    assert!(
        permission_edit_messages(failing_server.requests()).is_empty(),
        "a failed deny reply must not edit the card"
    );
    assert!(
        permission_send_messages(failing_server.requests()).is_empty(),
        "a failed deny reply must not notify the chat"
    );

    let retried = runtime
        .expire_due(
            &succeeding_server.url(),
            now + PERMISSION_TIMEOUT * 2,
        )
        .await;
    assert_eq!(retried, 1, "the retried sweep must deny the request");

    let reply = permission_wait_for_request(
        &succeeding_server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/reply")
                && request.path.contains("per_tmo_fail_1")
        },
        "retried timeout deny reply",
    )
    .await;
    assert!(
        reply.body.contains("\"reply\":\"deny\""),
        "retried timeout must reply deny: {}",
        reply.body
    );

    let edit = permission_wait_for_request(
        &failing_server,
        |request| request.method == "POST" && request.path.ends_with("/editMessageText"),
        "timeout card edit after retry",
    )
    .await;
    assert!(
        edit.body.contains("已超时"),
        "retried timeout card must show the timeout result: {}",
        edit.body
    );

    let notification = permission_wait_for_request(
        &failing_server,
        |request| request.method == "POST" && request.path.ends_with("/sendMessage"),
        "timeout notification after retry",
    )
    .await;
    assert!(
        notification.body.contains("自动拒绝"),
        "retried timeout notification must mention auto-deny: {}",
        notification.body
    );

    assert!(
        registry.get("per_tmo_fail_1").is_none(),
        "a committed timeout must leave the registry"
    );
    failing_server.shutdown();
    succeeding_server.shutdown();
}

#[tokio::test]
async fn permission_timeout_sweep_boundary() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    let now = Instant::now();
    registry.insert(permission_pending(
        "per_bound_1",
        PermissionRisk::Routine,
        now,
    ));

    let before = runtime
        .expire_due(
            &server.url(),
            now + PERMISSION_TIMEOUT - Duration::from_secs(1),
        )
        .await;
    assert_eq!(before, 0, "29m59s must still be pending");
    assert!(
        registry.get("per_bound_1").is_some(),
        "a request short of the deadline stays registered"
    );

    let at_deadline = runtime
        .expire_due(&server.url(), now + PERMISSION_TIMEOUT)
        .await;
    assert_eq!(at_deadline, 1, "exactly 30 minutes must expire");
    assert!(
        registry.get("per_bound_1").is_none(),
        "the expired request leaves the registry"
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_timeout_ignores_resolved() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let runtime = bridge.permission_runtime();
    let registry = runtime.registry();
    let now = Instant::now();
    registry.insert(permission_pending(
        "per_resolved_1",
        PermissionRisk::Routine,
        now,
    ));
    let resolved = registry
        .resolve("per_resolved_1", PermissionDecision::Once)
        .expect("the pending request must resolve");
    assert_eq!(resolved.decision, PermissionDecision::Once);

    let expired = runtime
        .expire_due(&server.url(), now + PERMISSION_TIMEOUT)
        .await;
    assert_eq!(expired, 0, "a resolved request must never time out");

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        server.requests().is_empty(),
        "a resolved request must not be denied, edited or announced: {:?}",
        server.requests()
    );
    server.shutdown();
}

#[tokio::test]
async fn permission_clear_on_task_end_clears_pending() {
    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));
    let registry = bridge.permission_runtime().registry();
    registry.insert(permission_pending(
        "per_end_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));

    bridge.finish_task().await;

    assert!(
        registry.is_empty(),
        "finishing a task must drop every pending permission"
    );
    assert!(
        registry.get("per_end_1").is_none(),
        "the finished task's pending request must be gone"
    );
}

#[tokio::test]
async fn permission_clear_on_cancel_clears_pending() {
    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));
    let registry = bridge.permission_runtime().registry();
    registry.insert(permission_pending(
        "per_cancel_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));

    bridge.cancel_active_task().await;

    assert!(
        registry.is_empty(),
        "cancelling a task must drop every pending permission"
    );
    assert!(
        registry.get("per_cancel_1").is_none(),
        "the cancelled task's pending request must be gone"
    );
}

#[tokio::test]
async fn permission_poller_expiry_clears_stale_request() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let registry = bridge.permission_runtime().registry();
    let sent_at = Instant::now()
        .checked_sub(PERMISSION_TIMEOUT + Duration::from_secs(1))
        .expect("clock");
    registry.insert(
        PendingPermissionRequest::new(
            "per_stale_1",
            42,
            "bash",
            Some("rm -rf target".to_string()),
            PermissionRisk::HighRisk,
            sent_at,
        )
        .with_card_message_id(Some(778))
        .with_session_id(None),
    );

    // The agent no longer lists the request, so only the poller's own sweep
    // can observe that it has passed the deadline.
    server.set_permissions_json("[]");
    let cancel = Arc::new(AtomicBool::new(false));
    let policy: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(|| false);
    let handle = bridge.spawn_question_and_permission_poller(
        42,
        1,
        server.url(),
        cancel.clone(),
        Some("proj".to_string()),
        policy,
    );

    let reply = permission_wait_for_request(
        &server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/reply")
                && request.path.contains("per_stale_1")
                && request.body.contains("\"reply\":\"deny\"")
        },
        "poller timeout deny reply",
    )
    .await;
    assert!(
        reply.body.contains("\"reply\":\"deny\""),
        "poller expiry must reply deny: {}",
        reply.body
    );

    permission_wait_for_request(
        &server,
        |request| {
            request.method == "POST"
                && request.path.ends_with("/sendMessage")
                && request.body.contains("自动拒绝")
        },
        "poller timeout notification",
    )
    .await;

    permission_wait_until(
        || registry.get("per_stale_1").is_none(),
        "the poller to drop the stale request",
    )
    .await;

    cancel.store(true, Ordering::SeqCst);
    let _ = handle.await;
    server.shutdown();
}

// --- Active task cancellation ---
// Cancelling must terminate the owned process group, escalate when a child
// ignores SIGTERM, leave unrelated groups running, suppress a late result from
// the cancelled generation, and retire pending interaction cards.

use super::opencode_bridge::CancelOutcome;
use std::path::Path;

/// Best-effort process-group cleanup for a test that fails before reaping.
#[cfg(unix)]
struct CancelGroupGuard {
    pgid: u32,
}

#[cfg(unix)]
impl Drop for CancelGroupGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("kill")
            .arg("-9")
            .arg(format!("-{}", self.pgid))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Spawn a child in its own process group so group signals are observable.
#[cfg(unix)]
async fn cancel_spawn_group(
    program: &str,
    args: &[&str],
) -> (tokio::process::Child, CancelGroupGuard) {
    use std::os::unix::process::CommandExt;

    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.as_std_mut().process_group(0);
    let child = command.spawn().expect("spawn grouped child");
    let pgid = child.id().expect("grouped child pid");
    (child, CancelGroupGuard { pgid })
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_process_group_terminates_sleep_child() {
    use super::opencode_bridge::terminate_process_group;

    let (mut child, _guard) = cancel_spawn_group("/bin/sleep", &["30"]).await;
    let pid = child.id().expect("sleep pid");

    let outcome = terminate_process_group(pid, Duration::from_secs(2)).await;
    assert_eq!(outcome, CancelOutcome::Cancelled);

    let waited = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    assert!(
        waited.is_ok(),
        "a terminated sleep child must be reaped within two seconds"
    );
    let _ = waited;
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_process_group_force_kills_after_grace() {
    use super::opencode_bridge::terminate_process_group;

    let (mut child, _guard) =
        cancel_spawn_group("/bin/sh", &["-c", "trap \"\" TERM; sleep 30"]).await;
    let pid = child.id().expect("shell pid");

    let outcome = terminate_process_group(pid, Duration::from_millis(300)).await;
    assert_eq!(outcome, CancelOutcome::ForcedCancelled);

    let waited = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    assert!(
        waited.is_ok(),
        "a SIGTERM-ignoring child must exit after the forced SIGKILL"
    );
    let _ = waited;
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_process_group_leaves_unrelated_group_running() {
    use super::opencode_bridge::terminate_process_group;

    let (mut child_a, _guard_a) = cancel_spawn_group("/bin/sleep", &["30"]).await;
    let pid_a = child_a.id().expect("first sleep pid");
    let (mut child_b, _guard_b) = cancel_spawn_group("/bin/sleep", &["30"]).await;
    let pid_b = child_b.id().expect("second sleep pid");

    let outcome = terminate_process_group(pid_a, Duration::from_secs(2)).await;
    assert_eq!(outcome, CancelOutcome::Cancelled);

    let waited = tokio::time::timeout(Duration::from_secs(2), child_a.wait()).await;
    assert!(waited.is_ok(), "the targeted group must be reaped");
    let _ = waited;
    assert!(
        child_b.try_wait().expect("probe unrelated child").is_none(),
        "an unrelated process group must keep running"
    );

    let _ = terminate_process_group(pid_b, Duration::from_secs(2)).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), child_b.wait()).await;
}

#[tokio::test]
async fn cancel_reports_nothing_when_idle() {
    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));
    let generation_before = bridge.task_generation();
    let snapshot_before = bridge.session_snapshot().await;

    let outcome = bridge.cancel_active_task().await;

    assert_eq!(outcome, CancelOutcome::NothingToCancel);
    assert_eq!(bridge.task_generation(), generation_before);
    let snapshot_after = bridge.session_snapshot().await;
    assert!(snapshot_after.task.is_none());
    assert_eq!(
        snapshot_after.conversation_busy,
        snapshot_before.conversation_busy
    );
    assert_eq!(
        snapshot_after.conversation_session_id,
        snapshot_before.conversation_session_id
    );
    assert_eq!(
        snapshot_after.conversation_project_name,
        snapshot_before.conversation_project_name
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_terminates_owned_process_and_suppresses_late_result() {
    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));
    let (mut child, _guard) = cancel_spawn_group("/bin/sleep", &["30"]).await;
    let pid = child.id().expect("sleep pid");

    bridge.track_active_process(pid).await;
    bridge.begin_conversation(Path::new("/tmp/cancel-a")).await;
    let old_generation = bridge.task_generation();

    let outcome = bridge.cancel_active_task().await;
    assert_eq!(outcome, CancelOutcome::Cancelled);

    let waited = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    assert!(
        waited.is_ok(),
        "cancelling must terminate the tracked process group"
    );
    let _ = waited;

    assert!(
        !bridge.should_publish_result(old_generation),
        "a result from the cancelled generation must be suppressed"
    );
    let current_generation = bridge.task_generation();
    assert!(
        bridge.should_publish_result(current_generation),
        "the current generation must stay publishable"
    );
    assert!(
        bridge.session_snapshot().await.task.is_none(),
        "cancelling must clear the active task snapshot"
    );
}

#[tokio::test]
async fn cancel_edits_pending_interaction_cards() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let registry = bridge.permission_runtime().registry();
    registry.insert(permission_pending(
        "per_cancel_card_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));
    bridge.track_question_message(778).await;
    bridge.begin_conversation(Path::new("/tmp/cancel-cards")).await;

    let outcome = bridge.cancel_active_task().await;
    assert_eq!(outcome, CancelOutcome::Cancelled);
    assert!(registry.is_empty(), "cancel must drop pending permissions");

    permission_wait_until(
        || {
            permission_edit_messages(server.requests())
                .iter()
                .filter(|request| request.body.contains("已取消"))
                .count()
                == 2
        },
        "both pending interaction cards to be edited as cancelled",
    )
    .await;
    let cancelled_edits: Vec<RecordedRequest> = permission_edit_messages(server.requests())
        .into_iter()
        .filter(|request| request.body.contains("已取消"))
        .collect();
    assert_eq!(
        cancelled_edits.len(),
        2,
        "the pending question and permission cards must both be cancelled: {cancelled_edits:?}"
    );

    let expired = bridge
        .permission_runtime()
        .expire_due(&server.url(), Instant::now() + PERMISSION_TIMEOUT * 2)
        .await;
    assert_eq!(
        expired, 0,
        "cancelled cards must not expire or be denied later"
    );
    let deny_replies = permission_reply_posts(server.requests())
        .into_iter()
        .filter(|request| request.body.contains("\"reply\":\"deny\""))
        .count();
    assert_eq!(
        deny_replies, 0,
        "a cancelled pending card must not send a deny reply"
    );
    server.shutdown();
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_forced_outcome_reported() {
    use super::opencode_bridge::CANCEL_GRACE;

    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));
    let (mut child, _guard) =
        cancel_spawn_group("/bin/sh", &["-c", "trap \"\" TERM; sleep 30"]).await;
    let pid = child.id().expect("shell pid");

    bridge.track_active_process(pid).await;
    bridge.begin_conversation(Path::new("/tmp/cancel-forced")).await;

    let outcome = bridge.cancel_active_task().await;
    assert_eq!(outcome, CancelOutcome::ForcedCancelled);

    let waited = tokio::time::timeout(CANCEL_GRACE + Duration::from_secs(3), child.wait()).await;
    assert!(
        waited.is_ok(),
        "forced cancellation must reap the SIGTERM-ignoring child"
    );
    let _ = waited;
}

#[tokio::test]
async fn cancel_late_generation_does_not_clear_newer_task() {
    use super::opencode_bridge::BridgeTaskKind;

    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));
    bridge.begin_conversation(Path::new("/tmp/cancel-old")).await;
    let old_generation = bridge.task_generation();

    let outcome = bridge.cancel_active_task().await;
    assert_eq!(outcome, CancelOutcome::Cancelled);

    bridge.begin_conversation(Path::new("/tmp/cancel-new")).await;
    assert!(
        !bridge.should_publish_result(old_generation),
        "the cancelled generation must stay stale after a newer task starts"
    );
    assert!(
        bridge.should_publish_result(bridge.task_generation()),
        "the newer conversation generation must be publishable"
    );

    let snapshot = bridge.session_snapshot().await;
    let task = snapshot
        .task
        .expect("the newer conversation must remain the active task");
    assert_eq!(task.kind, BridgeTaskKind::Conversation);
    assert_eq!(task.project_name.as_deref(), Some("cancel-new"));
    assert_eq!(
        snapshot.conversation_project_name.as_deref(),
        Some("cancel-new")
    );
}

// --- Bridge shutdown ---
// Shutdown must retire the tracked task process group and the bridge-owned
// server child, clear observable state, and stay safe when repeated or idle.

/// Poll until the process group led by `pid` no longer exists.
#[cfg(unix)]
async fn cancel_wait_group_exits(pid: u32, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let alive = std::process::Command::new("kill")
            .arg("-0")
            .arg(format!("-{pid}"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !alive {
            return;
        }
        if Instant::now() >= deadline {
            panic!("{label} process group {pid} must exit within three seconds");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_shutdown_terminates_task_and_server() {
    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));
    let (mut task_child, _task_guard) = cancel_spawn_group("/bin/sleep", &["30"]).await;
    let task_pid = task_child.id().expect("task sleep pid");
    bridge.track_active_process(task_pid).await;
    bridge.begin_conversation(Path::new("/tmp/cancel-shutdown")).await;

    let (server_child, _server_guard) = cancel_spawn_group("/bin/sleep", &["30"]).await;
    let server_pid = server_child.id().expect("server sleep pid");
    bridge.set_server_process_for_test(server_child).await;

    let old_generation = bridge.task_generation();

    let shutdown = tokio::time::timeout(Duration::from_secs(3), bridge.shutdown()).await;
    assert!(
        shutdown.is_ok(),
        "bridge shutdown must complete without hanging"
    );

    let task_exited = tokio::time::timeout(Duration::from_secs(3), task_child.wait()).await;
    assert!(
        task_exited.is_ok(),
        "shutdown must terminate and reap the tracked task process group"
    );
    cancel_wait_group_exits(server_pid, "bridge-owned server").await;

    assert!(
        bridge.session_snapshot().await.task.is_none(),
        "shutdown must clear the active task snapshot"
    );
    assert!(
        !bridge.should_publish_result(old_generation),
        "shutdown must suppress results from the retired generation"
    );
    assert!(
        bridge.permission_runtime().registry().is_empty(),
        "shutdown must clear pending permissions"
    );
}

#[tokio::test]
async fn cancel_shutdown_is_idempotent() {
    let bridge = OpenCodeBridge::new(Arc::new(TelegramClient::new("dummy-test-token")));

    for attempt in 1..=2 {
        let shutdown = tokio::time::timeout(Duration::from_secs(3), bridge.shutdown()).await;
        assert!(
            shutdown.is_ok(),
            "shutdown attempt {attempt} must complete without hanging"
        );
        assert!(
            bridge.session_snapshot().await.task.is_none(),
            "shutdown attempt {attempt} must leave no active task"
        );
        assert!(
            bridge.permission_runtime().registry().is_empty(),
            "shutdown attempt {attempt} must leave no pending permissions"
        );
    }
}

#[tokio::test]
async fn cancel_shutdown_clears_pending_permission() {
    let server = FakeHttpServer::start().await;
    let bridge = permission_test_bridge(&server);
    let registry = bridge.permission_runtime().registry();
    registry.insert(permission_pending(
        "per_shutdown_1",
        PermissionRisk::Routine,
        Instant::now(),
    ));
    assert_eq!(registry.len(), 1, "pending permission must be installed");

    bridge.shutdown().await;

    assert!(
        registry.is_empty(),
        "shutdown must clear the pending permission registry"
    );
    let expired = bridge
        .permission_runtime()
        .expire_due(&server.url(), Instant::now() + PERMISSION_TIMEOUT * 2)
        .await;
    assert_eq!(
        expired, 0,
        "a shutdown-cleared permission must not time out or be denied later"
    );

    server.shutdown();
}


fn hosted_test_session(session_id: &str, pid: u32) -> super::types::ExternalOpenCodeSessionInfo {
    super::types::ExternalOpenCodeSessionInfo {
        pid,
        ppid: 1,
        elapsed: "00:00".to_string(),
        command: "opencode".to_string(),
        cwd: "/tmp".to_string(),
        project_name: "proj".to_string(),
        session_id: Some(session_id.to_string()),
        title: None,
        model: None,
        agent: None,
        is_busy: false,
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

fn hosted_test_question(call_id: &str, question_text: &str) -> super::types::HostedQuestionItem {
    super::types::HostedQuestionItem {
        call_id: call_id.to_string(),
        question_text: question_text.to_string(),
        options: vec![super::types::HostedOptionItem {
            index: 1,
            label: "A".to_string(),
            description: None,
            is_recommended: false,
        }],
    }
}

fn hosted_test_supervisor() -> super::session_supervisor::SessionSupervisor {
    let client = Arc::new(super::client::TelegramClient::with_base_url(
        "test-token",
        "http://127.0.0.1:9",
    ));
    let bridge = Arc::new(super::opencode_bridge::OpenCodeBridge::new(client.clone()));
    super::session_supervisor::SessionSupervisor::new(client, bridge)
}

#[tokio::test]
async fn hosted_failed_answer_keeps_question_retryable() {
    let client = Arc::new(super::client::TelegramClient::with_base_url(
        "test-token",
        "http://127.0.0.1:9",
    ));
    let bridge = Arc::new(super::opencode_bridge::OpenCodeBridge::new(client.clone()));
    let supervisor = super::session_supervisor::SessionSupervisor::new(client, bridge.clone());
    supervisor
        .start_hosting(
            42,
            hosted_test_session("ses_hosted_1", 4242),
            super::types::SessionHostingDecisionMode::Manual,
        )
        .await
        .expect("hosting must start");

    let question = hosted_test_question("call-1", "Choose an option");
    supervisor
        .set_active_question("ses_hosted_1", question.clone())
        .await;

    bridge.set_busy_for_test(true);
    let result = supervisor
        .handle_answer_question(42, "ses_hosted_1", 1)
        .await;
    assert!(result.is_err(), "busy injection must fail");

    assert_eq!(
        supervisor.get_active_question("ses_hosted_1").await,
        Some(question),
        "a failed injection must keep the question retryable"
    );
    let hosted = supervisor.list_hosted_sessions(Some(42)).await;
    let record = hosted
        .iter()
        .find(|h| h.session_id == "ses_hosted_1")
        .expect("session must still be hosted");
    assert_eq!(
        record.active_question_summary,
        Some("Choose an option".to_string())
    );

    bridge.set_busy_for_test(false);
    supervisor.stop_hosting("ses_hosted_1").await;
}

#[tokio::test]
async fn hosted_stale_question_clear_and_detection() {
    use super::session_supervisor::question_stale;

    assert!(!question_stale(Some("call-1"), Some("call-1")));
    assert!(question_stale(Some("call-1"), Some("call-2")));
    assert!(question_stale(Some("call-1"), None));
    assert!(!question_stale(None, Some("call-1")));

    let supervisor = hosted_test_supervisor();
    supervisor
        .start_hosting(
            43,
            hosted_test_session("ses_hosted_2", 4343),
            super::types::SessionHostingDecisionMode::Manual,
        )
        .await
        .expect("hosting must start");
    supervisor
        .set_active_question("ses_hosted_2", hosted_test_question("call-1", "Choose an option"))
        .await;
    assert!(supervisor.get_active_question("ses_hosted_2").await.is_some());

    supervisor.clear_active_question("ses_hosted_2").await;
    assert_eq!(supervisor.get_active_question("ses_hosted_2").await, None);
    let hosted = supervisor.list_hosted_sessions(Some(43)).await;
    let record = hosted
        .iter()
        .find(|h| h.session_id == "ses_hosted_2")
        .expect("session must still be hosted");
    assert_eq!(record.active_question_summary, None);

    supervisor.stop_hosting("ses_hosted_2").await;
}

#[test]
fn hosted_idle_resets_while_question_pending() {
    use super::session_supervisor::next_idle_checks;

    assert_eq!(next_idle_checks(1, false, true), 0);
    assert_eq!(next_idle_checks(1, true, false), 0);
    assert_eq!(next_idle_checks(0, false, false), 1);
    assert_eq!(next_idle_checks(1, false, false), 2);
}

#[test]
fn hosted_completion_waits_for_pending_question() {
    use super::session_supervisor::completion_due;

    assert!(!completion_due(2, true));
    assert!(completion_due(2, false));
    assert!(!completion_due(1, false));
    assert!(!completion_due(5, true));
}

// ---------------------------------------------------------------------------
// Hosted question visibility: `/host` overview, `/sessions` detail re-send,
// and expired-question status.
// ---------------------------------------------------------------------------

fn hosted_ui_session(question_summary: Option<&str>) -> super::types::HostedSessionInfo {
    super::types::HostedSessionInfo {
        session_id: "ses_hosted_ui".to_string(),
        pid: 4242,
        cwd: "/tmp/proj".to_string(),
        project_name: "proj".to_string(),
        chat_id: 42,
        mode: super::types::SessionHostingDecisionMode::Manual,
        started_at: 1,
        last_observed_step: 3,
        completed_todos_count: 1,
        total_todos_count: 2,
        active_question_summary: question_summary.map(|s| s.to_string()),
    }
}

fn flatten_callbacks(markup: &InlineKeyboardMarkup) -> Vec<String> {
    markup
        .inline_keyboard
        .iter()
        .flatten()
        .filter_map(|button| button.callback_data.clone())
        .collect()
}

fn flatten_labels(markup: &InlineKeyboardMarkup) -> Vec<String> {
    markup
        .inline_keyboard
        .iter()
        .flatten()
        .map(|button| button.text.clone())
        .collect()
}

#[test]
fn hosted_overview_card_shows_pending_question() {
    use super::card_builder::CardBuilder;

    let session = hosted_ui_session(Some("Choose deployment target"));
    let (text, markup) = CardBuilder::hosted_sessions_overview_card(&[session]);
    assert!(text.contains("待答"), "overview text: {}", text);
    assert!(text.contains("Choose deployment target"));
    let callbacks = flatten_callbacks(&markup);
    let labels = flatten_labels(&markup);
    assert!(callbacks.iter().any(|c| c == "host_question:ses_hosted_ui"));
    assert!(labels.iter().any(|l| l.contains("查看待答提问")));
    assert!(callbacks.iter().any(|c| c.starts_with("toggle_host_mode:")));
    assert!(callbacks.iter().any(|c| c.starts_with("stop_host:")));
}

#[test]
fn hosted_overview_card_without_question_has_no_resend() {
    use super::card_builder::CardBuilder;

    let session = hosted_ui_session(None);
    let (text, markup) = CardBuilder::hosted_sessions_overview_card(&[session]);
    assert!(!text.contains("待答"), "overview text: {}", text);
    let callbacks = flatten_callbacks(&markup);
    assert!(!callbacks.iter().any(|c| c.starts_with("host_question:")));
}

#[test]
fn hosted_session_detail_card_shows_question_and_action() {
    use super::card_builder::CardBuilder;
    use super::session_inventory::{SessionEntry, SessionEntryKind};

    let entry = SessionEntry {
        primary_kind: SessionEntryKind::Hosted,
        kinds: vec![SessionEntryKind::Hosted],
        session_id: Some("ses_hosted_ui".to_string()),
        session_id_marker: String::new(),
        plan_id: None,
        pid: Some(4242),
        project_name: "proj".to_string(),
        status_label: "Hosted".to_string(),
        is_busy: false,
    };

    let (text, markup) = CardBuilder::session_detail_card_with_hosted_question(
        &entry,
        None,
        Some("Choose deployment target"),
    );
    assert!(text.contains("待答提问"), "detail text: {}", text);
    assert!(text.contains("Choose deployment target"));
    let markup = markup.expect("hosted question detail must carry a keyboard");
    assert!(flatten_callbacks(&markup)
        .iter()
        .any(|c| c == "host_question:ses_hosted_ui"));

    let (plain_text, plain_markup) =
        CardBuilder::session_detail_card_with_hosted_question(&entry, None, None);
    assert!(!plain_text.contains("待答提问"), "detail text: {}", plain_text);
    let plain_callbacks = plain_markup
        .as_ref()
        .map(flatten_callbacks)
        .unwrap_or_default();
    assert!(!plain_callbacks.iter().any(|c| c.starts_with("host_question:")));
}

#[test]
fn hosted_question_expired_card_reports_status() {
    use super::card_builder::CardBuilder;

    let text = CardBuilder::hosted_question_expired_card("ses_hosted_ui");
    assert!(text.contains("已不再等待"), "expired card: {}", text);
    assert!(text.contains("ses_hosted_ui"));
    assert!(!text.contains("决策已收到"));
}
