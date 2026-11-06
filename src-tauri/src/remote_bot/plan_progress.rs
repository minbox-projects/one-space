use std::path::Path;
use serde::{Deserialize, Serialize};
use super::types::{InlineKeyboardButton, InlineKeyboardMarkup};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanExecutionStatus {
    Completed,
    InProgress,
    NotStarted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanProgressInfo {
    pub plan_id: String,
    pub status: PlanExecutionStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub elapsed_desc: Option<String>,
    pub commit_hash: Option<String>,
    pub has_worktree: bool,
    pub worktree_branch: Option<String>,
    pub steps: Vec<String>,
    pub report_files: Vec<(String, String)>,
}

#[derive(Debug, Clone, Deserialize)]
struct ImplementationYaml {
    #[allow(dead_code)]
    pub plan_id: Option<String>,
    pub status: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub commit: Option<String>,
}

/// 探测指定 plan 的实施进度
pub fn get_plan_progress(project_dir: &Path, raw_plan_id: &str) -> Result<PlanProgressInfo, String> {
    let clean_plan_id = raw_plan_id
        .trim_start_matches(".ai-workflow/plans/")
        .trim_start_matches("plans/")
        .trim_matches('/')
        .to_string();

    let plan_dir = project_dir.join(".ai-workflow").join("plans").join(&clean_plan_id);
    let plan_file = plan_dir.join("plan.md");

    if !plan_file.is_file() {
        return Err(format!("未找到方案 [{}]，路径不存在: {}", clean_plan_id, plan_file.display()));
    }

    // 1. 读取 plan.md 中定义的实施步骤
    let steps = extract_plan_steps(&plan_file);

    // 2. 检查 implementation.yaml（优先检查 plan 目录，其次检查对应 worktree 中的目录）
    let worktree_dir = project_dir.join(".worktrees").join(&clean_plan_id);
    let has_worktree = worktree_dir.is_dir();

    let impl_yaml_path = if plan_dir.join("implementation.yaml").is_file() {
        Some(plan_dir.join("implementation.yaml"))
    } else if has_worktree && worktree_dir.join(".ai-workflow").join("plans").join(&clean_plan_id).join("implementation.yaml").is_file() {
        Some(worktree_dir.join(".ai-workflow").join("plans").join(&clean_plan_id).join("implementation.yaml"))
    } else {
        None
    };

    let impl_data: Option<ImplementationYaml> = impl_yaml_path.and_then(|p| {
        std::fs::read_to_string(p).ok().and_then(|content| {
            serde_yaml::from_str::<ImplementationYaml>(&content).ok()
        })
    });

    // 3. 计算状态与耗时
    let mut status = PlanExecutionStatus::NotStarted;
    let mut started_at = None;
    let mut completed_at = None;
    let mut commit_hash = None;
    let mut elapsed_desc = None;

    if let Some(ref data) = impl_data {
        started_at = data.started_at.clone();
        completed_at = data.completed_at.clone();
        commit_hash = data.commit.clone();

        let raw_status = data.status.as_deref().unwrap_or("").to_lowercase();
        if raw_status.contains("completed") || raw_status.contains("done") || raw_status.contains("success") {
            status = PlanExecutionStatus::Completed;
        } else if raw_status.contains("progress") || raw_status.contains("running") {
            status = PlanExecutionStatus::InProgress;
        } else if raw_status.contains("fail") || raw_status.contains("error") {
            status = PlanExecutionStatus::Failed;
        }

        // 计算耗时
        if let Some(ref start_str) = data.started_at {
            if let Ok(start_time) = chrono::DateTime::parse_from_rfc3339(start_str) {
                let end_time = if let Some(ref comp_str) = data.completed_at {
                    chrono::DateTime::parse_from_rfc3339(comp_str).ok()
                } else {
                    None
                }.unwrap_or_else(|| chrono::Utc::now().into());

                let duration = end_time.signed_duration_since(start_time);
                let total_secs = duration.num_seconds().max(0);
                let hours = total_secs / 3600;
                let mins = (total_secs % 3600) / 60;
                let secs = total_secs % 60;

                let desc = if hours > 0 {
                    format!("{} 小时 {} 分钟", hours, mins)
                } else if mins > 0 {
                    format!("{} 分 {} 秒", mins, secs)
                } else {
                    format!("{} 秒", secs)
                };
                elapsed_desc = Some(desc);
            }
        }
    } else if has_worktree {
        status = PlanExecutionStatus::InProgress;
    }

    // 4. 提取报告文件
    let mut report_files = Vec::new();
    let reports_dir = plan_dir.join("reports");
    if let Ok(entries) = std::fs::read_dir(&reports_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                let mtime_str = p.metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .map(|t| {
                        let dt: chrono::DateTime<chrono::Local> = t.into();
                        dt.format("%H:%M:%S").to_string()
                    })
                    .unwrap_or_default();
                report_files.push((name, mtime_str));
            }
        }
    }
    // 按文件名排序
    report_files.sort_by(|a, b| a.0.cmp(&b.0));

    let worktree_branch = if has_worktree {
        Some(format!("ai-workflow/{}", clean_plan_id))
    } else {
        None
    };

    Ok(PlanProgressInfo {
        plan_id: clean_plan_id,
        status,
        started_at,
        completed_at,
        elapsed_desc,
        commit_hash,
        has_worktree,
        worktree_branch,
        steps,
        report_files,
    })
}

/// 自动探测当前正在实施中的 planId
pub fn find_active_plan_id(project_dir: &Path) -> Option<String> {
    // 1. 优先从 .worktrees 目录探测正在运行的独立临时工作区
    let worktrees_dir = project_dir.join(".worktrees");
    if let Ok(entries) = std::fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if project_dir.join(".ai-workflow").join("plans").join(&name).join("plan.md").is_file() {
                    return Some(name);
                }
            }
        }
    }

    // 2. 从 .ai-workflow/plans 中扫描 status 为 in-progress 的方案
    let plans_dir = project_dir.join(".ai-workflow").join("plans");
    if let Ok(entries) = std::fs::read_dir(&plans_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let yaml_file = p.join("implementation.yaml");
                if let Ok(content) = std::fs::read_to_string(&yaml_file) {
                    if content.contains("in-progress") || content.contains("in_progress") {
                        return Some(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    None
}

/// 解析 plan.md 中的实施步骤标题列表
fn extract_plan_steps(plan_file: &Path) -> Vec<String> {
    let mut steps = Vec::new();
    if let Ok(content) = std::fs::read_to_string(plan_file) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("### Step") || trimmed.starts_with("### 步骤") {
                let clean = trimmed.trim_start_matches('#').trim();
                steps.push(clean.to_string());
            }
        }
    }
    steps
}

/// 格式化进度为移动端友好的展示消息及操作按钮
pub fn format_plan_progress_message(info: &PlanProgressInfo) -> (String, Option<InlineKeyboardMarkup>) {
    let mut lines = Vec::new();
    lines.push("📊 【Plan 实施进度报告】".to_string());
    lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());
    lines.push(format!("📋 方案标识: `{}`", info.plan_id));

    let (status_text, can_run, can_refresh) = match info.status {
        PlanExecutionStatus::Completed => ("🟢 已圆满交付 (Completed)", false, true),
        PlanExecutionStatus::InProgress => ("🟡 正在实施中 (In-Progress)", false, true),
        PlanExecutionStatus::NotStarted => ("⚪ 尚未实施 (Not Started)", true, false),
        PlanExecutionStatus::Failed => ("🔴 实施中断/失败 (Failed)", true, true),
    };
    lines.push(format!("🏷️ 实施状态: {}", status_text));

    if let Some(ref elapsed) = info.elapsed_desc {
        if info.status == PlanExecutionStatus::Completed {
            lines.push(format!("⏱️ 交付总耗时: {}", elapsed));
        } else {
            lines.push(format!("⏳ 运行已耗时: {}", elapsed));
        }
    }

    if let Some(ref commit) = info.commit_hash {
        let short = if commit.len() > 8 { &commit[..8] } else { commit };
        lines.push(format!("🎯 交付提交: `{}`", short));
    }

    if info.has_worktree {
        lines.push("".to_string());
        lines.push("🌿 隔离构建环境 (Worktree):".to_string());
        lines.push(format!("• 路径: `.worktrees/{}`", info.plan_id));
        if let Some(ref br) = info.worktree_branch {
            lines.push(format!("• 分支: `{}`", br));
        }
    }

    if !info.steps.is_empty() {
        lines.push("".to_string());
        lines.push(format!("📝 规划实施步骤 (共 {} 步):", info.steps.len()));
        for (idx, step) in info.steps.iter().enumerate() {
            let prefix = match info.status {
                PlanExecutionStatus::Completed => "✅",
                PlanExecutionStatus::InProgress => if idx == 0 { "⏳" } else { "•" },
                _ => "•",
            };
            lines.push(format!("{} {}", prefix, step));
        }
    }

    if !info.report_files.is_empty() {
        lines.push("".to_string());
        lines.push(format!("📁 验证与测试证据 (已产出 {} 份):", info.report_files.len()));
        for (name, time) in info.report_files.iter().take(5) {
            lines.push(format!("• `{}` ({})", name, time));
        }
        if info.report_files.len() > 5 {
            lines.push(format!("• ...以及其余 {} 份日志", info.report_files.len() - 5));
        }
    }

    lines.push("━━━━━━━━━━━━━━━━━━━━".to_string());

    let mut buttons = Vec::new();
    let mut row = Vec::new();

    if can_refresh {
        row.push(InlineKeyboardButton::callback(
            format!("🔄 刷新进度"),
            format!("check_progress:{}", info.plan_id),
        ));
    }

    if can_run {
        row.push(InlineKeyboardButton::callback(
            format!("🚀 启动 Coding 实施"),
            format!("approve_plan:{}", info.plan_id),
        ));
    }

    if info.status == PlanExecutionStatus::InProgress {
        row.push(InlineKeyboardButton::callback(
            "🛑 中断任务",
            "cancel_task",
        ));
    }

    if !row.is_empty() {
        buttons.push(row);
    }

    let markup = if !buttons.is_empty() {
        Some(InlineKeyboardMarkup::new(buttons))
    } else {
        None
    };

    (lines.join("\n"), markup)
}
