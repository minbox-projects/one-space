use super::commands::resolve_host;
use super::gateway_sync::{build_cloud_gateway_section, build_remote_config_write};
use super::terminal::{
    build_ssh_connect_command, clean_terminal_app_name, escape_applescript_string,
    normalize_terminal_app_key,
};
use super::types::CloudAgentSettings;
use crate::ai_gateway::{
    GatewayConfig, GatewayUpstreamProvider, ModelMapping, UpstreamKey,
};

#[test]
fn test_cloud_agent_settings_defaults() {
    let s = CloudAgentSettings::default();
    assert_eq!(s.default_host, "");
    assert_eq!(s.remote_workspace_root, "/root/cloud-agent-workspaces");
    assert_eq!(s.remote_agent_dir, "/root/onespace-cloud-agent");
    assert!(s.opencode_yolo_mode);
    assert_eq!(s.opencode_extra_args, "");
    assert_eq!(s.auto_refresh_interval_seconds, 10);
    assert_eq!(s.disk_alert_threshold_percent, 80.0);
    assert_eq!(s.terminal_app, "auto");
}

#[test]
fn test_resolve_host() {
    let empty_settings = CloudAgentSettings::default();
    assert!(resolve_host(None, &empty_settings).is_err());
    assert!(resolve_host(Some("  ".to_string()), &empty_settings).is_err());

    let mut configured_settings = CloudAgentSettings::default();
    configured_settings.default_host = "server-a".to_string();
    assert_eq!(
        resolve_host(None, &configured_settings).unwrap(),
        "server-a"
    );
    assert_eq!(
        resolve_host(Some("server-b".to_string()), &configured_settings).unwrap(),
        "server-b"
    );
}

#[test]
fn test_terminal_name_and_applescript_escaping() {
    assert_eq!(clean_terminal_app_name("iTerm.app"), "iTerm");
    assert_eq!(clean_terminal_app_name("Ghostty"), "Ghostty");
    assert_eq!(normalize_terminal_app_key("  iTerm2.app "), "iterm2");
    assert_eq!(
        escape_applescript_string("hello \"world\" \\ test"),
        "hello \\\"world\\\" \\\\ test"
    );
}

#[test]
fn test_build_ssh_connect_command() {
    // 默认 yolo
    let cmd1 = build_ssh_connect_command("dmit-server", "dev_repo_feat", None, true, "");
    assert_eq!(cmd1, "ssh -t dmit-server \"cloud-session dev_repo_feat\"");

    // 禁用 yolo
    let cmd2 = build_ssh_connect_command("dmit-server", "one-space", Some("feat-auth"), false, "");
    assert_eq!(
        cmd2,
        "ssh -t dmit-server \"cloud-session one-space feat-auth --no-yolo\""
    );

    // 附带 extra_args
    let cmd3 = build_ssh_connect_command(
        "my-host",
        "dev_app",
        None,
        true,
        "--model deepseek-chat",
    );
    assert_eq!(
        cmd3,
        "ssh -t my-host \"cloud-session dev_app --extra-args '--model deepseek-chat'\""
    );
}

#[test]
fn test_build_cloud_gateway_section() {
    let mut config = GatewayConfig::default();
    let provider = GatewayUpstreamProvider {
        id: "deepseek".to_string(),
        name: "DeepSeek".to_string(),
        base_url: "https://api.deepseek.com/v1".to_string(),
        keys: vec![
            UpstreamKey {
                id: "k1".to_string(),
                name: "default".to_string(),
                value: "sk-real-secret".to_string(),
                enabled: true,
                daily_reset: false,
                auto_marked: false,
                failure_kind: None,
                marked_at: None,
                reason: None,
            },
            UpstreamKey {
                id: "k2".to_string(),
                name: "disabled".to_string(),
                value: "sk-disabled".to_string(),
                enabled: false,
                daily_reset: false,
                auto_marked: false,
                failure_kind: None,
                marked_at: None,
                reason: None,
            },
        ],
        enabled: true,
        mappings: vec![
            ModelMapping {
                local_model: "deepseek-chat".to_string(),
                upstream_model: "deepseek-chat".to_string(),
                display_name: Some("DeepSeek V3".to_string()),
                context_window: Some(64000),
                reasoning_efforts: vec!["low".to_string(), "medium".to_string(), "high".to_string()],
                enabled: true,
                auto_disabled: false,
                ..Default::default()
            },
            ModelMapping {
                local_model: "disabled-model".to_string(),
                upstream_model: "disabled-model".to_string(),
                enabled: false,
                auto_disabled: false,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    config.providers.push(provider);

    let (val, p_count, pool_count) = build_cloud_gateway_section(&config);
    assert_eq!(p_count, 1);
    assert_eq!(pool_count, 0);

    let providers = val.get("providers").unwrap().as_object().unwrap();
    assert!(providers.contains_key("deepseek"));
    let ds = providers.get("deepseek").unwrap();
    assert_eq!(ds.get("name").unwrap().as_str().unwrap(), "DeepSeek");
    let keys = ds.get("api_keys").unwrap().as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].as_str().unwrap(), "sk-real-secret");

    let models = ds.get("models").unwrap().as_object().unwrap();
    assert!(models.contains_key("deepseek-chat"));
    assert!(!models.contains_key("disabled-model"));

    let chat_model = models.get("deepseek-chat").unwrap();
    assert_eq!(chat_model.get("upstream_model").unwrap().as_str().unwrap(), "deepseek-chat");
    assert_eq!(chat_model.get("display_name").unwrap().as_str().unwrap(), "DeepSeek V3");
    assert_eq!(chat_model.get("context_window").unwrap().as_u64().unwrap(), 64000);
    let efforts = chat_model.get("reasoning_efforts").unwrap().as_array().unwrap();
    assert_eq!(efforts.len(), 3);
    assert_eq!(efforts[0].as_str().unwrap(), "low");
    assert_eq!(efforts[1].as_str().unwrap(), "medium");
    assert_eq!(efforts[2].as_str().unwrap(), "high");
}

#[test]
fn test_remote_config_write_keeps_large_payload_out_of_ssh_arguments() {
    let config_path = "/root/onespace-cloud-agent/config.json";
    let huge_key = format!("sk-{}", "x".repeat(600_000));
    let config = serde_json::json!({
        "ai_gateway": {
            "providers": {
                "large-provider": {
                    "api_keys": [huge_key]
                }
            }
        }
    });

    let (cmd, payload) = build_remote_config_write(config_path, &config).unwrap();

    assert_eq!(
        cmd,
        "cat > /root/onespace-cloud-agent/config.json.tmp && mv /root/onespace-cloud-agent/config.json.tmp /root/onespace-cloud-agent/config.json"
    );
    assert!(payload.len() > 600_000);
    assert!(!cmd.contains(&"x".repeat(64)));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&payload).unwrap(),
        config
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_run_with_stdin_streams_large_payload_to_child() {
    let dir = std::env::temp_dir().join(format!(
        "onespace-cloud-agent-stdin-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("payload.json");
    let payload = format!("{{\"key\":\"{}\"}}", "x".repeat(600_000));

    let mut cmd = tokio::process::Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(format!("cat > '{}'", target.display()));
    let output = super::ssh::run_with_stdin(cmd, payload.as_bytes())
        .await
        .unwrap();

    assert!(output.status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), payload);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_cloud_agent_task_deserialization_backward_compatible() {
    use super::types::CloudAgentTask;

    // 1. 测试旧格式 JSON（缺少新字段）正常反序列化为 None
    let legacy_json = r#"{
        "id": "task_1",
        "repo": "minbox/onespace",
        "issue_number": 42,
        "title": "Fix something",
        "status": "completed",
        "started_at": 1700000000,
        "finished_at": 1700000060,
        "duration_seconds": 60,
        "branch": "bot/issue-42",
        "pr_url": "https://github.com/minbox/onespace/pull/42",
        "error_message": null
    }"#;

    let task: CloudAgentTask = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(task.id, "task_1");
    assert_eq!(task.author, None);
    assert_eq!(task.trigger_type, None);
    assert_eq!(task.attempts, None);
    assert_eq!(task.max_attempts, None);
    assert_eq!(task.diff_stat, None);
    assert_eq!(task.step, None);

    // 2. 测试带全量扩展字段的 JSON 正常反序列化
    let rich_json = r#"{
        "id": "task_2",
        "repo": "minbox/onespace",
        "issue_number": 43,
        "title": "Add feature",
        "status": "running",
        "started_at": 1700000100,
        "branch": "bot/issue-43",
        "author": "octocat",
        "trigger_type": "follow_up",
        "attempts": 2,
        "max_attempts": 3,
        "diff_stat": "3 files changed, +45 -12",
        "step": "opencode"
    }"#;

    let task2: CloudAgentTask = serde_json::from_str(rich_json).unwrap();
    assert_eq!(task2.author.as_deref(), Some("octocat"));
    assert_eq!(task2.trigger_type.as_deref(), Some("follow_up"));
    assert_eq!(task2.attempts, Some(2));
    assert_eq!(task2.max_attempts, Some(3));
    assert_eq!(task2.diff_stat.as_deref(), Some("3 files changed, +45 -12"));
    assert_eq!(task2.step.as_deref(), Some("opencode"));
}

// --- Step 1 cloud agent desktop slice behavior tests ---

use super::local_plans;
use super::plan_transfer;
use super::ssh;
use super::types::{
    CloudAgentTask, CloudPlanRepository, CloudPlanSubmitResult, LocalPlanContext, LocalPlanPayload,
    LocalPlanSummary, PlanFilePayload,
};
use crate::config::test_home::TestHomeGuard;
use base64::Engine;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

fn git(dir: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git");
    assert!(status.success(), "git {:?} failed", args);
}

fn init_repo(root: &Path) {
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.com"]);
    git(&root, &["config", "user.name", "t"]);
    fs::write(root.join("README.md"), "x").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init"]);
}

fn unique_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ca_test_{}", uuid::Uuid::new_v4()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_state(home: &Path, root: &Path) {
    let cfg = home.join(".config/onespace/local_data/data/workspaces");
    fs::create_dir_all(&cfg).unwrap();
    let state = serde_json::json!({
        "workspaces": [{"id":"ws1","name":"Workspace One","root_path": root.to_string_lossy()}]
    });
    fs::write(cfg.join("state.json"), state.to_string()).unwrap();
}

#[test]
fn step1_valid_invalid_enumeration() {
    let home = unique_root();
    let _guard = TestHomeGuard::set(home.clone());
    let root = unique_root();
    init_repo(&root);
    write_state(&home, &root);

    let plans = root.join(".ai-workflow/plans");
    fs::create_dir_all(&plans).unwrap();

    // (a) valid frozen plan
    let valid = plans.join("plan-a");
    fs::create_dir_all(valid.join("tasks")).unwrap();
    fs::write(valid.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(valid.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(valid.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - workspace\n---\n").unwrap();
    fs::write(valid.join("plan.zh.md"), "---\n---\n").unwrap();

    // (b) in-progress record
    let inprog = plans.join("plan-b");
    fs::create_dir_all(inprog.join("tasks")).unwrap();
    fs::write(inprog.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(inprog.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(inprog.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - workspace\n---\n").unwrap();
    fs::write(inprog.join("plan.zh.md"), "---\n---\n").unwrap();
    fs::write(inprog.join("implementation.yaml"), "status: in-progress\n").unwrap();

    // (c) malformed
    let bad = plans.join("plan-c");
    fs::create_dir_all(&bad).unwrap();
    fs::write(bad.join("spec.md"), "not frontmatter").unwrap();

    // (d) plain file
    fs::write(plans.join("not-a-dir.txt"), "x").unwrap();

    let ctx = local_plans::list_local_plans("ws1").unwrap();
    assert_eq!(ctx.workspace_name, "Workspace One");
    assert_eq!(ctx.root_path, root.to_string_lossy());
    let ids: Vec<String> = ctx.plans.iter().map(|p| p.plan_id.clone()).collect();
    assert_eq!(ids, vec!["plan-c", "plan-b", "plan-a"]);
    for p in &ctx.plans {
        assert!(["valid", "invalid", "started"].contains(&p.status.as_str()));
    }
    // valid plan exact
    let vp = ctx.plans.iter().find(|p| p.plan_id == "plan-a").unwrap();
    assert_eq!(vp.status, "valid");
    assert!(vp.valid);
    assert!(vp.error.is_none());
    assert!(vp.has_tasks);
    // in-progress
    let ip = ctx.plans.iter().find(|p| p.plan_id == "plan-b").unwrap();
    assert_eq!(ip.status, "started");
    assert!(!ip.valid);
    assert!(ip.error.as_deref().unwrap_or("").to_lowercase().contains("implementation already started")
        || ip.error.as_deref().unwrap_or("").to_lowercase().contains("started"));
    // malformed
    let mp = ctx.plans.iter().find(|p| p.plan_id == "plan-c").unwrap();
    assert_eq!(mp.status, "invalid");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn step1_repository_base_resolution() {
    // (a) current branch feature tracking origin/feature
    let root = unique_root();
    init_repo(&root);
    // rename default branch? init -b main; create feature branch
    git(&root, &["checkout", "-q", "-b", "feature"]);
    let bare = unique_root();
    fs::remove_dir_all(&bare).ok();
    fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "--bare", "-q"]);
    git(&root, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&root, &["push", "-q", "origin", "HEAD:feature"]);
    git(&root, &["branch", "--set-upstream-to=origin/feature", "feature"]);
    fs::write(root.join("README.md"), "dirty").unwrap();
    let head_before = String::from_utf8(StdCommand::new("git").args(["rev-parse", "HEAD"]).current_dir(&root).output().unwrap().stdout).unwrap();
    let info = local_plans::read_repository_info(root.to_str().unwrap(), None).unwrap();
    assert_eq!(info.source, "upstream");
    assert_eq!(info.base_branch, "feature");
    assert_eq!(info.branch, "feature");
    assert!(info.dirty);
    let head_after = String::from_utf8(StdCommand::new("git").args(["rev-parse", "HEAD"]).current_dir(&root).output().unwrap().stdout).unwrap();
    assert_eq!(head_before, head_after);

    // (b) no upstream but origin/HEAD -> origin/main while on main
    let root2 = unique_root();
    init_repo(&root2);
    let bare2 = unique_root();
    fs::remove_dir_all(&bare2).ok();
    fs::create_dir_all(&bare2).unwrap();
    git(&bare2, &["init", "--bare", "-q"]);
    git(&root2, &["remote", "add", "origin", bare2.to_str().unwrap()]);
    git(&root2, &["push", "-q", "origin", "HEAD:main"]);
    git(&bare2, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    // set origin/HEAD
    git(&root2, &["remote", "set-head", "origin", "main"]);
    let info2 = local_plans::read_repository_info(root2.to_str().unwrap(), None).unwrap();
    assert_eq!(info2.source, "origin_default");
    assert_eq!(info2.base_branch, "main");

    // (c) detached HEAD no origin default -> Err
    let root3 = unique_root();
    init_repo(&root3);
    git(&root3, &["checkout", "-q", "--detach"]);
    let res = local_plans::read_repository_info(root3.to_str().unwrap(), None);
    assert!(res.is_err());
    let msg = res.unwrap_err();
    assert!(!msg.is_empty());

    // explicit Some("release")
    let root4 = unique_root();
    init_repo(&root4);
    let info4 = local_plans::read_repository_info(root4.to_str().unwrap(), Some("release")).unwrap();
    assert_eq!(info4.source, "explicit");
    assert_eq!(info4.base_branch, "release");

    for d in [root, bare, root2, bare2, root3, root4] { let _ = fs::remove_dir_all(d); }
}

#[test]
fn step1_payload_bytes() {
    let root = unique_root();
    let plan = root.join("plan");
    fs::create_dir_all(plan.join("tasks")).unwrap();
    fs::write(plan.join("tasks/x.txt"), b"hello").unwrap();
    let unicode = plan.join("ref file \u{00e9}.bin");
    fs::write(&unicode, (0u8..=255u8).collect::<Vec<_>>()).unwrap();
    // nested
    fs::create_dir_all(plan.join("tasks/sub")).unwrap();
    fs::write(plan.join("tasks/sub/y.txt"), b"nested").unwrap();
    // symlink to file inside plan accepted
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(plan.join("tasks/x.txt"), plan.join("link.txt"));
    }

    let payload = local_plans::collect_plan_payload(root.to_str().unwrap(), "plan").unwrap();
    // paths slash separated
    for f in &payload.files {
        assert!(!f.path.contains('\\'));
    }
    // byte equality for binary
    let bin_entry = payload.files.iter().find(|f| f.path.contains("ref file")).unwrap();
    let decoded = base64::engine::general_purpose::STANDARD.decode(&bin_entry.content_base64).unwrap();
    assert_eq!(decoded, (0u8..=255u8).collect::<Vec<_>>());
    // symlink accepted with target bytes
    #[cfg(unix)]
    {
        let link_entry = payload.files.iter().find(|f| f.path == "link.txt").unwrap();
        let dec = base64::engine::general_purpose::STANDARD.decode(&link_entry.content_base64).unwrap();
        assert_eq!(dec, b"hello");
    }

    // escaping absolute rejected (distinct plan dir)
    let plan2 = root.join("plan2");
    fs::create_dir_all(&plan2).unwrap();
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink("/etc/passwd", plan2.join("escape"));
        let res = local_plans::collect_plan_payload(root.to_str().unwrap(), "plan2");
        assert!(res.is_err());
    }
    // dangling symlink rejected (distinct plan dir)
    let plan3 = root.join("plan3");
    fs::create_dir_all(&plan3).unwrap();
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(plan3.join("nope"), plan3.join("dangling"));
        let res = local_plans::collect_plan_payload(root.to_str().unwrap(), "plan3");
        assert!(res.is_err());
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn step1_validation_refusals() {
    let home = unique_root();
    let _guard = TestHomeGuard::set(home.clone());
    let root = unique_root();
    init_repo(&root);
    write_state(&home, &root);
    let plans = root.join(".ai-workflow/plans");
    fs::create_dir_all(&plans).unwrap();

    // valid frozen plan fixture
    let valid = plans.join("validplan");
    fs::create_dir_all(valid.join("tasks")).unwrap();
    fs::write(valid.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(valid.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(valid.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - workspace\n---\n").unwrap();
    fs::write(valid.join("plan.zh.md"), "---\n---\n").unwrap();
    fs::write(valid.join("execution-order.yaml"), "order: []\n").unwrap();

    // (a) workspace_repos naming non-root child repo => invalid
    let a = plans.join("casea");
    fs::create_dir_all(a.join("tasks")).unwrap();
    fs::write(a.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(a.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(a.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - childrepo\n---\n").unwrap();
    fs::write(a.join("plan.zh.md"), "---\n---\n").unwrap();

    // (b) tasks dir with files but no execution-order.yaml => invalid
    let b = plans.join("caseb");
    fs::create_dir_all(b.join("tasks")).unwrap();
    fs::write(b.join("tasks/t.md"), "x").unwrap();
    fs::write(b.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(b.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(b.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - workspace\n---\n").unwrap();
    fs::write(b.join("plan.zh.md"), "---\n---\n").unwrap();

    // (c) implementation.yaml status completed => started
    let c = plans.join("casec");
    fs::create_dir_all(c.join("tasks")).unwrap();
    fs::write(c.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(c.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(c.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - workspace\n---\n").unwrap();
    fs::write(c.join("plan.zh.md"), "---\n---\n").unwrap();
    fs::write(c.join("implementation.yaml"), "status: completed\n").unwrap();

    // (d) malformed implementation.yaml => invalid, never fresh
    let d = plans.join("cased");
    fs::create_dir_all(d.join("tasks")).unwrap();
    fs::write(d.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(d.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(d.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - workspace\n---\n").unwrap();
    fs::write(d.join("plan.zh.md"), "---\n---\n").unwrap();
    fs::write(d.join("implementation.yaml"), "status: [\n").unwrap();

    // (e) absent workspace_repos and root-only reserved workspace entry both valid
    let e1 = plans.join("casee1");
    fs::create_dir_all(e1.join("tasks")).unwrap();
    fs::write(e1.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(e1.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(e1.join("plan.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(e1.join("plan.zh.md"), "---\n---\n").unwrap();
    let e2 = plans.join("casee2");
    fs::create_dir_all(e2.join("tasks")).unwrap();
    fs::write(e2.join("spec.md"), "---\nstatus: frozen\n---\n").unwrap();
    fs::write(e2.join("spec.zh.md"), "---\n---\n").unwrap();
    fs::write(e2.join("plan.md"), "---\nstatus: frozen\nworkspace_repos:\n  - workspace\n---\n").unwrap();
    fs::write(e2.join("plan.zh.md"), "---\n---\n").unwrap();

    let ctx = local_plans::list_local_plans("ws1").unwrap();
    let get = |id: &str| ctx.plans.iter().find(|p| p.plan_id == id).unwrap();
    // casea invalid
    assert_eq!(get("casea").status, "invalid");
    // caseb invalid
    assert_eq!(get("caseb").status, "invalid");
    // casec started
    assert_eq!(get("casec").status, "started");
    // cased invalid
    assert_eq!(get("cased").status, "invalid");
    // e1/e2 valid
    assert_eq!(get("casee1").status, "valid");
    assert_eq!(get("casee2").status, "valid");
    // validplan valid
    assert_eq!(get("validplan").status, "valid");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn step1_submit_envelope() {
    let repo = CloudPlanRepository {
        remote_url: "u".into(),
        canonical: "c".into(),
        host: "h".into(),
        branch: "b".into(),
        base_branch: "main".into(),
        source: "upstream".into(),
        head_sha: "sha".into(),
        dirty: false,
    };
    let payload = LocalPlanPayload {
        plan_id: "p".into(),
        plan_title: "t".into(),
        directory: "d".into(),
        files: vec![PlanFilePayload {
            path: "a.txt".into(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(b"hi"),
        }],
    };
    let v = plan_transfer::build_plan_submit_request("ws", "wname", "/root", Some(repo), payload);
    let s = v.to_string();
    assert!(s.contains("/root"));
    assert!(s.contains("ws"));
    assert!(s.contains("wname"));
    assert!(s.contains("main"));
    assert!(s.contains("p"));
    assert!(s.contains("a.txt"));
    // base64 exact
    assert!(s.contains(&base64::engine::general_purpose::STANDARD.encode(b"hi")));

    // parse error response
    let raw = r#"{"ok":false,"error":"boom"}"#;
    let res = plan_transfer::parse_plan_submit_response(raw);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "boom");

    // map_plan_command_error
    let m1 = plan_transfer::map_plan_command_error(true, "", "");
    assert!(m1.is_empty());
    let m2 = plan_transfer::map_plan_command_error(false, "auth failed", "");
    assert!(m2.to_lowercase().contains("auth") || !m2.is_empty());

    // ok response still parses
    let raw2 = r#"{"ok":true,"reused":true,"request_id":"r1","task":{"id":"t1","repo":"r","issue_number":null,"title":"t","status":"queued","started_at":1,"finished_at":null,"duration_seconds":null,"branch":null,"pr_url":null,"error_message":null}}"#;
    let res2: CloudPlanSubmitResult = plan_transfer::parse_plan_submit_response(raw2).unwrap();
    assert!(res2.reused);
    assert_eq!(res2.request_id, "r1");
}

#[test]
fn step1_attach_command() {
    let cmd = plan_transfer::build_plan_attach_command(
        "https://127.0.0.1:4096",
        "ses_x",
        "/root/job/workspace",
    )
    .unwrap();
    assert_eq!(cmd, "opencode attach https://127.0.0.1:4096 --session ses_x --dir /root/job/workspace");
    let cmd2 = plan_transfer::build_plan_attach_command("https://x?a=1&b=2", "ses;rm", "/root").unwrap();
    // exact fully-quoted expected
    assert_eq!(cmd2, "opencode attach 'https://x?a=1&b=2' --session 'ses;rm' --dir '/root'");
    assert!(plan_transfer::build_plan_attach_command("", "s", "r").is_err());
    assert!(plan_transfer::build_plan_attach_command("u", "", "r").is_err());
    assert!(plan_transfer::build_plan_attach_command("u", "s", "").is_err());
    // distinct messages
    let e1 = plan_transfer::build_plan_attach_command("", "s", "r").unwrap_err();
    let e2 = plan_transfer::build_plan_attach_command("u", "", "r").unwrap_err();
    let e3 = plan_transfer::build_plan_attach_command("u", "s", "").unwrap_err();
    assert_ne!(e1, e2);
    assert_ne!(e2, e3);
    assert_ne!(e1, e3);
}

#[tokio::test]
async fn step1_timeout() {
    let mut cmd = tokio::process::Command::new("/bin/sh");
    cmd.arg("-c").arg("sleep 5");
    let start = Instant::now();
    let res = ssh::run_with_stdin_timeout(cmd, b"", Duration::from_millis(100)).await;
    let elapsed = start.elapsed();
    assert!(res.is_err());
    assert!(elapsed < Duration::from_secs(4));
}

#[test]
fn step1_dto_compatibility() {
    let legacy = r#"{"id":"1","repo":"r","issue_number":42,"title":"t","status":"done","started_at":1,"finished_at":null,"duration_seconds":null,"branch":null,"pr_url":null,"error_message":null}"#;
    let t: CloudAgentTask = serde_json::from_str(legacy).unwrap();
    assert_eq!(t.issue_number, Some(42));
    assert!(t.source.is_none());
    assert!(t.stage.is_none());

    let plan = r#"{"id":"2","repo":"r","title":"t","status":"queued","started_at":1,"finished_at":null,"duration_seconds":null,"branch":null,"pr_url":null,"error_message":null,"source":"plan","stage":"running","session_id":"ses","plan_id":"p"}"#;
    let t2: CloudAgentTask = serde_json::from_str(plan).unwrap();
    assert_eq!(t2.issue_number, None);
    assert_eq!(t2.source.as_deref(), Some("plan"));
}

#[path = "deployment_tests.rs"]
mod deployment_tests;
