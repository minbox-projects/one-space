use std::path::{Path, PathBuf};
use std::process::Stdio;
use log::{debug, warn};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use tokio::process::Command;

use super::session_inventory::is_subagent_session;
use super::types::{ExternalOpenCodeSessionInfo, OpenCodeTodoInfo};

/// 获取本机 OpenCode 默认存储与数据库路径
pub fn resolve_opencode_db_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".local").join("share").join("opencode").join("opencode.db"))
        .unwrap_or_else(|| PathBuf::from("/nonexistent/opencode.db"))
}

/// 获取本机 OpenCode 实时日志路径
pub fn resolve_opencode_log_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".local").join("share").join("opencode").join("log").join("opencode.log"))
        .unwrap_or_else(|| PathBuf::from("/nonexistent/opencode.log"))
}

/// 扫描本机所有非 OneSpace 启动的 OpenCode 会话原始记录（含 busy/idle 与暂未识别 session_id 的进程）
async fn collect_external_opencode_sessions() -> Vec<ExternalOpenCodeSessionInfo> {
    let raw_processes = match fetch_running_opencode_processes().await {
        Ok(procs) => procs,
        Err(e) => {
            warn!("获取运行中进程失败: {}", e);
            return Vec::new();
        }
    };

    let self_pid = std::process::id();
    let db_path = resolve_opencode_db_path();
    let log_path = resolve_opencode_log_path();

    let mut sessions = Vec::new();

    for proc in raw_processes {
        // 过滤排除 OneSpace 自身或其直接子进程
        if proc.pid == self_pid || proc.ppid == self_pid {
            continue;
        }

        // 解析工作目录 CWD
        let cwd = match fetch_process_cwd(proc.pid).await {
            Some(dir) => dir,
            None => continue,
        };

        let project_name = detect_meaningful_project_name(&cwd);

        let mut session_info = ExternalOpenCodeSessionInfo {
            pid: proc.pid,
            ppid: proc.ppid,
            elapsed: proc.elapsed,
            command: proc.command,
            cwd: cwd.clone(),
            project_name,
            session_id: None,
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
        };

        let subagent_ids = if db_path.is_file() {
            enrich_session_from_db(&mut session_info, &db_path, &cwd)
        } else {
            Vec::new()
        };

        if let Some(ref sid) = session_info.session_id {
            if log_path.is_file() {
                let mut all_sids = vec![sid.as_str()];
                for sub_id in &subagent_ids {
                    all_sids.push(sub_id.as_str());
                }
                session_info.is_busy = check_busy_state_from_log(&log_path, &all_sids);
            }
        }

        sessions.push(session_info);
    }

    sessions
}

/// 发现用于统一会话清单的外部终端会话：保留 busy 与 idle 的主代理会话，
/// 包括暂时无法读取 session_id 的进程。
pub async fn discover_inventory_opencode_sessions() -> Vec<ExternalOpenCodeSessionInfo> {
    let mut sessions = collect_external_opencode_sessions().await;
    sessions.retain(is_listable_terminal_session);
    sort_and_dedupe_external_sessions(&mut sessions);
    sessions
}

/// 扫描并发现所有非 OneSpace 启动的、正在运行的 OpenCode 会话。
///
/// 保留既有 `/host`、`/prompt`、`/status` 调用方语义：仅返回 busy 且已识别
/// session_id 的主代理会话。
pub async fn discover_external_opencode_sessions() -> Vec<ExternalOpenCodeSessionInfo> {
    let mut sessions = collect_external_opencode_sessions().await;
    sessions.retain(|sess| sess.is_busy);
    sessions.retain(|sess| sess.session_id.is_some() && is_listable_terminal_session(sess));
    sort_and_dedupe_external_sessions(&mut sessions);
    sessions
}

/// 判断会话记录是否为可列出的主代理终端会话。
pub fn is_listable_terminal_session(info: &ExternalOpenCodeSessionInfo) -> bool {
    !is_subagent_session(info.title.as_deref(), None)
}

fn sort_and_dedupe_external_sessions(sessions: &mut Vec<ExternalOpenCodeSessionInfo>) {
    // 按 updated_at 或 pid 倒序排列，优先展示最新活跃的会话
    sessions.sort_by(|a, b| {
        b.updated_at
            .unwrap_or_default()
            .cmp(&a.updated_at.unwrap_or_default())
            .then_with(|| b.pid.cmp(&a.pid))
    });

    // 严格按 session_id 去重：若多个进程关联同一会话，仅展示一条最新活跃会话
    let mut seen_sids = std::collections::HashSet::new();
    sessions.retain(|sess| match &sess.session_id {
        Some(sid) => seen_sids.insert(sid.clone()),
        None => true,
    });
}

/// 内部进程探测原始结构
#[derive(Debug, Clone)]
struct RawProcessInfo {
    pid: u32,
    ppid: u32,
    elapsed: String,
    command: String,
}

/// 执行 ps 命令扫描当前机器所有包含 opencode 的进程
async fn fetch_running_opencode_processes() -> Result<Vec<RawProcessInfo>, String> {
    let output = Command::new("ps")
        .args(["-eo", "pid,ppid,etime,command"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("执行 ps 异常: {}", e))?;

    if !output.status.success() {
        return Err("ps 执行失败".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }

        let pid: u32 = match parts[0].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let ppid: u32 = match parts[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let elapsed = parts[2].to_string();
        let cmd = parts[3..].join(" ");

        // 严格过滤：必须包含 opencode 且不能是 grep/node/onespace 内部封装脚本
        if is_opencode_executable_process(&cmd) {
            results.push(RawProcessInfo {
                pid,
                ppid,
                elapsed,
                command: cmd,
            });
        }
    }

    Ok(results)
}

/// 判定命令字符串是否为 OpenCode 核心执行程序（排除后台 serve 守护服务及辅助脚本）
pub fn is_opencode_executable_process(cmd: &str) -> bool {
    let lower = cmd.to_lowercase();
    if lower.contains("grep") || lower.contains("onespace-zen-relay") {
        return false;
    }

    // 排除 opencode serve / daemon 后台服务守护进程，它们是服务端而非前台/批处理任务会话
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    if tokens.contains(&"serve") || tokens.contains(&"daemon") {
        return false;
    }

    // 排除带有服务化监听端口/主机名参数的常驻服务端进程（如 --port 与 --hostname）
    if lower.contains("--port") && lower.contains("--hostname") {
        return false;
    }

    let first_token = tokens.first().copied().unwrap_or("");
    let is_bin_name = first_token == "opencode"
        || first_token.ends_with("/opencode")
        || (first_token.contains("opencode") && !first_token.ends_with(".js") && !first_token.ends_with(".ts") && !first_token.ends_with(".sh"));

    is_bin_name
}

/// 解析有意义的项目名称：若工作目录位于 Git 仓库子目录内，优先返回仓库根目录名，避免展示底层构建子目录（如 src-tauri）
pub fn detect_meaningful_project_name(cwd: &str) -> String {
    let mut current_p = Path::new(cwd);
    while let Some(parent) = current_p.parent() {
        if current_p.join(".git").exists() {
            if let Some(name) = current_p.file_name() {
                return name.to_string_lossy().to_string();
            }
        }
        if parent.join(".git").exists() {
            if let Some(name) = parent.file_name() {
                return name.to_string_lossy().to_string();
            }
        }
        current_p = parent;
    }

    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| cwd.to_string())
}

/// 通过 lsof 命令精确读取目标进程的 CWD
async fn fetch_process_cwd(pid: u32) -> Option<String> {
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix('n') {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

/// 从 SQLite 数据库只读模式中检索该工作目录匹配的最新主代理会话信息，并返回其派生的子会话 ID 列表
fn enrich_session_from_db(
    info: &mut ExternalOpenCodeSessionInfo,
    db_path: &Path,
    cwd: &str,
) -> Vec<String> {
    let mut subagent_ids: Vec<String> = Vec::new();
    let conn = match Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
    ) {
        Ok(c) => c,
        Err(e) => {
            debug!("无法以只读模式打开 opencode.db: {}", e);
            return subagent_ids;
        }
    };

    // 候选目录：优先使用进程精确 CWD；若属于某个 Git 仓库子目录，追加其仓库根目录
    let mut candidate_dirs = vec![cwd.to_string()];
    let mut current_p = Path::new(cwd);
    while let Some(parent) = current_p.parent() {
        if current_p.join(".git").exists() {
            let p_str = current_p.to_string_lossy().to_string();
            if !candidate_dirs.contains(&p_str) {
                candidate_dirs.push(p_str);
            }
            break;
        }
        if parent.join(".git").exists() {
            let p_str = parent.to_string_lossy().to_string();
            if !candidate_dirs.contains(&p_str) {
                candidate_dirs.push(p_str);
            }
            break;
        }
        current_p = parent;
    }

    // 1. 查询 session 或 session_v2 表匹配工作目录的最新会话（仅检索主代理会话，排除 subagent 会话）
    let query_session = |table: &str, dir_param: &str| -> Option<(String, Option<String>, Option<String>, Option<String>, i64, i64, i64, i64, i64)> {
        let sql = format!(
            "SELECT id, title, agent, model, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, time_updated \
             FROM {} \
             WHERE directory = ?1 \
               AND (parent_id IS NULL OR parent_id = '') \
             ORDER BY time_updated DESC \
             LIMIT 1",
            table
        );

        let mut stmt = conn.prepare(&sql).ok()?;
        let row_opt = stmt.query_row([dir_param], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                row.get::<_, Option<i64>>(8)?.unwrap_or(0),
            ))
        }).ok()?;

        // 防御性过滤：标题带有 subagent 标识的会话不是主代理会话
        if is_subagent_session(row_opt.1.as_deref(), None) {
            return None;
        }

        Some(row_opt)
    };

    let mut session_row = None;
    for cand_dir in &candidate_dirs {
        // 同一工作目录的主代理会话可能分别存在于 legacy session 与 session_v2 表中，
        // 不能固定优先某一表：历史遗留行可能比另一表中的活跃会话更新。
        // 同时查询两表并按 time_updated 选较新者，避免过期行遮蔽真正活跃的会话。
        let v2_row = query_session("session_v2", cand_dir);
        let legacy_row = query_session("session", cand_dir);
        let newest = match (v2_row, legacy_row) {
            (Some(a), Some(b)) => Some(if a.8 >= b.8 { a } else { b }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        if let Some(r) = newest {
            session_row = Some(r);
            break;
        }
    }

    if let Some((sid, title, agent, raw_model, t_in, t_out, t_reason, t_cache, updated)) = session_row {
        info.session_id = Some(sid.clone());
        info.title = title;
        info.agent = agent;
        info.model = raw_model.map(|m| parse_model_id(&m));
        info.tokens_input = t_in;
        info.tokens_output = t_out;
        info.tokens_reasoning = t_reason;
        info.tokens_cache_read = t_cache;
        info.updated_at = if updated > 0 { Some(updated) } else { None };

        // 收集该主会话名下的子代理会话 (subagent) ID，用于后续日志状态与动态关联
        let subagents_sql = "SELECT id FROM session WHERE parent_id = ?1 UNION SELECT id FROM session_v2 WHERE parent_id = ?1";
        if let Ok(mut stmt) = conn.prepare(subagents_sql) {
            if let Ok(rows) = stmt.query_map([&sid], |row| row.get::<_, String>(0)) {
                for id_res in rows.flatten() {
                    subagent_ids.push(id_res);
                }
            }
        }

        // 2. 统计主会话步骤总数 (兼顾 v1 的 step-start 与 v2 的 step_start)
        let step_count_sql = "SELECT count(*) FROM part WHERE session_id = ?1 AND (json_extract(data, '$.type') = 'step-start' OR json_extract(data, '$.type') = 'step_start')";
        if let Ok(mut stmt) = conn.prepare(step_count_sql) {
            if let Ok(cnt) = stmt.query_row([&sid], |row| row.get::<_, u32>(0)) {
                if cnt > 0 {
                    info.current_step = Some(cnt);
                }
            }
        }

        // 3. 读取最近的 parts 获取最新工具调用与思考（覆盖主会话及关联 subagent 的最新执行动作）
        let parts_sql = "SELECT data, time_created FROM part \
                         WHERE session_id = ?1 \
                            OR session_id IN (SELECT id FROM session WHERE parent_id = ?1) \
                            OR session_id IN (SELECT id FROM session_v2 WHERE parent_id = ?1) \
                         ORDER BY time_created DESC LIMIT 20";
        if let Ok(mut stmt) = conn.prepare(parts_sql) {
            if let Ok(rows) = stmt.query_map([&sid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            }) {
                let mut found_tool = false;
                let mut found_reasoning = false;

                for row_res in rows.flatten() {
                    let (data_str, _ts) = row_res;
                    let v: Value = match serde_json::from_str(&data_str) {
                        Ok(val) => val,
                        Err(_) => continue,
                    };

                    let part_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    // 提取工具调用
                    if !found_tool && part_type == "tool" {
                        let tool_name = v.get("tool").and_then(|t| t.as_str()).map(|s| s.to_string());
                        let tool_status = v.get("state")
                            .and_then(|s| s.get("status"))
                            .and_then(|st| st.as_str())
                            .map(|s| s.to_string());

                        let input_preview = extract_tool_input_preview(&v);

                        info.last_tool = tool_name;
                        info.last_tool_status = tool_status;
                        info.last_tool_input_preview = input_preview;
                        found_tool = true;
                    }

                    // 提取推理思考或文本
                    if !found_reasoning && (part_type == "reasoning" || part_type == "text") {
                        if let Some(txt) = v.get("text").and_then(|t| t.as_str()) {
                            let trimmed = txt.trim();
                            if !trimmed.is_empty() {
                                let preview: String = trimmed.chars().take(140).collect();
                                info.last_reasoning = Some(if trimmed.chars().count() > 140 {
                                    format!("{}…", preview)
                                } else {
                                    preview
                                });
                                found_reasoning = true;
                            }
                        }
                    }

                    if found_tool && found_reasoning {
                        break;
                    }
                }
            }
        }

        // 4. 读取待办任务 todo 表
        let todos_sql = "SELECT position, status, content FROM todo WHERE session_id = ?1 ORDER BY position ASC LIMIT 10";
        if let Ok(mut stmt) = conn.prepare(todos_sql) {
            if let Ok(rows) = stmt.query_map([&sid], |row| {
                Ok(OpenCodeTodoInfo {
                    position: row.get::<_, i32>(0)?,
                    status: row.get::<_, String>(1)?,
                    content: row.get::<_, String>(2)?,
                })
            }) {
                info.todos = rows.flatten().collect();
            }
        }
    }

    subagent_ids
}

/// 解析存储中的模型标识
fn parse_model_id(raw_model: &str) -> String {
    if let Ok(val) = serde_json::from_str::<Value>(raw_model) {
        if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
            return id.to_string();
        }
        if let Some(id) = val.get("modelID").and_then(|v| v.as_str()) {
            return id.to_string();
        }
    }
    raw_model.to_string()
}

/// 提取工具调用的参数简要预览
pub(super) fn extract_tool_input_preview(v: &Value) -> Option<String> {
    let input = v.get("state").and_then(|s| s.get("input"))?;

    if let Some(cmd) = input.get("command").and_then(|c| c.as_str()) {
        let single_line = cmd.lines().next().unwrap_or("").trim();
        return Some(truncate_str(single_line, 80));
    }
    if let Some(path) = input.get("filePath").or_else(|| input.get("path")).and_then(|p| p.as_str()) {
        return Some(truncate_str(path, 80));
    }
    if let Some(pattern) = input.get("pattern").and_then(|p| p.as_str()) {
        return Some(format!("pattern: {}", truncate_str(pattern, 60)));
    }
    if let Some(query) = input.get("query").and_then(|q| q.as_str()) {
        return Some(format!("query: {}", truncate_str(query, 60)));
    }

    None
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() > max_len {
        let prefix: String = trimmed.chars().take(max_len).collect();
        format!("{}…", prefix)
    } else {
        trimmed.to_string()
    }
}

/// 快速从日志文件的末尾段检查是否有匹配主会话及其子代理的最新 busy/idle 状态
fn check_busy_state_from_log(log_path: &Path, session_ids: &[&str]) -> bool {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    if session_ids.is_empty() {
        return false;
    }

    let mut file = match File::open(log_path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(_) => return false,
    };

    let file_len = metadata.len();
    if file_len == 0 {
        return false;
    }

    // 读取最后 512 KB 数据
    let read_size = std::cmp::min(file_len, 512 * 1024) as usize;
    let seek_pos = file_len.saturating_sub(read_size as u64);
    if file.seek(SeekFrom::Start(seek_pos)).is_err() {
        return false;
    }

    let mut buffer = vec![0u8; read_size];
    if file.read_exact(&mut buffer).is_err() {
        return false;
    }

    let text = String::from_utf8_lossy(&buffer);
    let mut last_status: Option<bool> = None;

    for line in text.lines() {
        let matches_session = session_ids.iter().any(|sid| line.contains(*sid));
        if matches_session {
            if line.contains("type=session.status") {
                if line.contains("status=busy") {
                    last_status = Some(true);
                } else if line.contains("status=idle") {
                    last_status = Some(false);
                }
            } else if line.contains("type=session.idle") {
                last_status = Some(false);
            }
        }
    }

    // 只有在日志中明确且最新判定为 busy 时才算 busy；无记录或 idle 均视为已非活跃进行
    last_status.unwrap_or(false)
}

/// 格式化 Token 计数展示（例如 1.83M 或 125K）
pub fn format_token_count(count: i64) -> String {
    if count >= 1_000_000 {
        format!("{:.2}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

#[cfg(test)]
mod session_selection {
    use super::*;
    use crate::remote_bot::types::ExternalOpenCodeSessionInfo;
    use rusqlite::Connection;
    use std::path::PathBuf;

    fn make_db(path: &std::path::Path) {
        let conn = Connection::open(path).unwrap();
        for table in ["session", "session_v2"] {
            conn.execute(
                &format!(
                    "CREATE TABLE {} (
                        id TEXT,
                        title TEXT,
                        agent TEXT,
                        model TEXT,
                        tokens_input INTEGER,
                        tokens_output INTEGER,
                        tokens_reasoning INTEGER,
                        tokens_cache_read INTEGER,
                        time_updated INTEGER,
                        directory TEXT,
                        parent_id TEXT
                    )",
                    table
                ),
                [],
            )
            .unwrap();
        }
    }

    fn make_info(cwd: &str) -> ExternalOpenCodeSessionInfo {
        ExternalOpenCodeSessionInfo {
            pid: 1,
            ppid: 0,
            elapsed: String::new(),
            command: String::new(),
            cwd: cwd.to_string(),
            project_name: String::new(),
            session_id: None,
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

    #[test]
    fn selects_newest_session_across_v1_and_v2() {
        let base =
            std::env::temp_dir().join(format!("oc_session_selection_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let db_path: PathBuf = base.join("opencode.db");
        let _ = std::fs::remove_file(&db_path);
        make_db(&db_path);

        let conn = Connection::open(&db_path).unwrap();
        let dir_a = "/tmp/onespace_test_dir_a";
        let dir_b = "/tmp/onespace_test_dir_b";

        // Directory A: stale v2 (older) vs active v1 (newer)
        conn.execute(
            "INSERT INTO session_v2 (id, title, agent, model, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, time_updated, directory, parent_id) VALUES (?1, NULL, NULL, NULL, 0, 0, 0, 0, ?2, ?3, NULL)",
            rusqlite::params!["ses_stale_v2", 1000i64, dir_a],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, agent, model, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, time_updated, directory, parent_id) VALUES (?1, NULL, NULL, NULL, 0, 0, 0, 0, ?2, ?3, NULL)",
            rusqlite::params!["ses_active_v1", 2000i64, dir_a],
        )
        .unwrap();

        // Directory B: active v2 (newer) vs stale v1 (older)
        conn.execute(
            "INSERT INTO session_v2 (id, title, agent, model, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, time_updated, directory, parent_id) VALUES (?1, NULL, NULL, NULL, 0, 0, 0, 0, ?2, ?3, NULL)",
            rusqlite::params!["ses_active_v2", 4000i64, dir_b],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, agent, model, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, time_updated, directory, parent_id) VALUES (?1, NULL, NULL, NULL, 0, 0, 0, 0, ?2, ?3, NULL)",
            rusqlite::params!["ses_stale_v1", 3000i64, dir_b],
        )
        .unwrap();
        drop(conn);

        // Case A: should pick the newer legacy session, not the stale v2 row.
        let mut info_a = make_info(dir_a);
        let _ = enrich_session_from_db(&mut info_a, &db_path, dir_a);
        assert_eq!(
            info_a.session_id,
            Some("ses_active_v1".to_string()),
            "case A: newer legacy session should win over stale v2 session"
        );

        // Case B: newer v2 record should still be selected.
        let mut info_b = make_info(dir_b);
        let _ = enrich_session_from_db(&mut info_b, &db_path, dir_b);
        assert_eq!(
            info_b.session_id,
            Some("ses_active_v2".to_string()),
            "case B: newer v2 session should win over stale legacy session"
        );

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&base);
    }
}
