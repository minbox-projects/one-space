use std::fs;
use std::path::{Component, Path};
use std::process::Command;

use base64::Engine;
use serde::Deserialize;
use serde_yaml::Value;

use crate::cloud_agent::types::{
    CloudPlanRepository, LocalPlanContext, LocalPlanPayload, LocalPlanSummary, PlanFilePayload,
};

const REQUIRED_PLAN_FILES: [&str; 4] = ["spec.md", "spec.zh.md", "plan.md", "plan.zh.md"];
const RESERVED_ROOT_REPOSITORY: &str = "workspace";

#[derive(Debug, Deserialize)]
struct WorkspaceState {
    #[serde(default)]
    workspaces: Vec<WorkspaceRecord>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceRecord {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    root_path: String,
}

/// Resolve a saved workspace ID to its display name and local root path.
///
/// Reads the existing workspace state file without changing its schema. A
/// missing state file, unreadable state or unknown workspace produces an
/// actionable error instead of a silent empty result.
pub fn read_workspace_root(workspace_id: &str) -> Result<(String, String), String> {
    let workspace_id = workspace_id.trim();
    if workspace_id.is_empty() {
        return Err("工作区 ID 不能为空，请先选择一个工作区".to_string());
    }

    let data_dir = crate::get_data_dir()?;
    let state_path = data_dir
        .join("data")
        .join("workspaces")
        .join("state.json");
    if !state_path.is_file() {
        return Err(format!(
            "未找到工作区状态文件: {}，请先在应用内保存工作区",
            state_path.display()
        ));
    }

    let content = fs::read_to_string(&state_path)
        .map_err(|e| format!("读取工作区状态失败 ({}): {}", state_path.display(), e))?;
    let state: WorkspaceState = serde_json::from_str(&content)
        .map_err(|e| format!("解析工作区状态失败 ({}): {}", state_path.display(), e))?;

    let workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or_else(|| format!("未找到工作区: {}", workspace_id))?;

    let root_path = workspace.root_path.trim().to_string();
    if root_path.is_empty() {
        return Err(format!("工作区 {} 未配置有效的本地根目录", workspace_id));
    }

    Ok((workspace.name.trim().to_string(), root_path))
}

/// List immediate plan directories for a saved workspace and validate each one.
///
/// Repository metadata failures are collected in `repository_error` and a
/// missing plans directory is surfaced in `plans_error`; neither prevents the
/// remaining context from being returned.
pub fn list_local_plans(workspace_id: &str) -> Result<LocalPlanContext, String> {
    let (workspace_name, root_path) = read_workspace_root(workspace_id)?;

    let (repository, repository_error) =
        match read_repository_info(&root_path, None) {
            Ok(repository) => (Some(repository), None),
            Err(error) => (None, Some(error)),
        };

    let plans_root = Path::new(&root_path).join(".ai-workflow").join("plans");
    let mut plans = Vec::new();
    let mut plans_error = None;

    if !plans_root.is_dir() {
        plans_error = Some(format!(
            "未找到本地计划目录: {}，请先在该工作区创建 .ai-workflow/plans",
            plans_root.display()
        ));
    } else {
        let mut plan_ids = Vec::new();
        let entries = fs::read_dir(&plans_root)
            .map_err(|e| format!("读取计划目录失败 ({}): {}", plans_root.display(), e))?;
        for entry in entries {
            let entry = entry
                .map_err(|e| format!("读取计划目录项失败 ({}): {}", plans_root.display(), e))?;
            let file_type = entry
                .file_type()
                .map_err(|e| format!("读取计划目录项类型失败 ({}): {}", entry.path().display(), e))?;
            if file_type.is_dir() {
                if let Ok(name) = entry.file_name().into_string() {
                    plan_ids.push(name);
                }
            }
        }
        plan_ids.sort_by(|left, right| right.cmp(left));
        for plan_id in plan_ids {
            plans.push(validate_plan_directory(&plans_root, &plan_id));
        }
    }

    Ok(LocalPlanContext {
        workspace_id: workspace_id.trim().to_string(),
        workspace_name,
        root_path,
        repository,
        repository_error,
        plans,
        plans_error,
    })
}

/// Validate one immediate plan directory and return its summary.
///
/// The directory name is the plan ID. It must be a direct child of the
/// workspace's plans directory, so separators and traversal are rejected.
pub fn validate_selected_plan(
    root_path: &str,
    plan_directory: &str,
) -> Result<LocalPlanSummary, String> {
    let plan_id = plan_directory.trim();
    if !is_immediate_plan_id(plan_id) {
        return Err(format!("无效的计划目录名: {}", plan_directory));
    }
    let plans_root = Path::new(root_path).join(".ai-workflow").join("plans");
    if !plans_root.is_dir() {
        return Err(format!(
            "未找到本地计划目录: {}，请先在该工作区创建 .ai-workflow/plans",
            plans_root.display()
        ));
    }
    let summary = validate_plan_directory(&plans_root, plan_id);
    if !summary.valid {
        return Err(summary
            .error
            .clone()
            .unwrap_or_else(|| format!("计划 {} 未通过本地校验", plan_id)));
    }
    Ok(summary)
}

fn is_immediate_plan_id(plan_id: &str) -> bool {
    !plan_id.is_empty()
        && plan_id != "."
        && plan_id != ".."
        && !plan_id.contains('/')
        && !plan_id.contains('\\')
}

fn validate_plan_directory(plans_root: &Path, plan_id: &str) -> LocalPlanSummary {
    let directory = plans_root.join(plan_id);
    let has_tasks = directory.join("tasks").is_dir();

    let invalid = |message: String| LocalPlanSummary {
        plan_id: plan_id.to_string(),
        title: plan_id.to_string(),
        directory: plan_id.to_string(),
        valid: false,
        error: Some(message),
        has_tasks,
        status: "invalid".to_string(),
    };

    for required in REQUIRED_PLAN_FILES {
        if !directory.join(required).is_file() {
            return invalid(format!("缺少必要的计划文件: {}", required));
        }
    }

    let spec_text = match fs::read_to_string(directory.join("spec.md")) {
        Ok(text) => text,
        Err(e) => return invalid(format!("读取 spec.md 失败: {}", e)),
    };
    let plan_text = match fs::read_to_string(directory.join("plan.md")) {
        Ok(text) => text,
        Err(e) => return invalid(format!("读取 plan.md 失败: {}", e)),
    };

    let spec_frontmatter = match extract_frontmatter(&spec_text, "spec.md") {
        Ok(value) => value,
        Err(error) => return invalid(error),
    };
    if spec_frontmatter.get("status").and_then(Value::as_str) != Some("frozen") {
        return invalid("spec.md 的 frontmatter 必须声明 status: frozen".to_string());
    }

    let plan_frontmatter = match extract_frontmatter(&plan_text, "plan.md") {
        Ok(value) => value,
        Err(error) => return invalid(error),
    };
    if plan_frontmatter.get("status").and_then(Value::as_str) != Some("frozen") {
        return invalid("plan.md 的 frontmatter 必须声明 status: frozen".to_string());
    }

    if let Some(workspace_repos) = plan_frontmatter.get("workspace_repos") {
        match workspace_repos {
            Value::Sequence(entries) => {
                for entry in entries {
                    match entry {
                        Value::String(name) if name == RESERVED_ROOT_REPOSITORY => {}
                        Value::String(name) => {
                            return invalid(format!(
                                "workspace_repos 只能声明根仓库保留项 workspace，不能包含子仓库: {}",
                                name
                            ))
                        }
                        _ => {
                            return invalid(
                                "workspace_repos 只能声明字符串保留项 workspace，不能包含映射或子仓库条目"
                                    .to_string(),
                            )
                        }
                    }
                }
            }
            _ => {
                return invalid(
                    "workspace_repos 必须为字符串列表且只能声明根仓库保留项 workspace".to_string(),
                )
            }
        }
    }

    if has_tasks {
        let tasks_dir = directory.join("tasks");
        if contains_markdown_file(&tasks_dir) {
            let order_path = tasks_dir.join("execution-order.yaml");
            if !order_path.is_file() {
                return invalid(
                    "tasks/ 包含任务文件但缺少 tasks/execution-order.yaml".to_string(),
                );
            }
            let order_text = match fs::read_to_string(&order_path) {
                Ok(text) => text,
                Err(e) => return invalid(format!("读取 tasks/execution-order.yaml 失败: {}", e)),
            };
            if let Err(e) = serde_yaml::from_str::<Value>(&order_text) {
                return invalid(format!("tasks/execution-order.yaml 无法解析: {}", e));
            }
        }
    }

    let title = first_heading(&plan_text).unwrap_or_else(|| plan_id.to_string());

    let implementation_path = directory.join("implementation.yaml");
    if implementation_path.is_file() {
        let implementation_text = match fs::read_to_string(&implementation_path) {
            Ok(text) => text,
            Err(e) => return invalid(format!("读取 implementation.yaml 失败: {}", e)),
        };
        let implementation: Value = match serde_yaml::from_str(&implementation_text) {
            Ok(value) => value,
            Err(e) => {
                return invalid(format!(
                    "implementation.yaml 无法解析，不能视为未开始的计划: {}",
                    e
                ))
            }
        };
        match implementation.get("status").and_then(Value::as_str) {
            Some("in-progress") | Some("completed") => {
                return LocalPlanSummary {
                    plan_id: plan_id.to_string(),
                    title,
                    directory: plan_id.to_string(),
                    valid: false,
                    error: Some(
                        "implementation already started (implementation.yaml 已存在实现记录)"
                            .to_string(),
                    ),
                    has_tasks,
                    status: "started".to_string(),
                };
            }
            Some(status) => {
                return invalid(format!(
                    "implementation.yaml 的 status 不合法: {}，不能视为未开始的计划",
                    status
                ))
            }
            None => {
                return invalid(
                    "implementation.yaml 缺少有效的 status，不能视为未开始的计划".to_string(),
                )
            }
        }
    }

    LocalPlanSummary {
        plan_id: plan_id.to_string(),
        title,
        directory: plan_id.to_string(),
        valid: true,
        error: None,
        has_tasks,
        status: "valid".to_string(),
    }
}

fn contains_markdown_file(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if contains_markdown_file(&entry.path()) {
                return true;
            }
        } else if file_type.is_file() {
            if entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("md")
            {
                return true;
            }
        }
    }
    false
}

fn extract_frontmatter(text: &str, file: &str) -> Result<Value, String> {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim_end_matches('\r');
    if first.trim() != "---" {
        return Err(format!("{} 缺少有效的 YAML frontmatter", file));
    }

    let mut yaml = String::new();
    let mut closed = false;
    for line in lines {
        let line = line.trim_end_matches('\r');
        if line.trim() == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    if !closed {
        return Err(format!("{} 的 YAML frontmatter 未闭合", file));
    }
    serde_yaml::from_str(&yaml).map_err(|e| format!("{} 的 YAML frontmatter 无法解析: {}", file, e))
}

fn first_heading(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|heading| !heading.is_empty())
        .map(str::to_string)
}

/// Read local Git metadata for the workspace without mutating the repository.
///
/// An explicit non-empty branch becomes the base. Otherwise the current
/// branch's upstream is preferred, followed by the remote default branch from
/// `origin/HEAD`. A detached HEAD without a resolvable fallback is an error.
pub fn read_repository_info(
    root_path: &str,
    explicit_branch: Option<&str>,
) -> Result<CloudPlanRepository, String> {
    let root = Path::new(root_path);
    if !root.is_dir() {
        return Err(format!("工作区根目录不存在: {}", root_path));
    }

    let head_sha = run_git(root, &["rev-parse", "HEAD"])?;
    let dirty = !run_git(root, &["status", "--porcelain"])?.is_empty();
    let remote_url = run_git_optional(root, &["remote", "get-url", "origin"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let (host, canonical) = derive_remote_identity(&remote_url);

    let current_branch = run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let current_branch = current_branch.trim().to_string();
    let detached = current_branch == "HEAD" || current_branch.is_empty();

    let explicit_branch = explicit_branch.map(str::trim).filter(|branch| !branch.is_empty());
    let (base_branch, source) = if let Some(branch) = explicit_branch {
        (branch.to_string(), "explicit".to_string())
    } else if let Some(upstream) =
        run_git_optional(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"])
    {
        let upstream = upstream.trim();
        let base = upstream
            .rsplit_once('/')
            .map(|(_, branch)| branch)
            .unwrap_or(upstream)
            .to_string();
        if base.is_empty() {
            return Err(format!(
                "无法解析当前分支的上游基准分支: {}",
                root_path
            ));
        }
        (base, "upstream".to_string())
    } else if detached {
        return Err(format!(
            "当前工作区处于 detached HEAD 且无可用上游或 origin 默认分支，请检出分支后重试: {}",
            root_path
        ));
    } else if let Some(origin_head) =
        run_git_optional(root, &["symbolic-ref", "refs/remotes/origin/HEAD"])
    {
        let origin_head = origin_head.trim();
        let base = origin_head
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if base.is_empty() {
            return Err(format!(
                "无法解析 origin 默认分支，请先配置远程仓库: {}",
                root_path
            ));
        }
        (base, "origin_default".to_string())
    } else {
        return Err(format!(
            "无法解析上游或 origin 默认分支，请为当前分支设置 upstream 或先配置 origin: {}",
            root_path
        ));
    };

    Ok(CloudPlanRepository {
        remote_url,
        canonical,
        host,
        branch: current_branch,
        base_branch,
        source,
        head_sha,
        dirty,
    })
}

fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("执行本地 Git 命令失败 (git {}): {}", args.join(" "), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("本地 Git 命令失败 (git {}): 退出码 {:?}", args.join(" "), output.status.code())
        } else {
            format!("本地 Git 命令失败 (git {}): {}", args.join(" "), stderr)
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git_optional(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn derive_remote_identity(remote_url: &str) -> (String, String) {
    let remote_url = remote_url.trim();
    if remote_url.is_empty() {
        return (String::new(), String::new());
    }

    let (host, path) = if let Some(rest) = remote_url.strip_prefix("git@") {
        match rest.split_once(':') {
            Some((host, path)) => (host.to_string(), path.to_string()),
            None => (String::new(), String::new()),
        }
    } else if let Ok(url) = url::Url::parse(remote_url) {
        (
            url.host_str().unwrap_or("").to_string(),
            url.path().trim_start_matches('/').to_string(),
        )
    } else if let Some((host, path)) = remote_url.split_once(':') {
        (host.to_string(), path.to_string())
    } else {
        (String::new(), remote_url.to_string())
    };

    let canonical = path
        .trim()
        .trim_start_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_string();

    (host.trim().to_lowercase(), canonical)
}

/// Collect every regular file of one selected plan directory.
///
/// The plan directory must be an immediate child of `<root>/.ai-workflow/plans`
/// and may not escape through a symlinked directory. Links are followed only
/// when their target resolves inside the selected plan; absolute targets
/// outside the plan, dangling targets and escaping targets are rejected. Bytes
/// are preserved exactly and results are sorted by slash-separated path.
pub fn collect_plan_payload(
    root_path: &str,
    plan_directory: &str,
) -> Result<LocalPlanPayload, String> {
    let plan_id = plan_directory.trim();
    if !is_immediate_plan_id(plan_id) {
        return Err(format!("无效的计划目录名: {}", plan_directory));
    }

    let workspace_root = Path::new(root_path);
    if !workspace_root.is_dir() {
        return Err(format!("未找到工作区根目录: {}", workspace_root.display()));
    }
    let plans_root = workspace_root.join(".ai-workflow").join("plans");
    let (base_root, plan_dir) = if plans_root.join(plan_id).is_dir() {
        (plans_root.clone(), plans_root.join(plan_id))
    } else if workspace_root.join(plan_id).is_dir() {
        (workspace_root.to_path_buf(), workspace_root.join(plan_id))
    } else {
        return Err(format!(
            "未找到计划目录: {}（已检查 {} 与 {}）",
            plan_id,
            plans_root.join(plan_id).display(),
            workspace_root.join(plan_id).display()
        ));
    };

    let canonical_base_root = fs::canonicalize(&base_root)
        .map_err(|e| format!("解析计划根目录失败 ({}): {}", base_root.display(), e))?;
    let canonical_plan_dir = fs::canonicalize(&plan_dir)
        .map_err(|e| format!("解析计划目录失败 ({}): {}", plan_dir.display(), e))?;
    if canonical_plan_dir == canonical_base_root
        || canonical_plan_dir.parent() != Some(canonical_base_root.as_path())
    {
        return Err(format!("计划目录越界: {}", plan_directory));
    }

    let plan_title = fs::read_to_string(plan_dir.join("plan.md"))
        .ok()
        .and_then(|text| first_heading(&text))
        .unwrap_or_else(|| plan_id.to_string());

    let mut files = Vec::new();
    collect_files(&plan_dir, &plan_dir, &canonical_plan_dir, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(LocalPlanPayload {
        plan_id: plan_id.to_string(),
        plan_title,
        directory: plan_id.to_string(),
        files,
    })
}

fn collect_files(
    base_dir: &Path,
    current_dir: &Path,
    canonical_plan_dir: &Path,
    files: &mut Vec<PlanFilePayload>,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir)
        .map_err(|e| format!("读取计划目录失败 ({}): {}", current_dir.display(), e))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("读取计划目录项失败 ({}): {}", current_dir.display(), e))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取计划目录项类型失败 ({}): {}", path.display(), e))?;

        if file_type.is_symlink() {
            let target = fs::read_link(&path)
                .map_err(|e| format!("读取符号链接失败 ({}): {}", path.display(), e))?;
            let resolved = if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or(current_dir).join(target)
            };
            let canonical_target = fs::canonicalize(&resolved).map_err(|e| {
                format!(
                    "符号链接目标无效或不存在 ({} -> {}): {}",
                    path.display(),
                    resolved.display(),
                    e
                )
            })?;
            if !canonical_target.starts_with(canonical_plan_dir) {
                return Err(format!(
                    "符号链接越出计划目录: {} -> {}",
                    path.display(),
                    canonical_target.display()
                ));
            }
            let metadata = fs::metadata(&canonical_target)
                .map_err(|e| format!("读取符号链接目标失败 ({}): {}", canonical_target.display(), e))?;
            if metadata.is_file() {
                push_file(base_dir, &path, &canonical_target, files)?;
            } else if metadata.is_dir() {
                return Err(format!(
                    "不支持指向目录的符号链接: {} -> {}",
                    path.display(),
                    canonical_target.display()
                ));
            }
        } else if file_type.is_dir() {
            collect_files(base_dir, &path, canonical_plan_dir, files)?;
        } else if file_type.is_file() {
            push_file(base_dir, &path, &path, files)?;
        }
    }
    Ok(())
}

fn push_file(
    base_dir: &Path,
    link_path: &Path,
    read_path: &Path,
    files: &mut Vec<PlanFilePayload>,
) -> Result<(), String> {
    let bytes = fs::read(read_path)
        .map_err(|e| format!("读取计划文件失败 ({}): {}", read_path.display(), e))?;
    let relative = link_path
        .strip_prefix(base_dir)
        .map_err(|e| format!("计算计划文件相对路径失败 ({}): {}", link_path.display(), e))?;
    let relative_path = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if relative_path.is_empty() {
        return Err(format!("无效的计划文件路径: {}", link_path.display()));
    }
    files.push(PlanFilePayload {
        path: relative_path,
        content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    });
    Ok(())
}
