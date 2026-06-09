use super::{dedupe_strings, normalize_working_dir_for_terminal};
use chrono::DateTime;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(in crate::ai_sessions) fn now_epoch_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub(in crate::ai_sessions) fn system_time_to_epoch_millis(ts: SystemTime) -> i64 {
    ts.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub(in crate::ai_sessions) fn parse_rfc3339_millis(input: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(input)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

pub(in crate::ai_sessions) fn canonicalize_to_string(path: &str) -> String {
    normalize_working_dir_for_terminal(path)
}

pub(in crate::ai_sessions) fn same_working_dir(left: &str, right: &str) -> bool {
    canonicalize_to_string(left) == canonicalize_to_string(right)
}

pub(in crate::ai_sessions) fn candidate_home_dirs(
    env: Option<&HashMap<String, String>>,
) -> Vec<PathBuf> {
    let mut homes = Vec::<PathBuf>::new();
    if let Some(env_home) = env
        .and_then(|vars| vars.get("HOME"))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        homes.push(PathBuf::from(env_home));
    }
    if let Some(system_home) = dirs::home_dir() {
        homes.push(system_home);
    }
    let mut deduped = Vec::<PathBuf>::new();
    let mut seen = HashSet::<String>::new();
    for home in homes {
        let key = fs::canonicalize(&home)
            .unwrap_or_else(|_| home.clone())
            .to_string_lossy()
            .to_string();
        if seen.insert(key) {
            deduped.push(home);
        }
    }
    deduped
}

pub(in crate::ai_sessions) fn build_create_seed_session_id(
    model_type: &str,
    requested_session_id: Option<&str>,
) -> Option<String> {
    if model_type.eq_ignore_ascii_case("claude") {
        let requested = requested_session_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string());
        return Some(requested.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()));
    }
    None
}

pub(in crate::ai_sessions) fn resolve_native_session_id_after_create(
    model_type: &str,
    working_dir: &str,
    seed_session_id: Option<&str>,
    launch_started_at_ms: i64,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    // Gemini and Opencode start slowly - allow more attempts (15 seconds)
    let max_attempts = if model_type.eq_ignore_ascii_case("gemini")
        || model_type.eq_ignore_ascii_case("opencode")
    {
        30
    } else {
        12
    };

    for attempt in 0..max_attempts {
        if let Some(id) = resolve_native_session_id_once(
            model_type,
            working_dir,
            seed_session_id,
            launch_started_at_ms,
            env,
        ) {
            return Some(id);
        }
        if attempt + 1 < max_attempts {
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    // For Gemini/Opencode, the session is already running even if we couldn't detect the ID
    // Return None to indicate "unbound" status rather than an error
    // The session will be bound later via pending_bind mechanism
    None
}

pub(in crate::ai_sessions) fn resolve_native_session_id_once(
    model_type: &str,
    working_dir: &str,
    seed_session_id: Option<&str>,
    launch_started_at_ms: i64,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    match model_type.to_lowercase().as_str() {
        "claude" => {
            resolve_claude_session_id(working_dir, launch_started_at_ms, env).or_else(|| {
                seed_session_id
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(String::from)
            })
        }
        "gemini" => resolve_gemini_session_id(working_dir, launch_started_at_ms),
        "codex" => resolve_codex_session_id(working_dir, launch_started_at_ms, env),
        "opencode" => resolve_opencode_session_id(working_dir, launch_started_at_ms),
        _ => None,
    }
}

pub(in crate::ai_sessions) fn resolve_codex_session_id_at_home(
    home: &Path,
    working_dir: &str,
    launch_started_at_ms: Option<i64>,
    max_scan: usize,
) -> Option<String> {
    #[derive(Debug, Deserialize)]
    struct CodexIndexEntry {
        id: String,
        #[serde(default)]
        updated_at: Option<String>,
    }

    let index_path = home.join(".codex").join("session_index.jsonl");
    let content = fs::read_to_string(index_path).ok()?;

    let mut entries: Vec<(String, i64)> = content
        .lines()
        .filter_map(|line| serde_json::from_str::<CodexIndexEntry>(line).ok())
        .map(|entry| {
            let updated = entry
                .updated_at
                .as_deref()
                .and_then(parse_rfc3339_millis)
                .unwrap_or(0);
            (entry.id, updated)
        })
        .collect();

    entries.sort_by(|a, b| b.1.cmp(&a.1));
    if let Some(launch_started_at_ms) = launch_started_at_ms {
        entries.retain(|(_, updated_at_ms)| *updated_at_ms + 15_000 >= launch_started_at_ms);
    }
    if entries.is_empty() {
        return None;
    }

    let sessions_root = home.join(".codex").join("sessions");
    for (id, _) in entries.iter().take(max_scan) {
        if let Some(path) = find_codex_session_file_for_id(&sessions_root, id) {
            if let Some(cwd) = read_codex_session_cwd(&path) {
                if same_working_dir(&cwd, working_dir) {
                    return Some(id.clone());
                }
            }
        }
    }

    fallback_codex_session_id_by_scan(
        &sessions_root,
        working_dir,
        launch_started_at_ms,
        max_scan * 8,
    )
}

pub(in crate::ai_sessions) fn resolve_codex_session_id(
    working_dir: &str,
    launch_started_at_ms: i64,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    for home in candidate_home_dirs(env) {
        if let Some(id) =
            resolve_codex_session_id_at_home(&home, working_dir, Some(launch_started_at_ms), 20)
        {
            return Some(id);
        }
    }
    None
}

pub(in crate::ai_sessions) fn resolve_codex_session_id_for_existing(
    working_dir: &str,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    for home in candidate_home_dirs(env) {
        if let Some(id) = resolve_codex_session_id_at_home(&home, working_dir, None, 80) {
            return Some(id);
        }
    }
    None
}

pub(in crate::ai_sessions) fn find_codex_session_file_for_id(
    root: &Path,
    session_id: &str,
) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if name.contains(session_id) && name.ends_with(".jsonl") {
                return Some(path);
            }
        }
    }
    None
}

pub(in crate::ai_sessions) fn read_codex_session_cwd(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    let line_len = reader.read_line(&mut first_line).ok()?;
    if line_len == 0 {
        return None;
    }
    let value: Value = serde_json::from_str(first_line.trim()).ok()?;
    if value.get("type").and_then(|v| v.as_str()) != Some("session_meta") {
        return None;
    }
    value
        .get("payload")
        .and_then(|payload| payload.get("cwd"))
        .and_then(|cwd| cwd.as_str())
        .map(|cwd| cwd.to_string())
}

pub(in crate::ai_sessions) fn collect_codex_session_files(
    root: &Path,
    limit: usize,
) -> Vec<(PathBuf, i64)> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::<(PathBuf, i64)>::new();
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".jsonl"))
                .unwrap_or(false)
            {
                continue;
            }
            let modified_ms = fs::metadata(&path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
                .unwrap_or(0);
            files.push((path, modified_ms));
        }
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files.truncate(limit);
    files
}

pub(in crate::ai_sessions) fn read_codex_session_meta(
    path: &Path,
) -> Option<(String, String, i64)> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    let line_len = reader.read_line(&mut first_line).ok()?;
    if line_len == 0 {
        return None;
    }
    let value: Value = serde_json::from_str(first_line.trim()).ok()?;
    if value.get("type").and_then(|v| v.as_str()) != Some("session_meta") {
        return None;
    }
    let payload = value.get("payload")?;
    let id = payload.get("id").and_then(|v| v.as_str())?.to_string();
    let cwd = payload.get("cwd").and_then(|v| v.as_str())?.to_string();
    let timestamp_ms = payload
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
        })?;
    Some((id, cwd, timestamp_ms))
}

pub(in crate::ai_sessions) fn fallback_codex_session_id_by_scan(
    sessions_root: &Path,
    working_dir: &str,
    launch_started_at_ms: Option<i64>,
    max_scan: usize,
) -> Option<String> {
    let normalized_working_dir = canonicalize_to_string(working_dir);
    let mut best: Option<(String, i64)> = None;
    for (path, _) in collect_codex_session_files(sessions_root, max_scan) {
        let Some((id, cwd, ts_ms)) = read_codex_session_meta(&path) else {
            continue;
        };
        if let Some(launch_started_at_ms) = launch_started_at_ms {
            if ts_ms + 15_000 < launch_started_at_ms {
                continue;
            }
        }
        if !same_working_dir(&cwd, &normalized_working_dir) {
            continue;
        }
        match &best {
            Some((_, best_ts_ms)) if *best_ts_ms >= ts_ms => {}
            _ => best = Some((id, ts_ms)),
        }
    }
    best.map(|(id, _)| id)
}

pub(in crate::ai_sessions) const GEMINI_BIND_WINDOW_MS: i64 = 15 * 60 * 1000;
pub(in crate::ai_sessions) const GEMINI_CREATE_GRACE_MS: i64 = 15_000;

#[derive(Debug, Clone)]
pub(in crate::ai_sessions) struct GeminiSessionCandidate {
    pub(in crate::ai_sessions) session_id: String,
    pub(in crate::ai_sessions) start_at_ms: i64,
    pub(in crate::ai_sessions) updated_at_ms: i64,
}

pub(in crate::ai_sessions) fn select_gemini_session_for_create(
    candidates: &[GeminiSessionCandidate],
    launch_started_at_ms: i64,
) -> Option<String> {
    let mut best_near_start: Option<(String, i64, i64)> = None;
    let mut best_recent_update: Option<(String, i64)> = None;

    for candidate in candidates {
        if candidate.updated_at_ms + GEMINI_CREATE_GRACE_MS < launch_started_at_ms {
            continue;
        }
        match &best_recent_update {
            Some((_, best_updated_at_ms)) if *best_updated_at_ms >= candidate.updated_at_ms => {}
            _ => {
                best_recent_update = Some((candidate.session_id.clone(), candidate.updated_at_ms));
            }
        }

        if candidate.start_at_ms + GEMINI_CREATE_GRACE_MS < launch_started_at_ms {
            continue;
        }
        let diff_ms = (candidate.start_at_ms - launch_started_at_ms).abs();
        match &best_near_start {
            Some((_, best_diff_ms, best_updated_at_ms))
                if *best_diff_ms < diff_ms
                    || (*best_diff_ms == diff_ms
                        && *best_updated_at_ms >= candidate.updated_at_ms) => {}
            _ => {
                best_near_start = Some((
                    candidate.session_id.clone(),
                    diff_ms,
                    candidate.updated_at_ms,
                ))
            }
        }
    }

    best_near_start
        .map(|(session_id, _, _)| session_id)
        .or_else(|| best_recent_update.map(|(session_id, _)| session_id))
}

pub(in crate::ai_sessions) fn select_gemini_session_for_existing(
    candidates: &[GeminiSessionCandidate],
    created_at_ms: Option<i64>,
) -> Option<String> {
    if let Some(created_at_ms) = created_at_ms {
        let mut best_near_start: Option<(String, i64, i64)> = None;

        for candidate in candidates {
            let start_diff_ms = (candidate.start_at_ms - created_at_ms).abs();
            if start_diff_ms <= GEMINI_BIND_WINDOW_MS {
                match &best_near_start {
                    Some((_, best_diff_ms, best_updated_at_ms))
                        if *best_diff_ms < start_diff_ms
                            || (*best_diff_ms == start_diff_ms
                                && *best_updated_at_ms >= candidate.updated_at_ms) => {}
                    _ => {
                        best_near_start = Some((
                            candidate.session_id.clone(),
                            start_diff_ms,
                            candidate.updated_at_ms,
                        ))
                    }
                }
            }
        }

        return best_near_start.map(|(session_id, _, _)| session_id);
    }

    candidates
        .iter()
        .max_by_key(|candidate| candidate.updated_at_ms)
        .map(|candidate| candidate.session_id.clone())
}

pub(in crate::ai_sessions) fn collect_gemini_session_candidates(
    working_dir: &str,
    exclude_ids: Option<&HashSet<String>>,
) -> Vec<GeminiSessionCandidate> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut candidates = Vec::<GeminiSessionCandidate>::new();

    for identifier in gemini_project_identifiers(working_dir) {
        let chats_dir = home
            .join(".gemini")
            .join("tmp")
            .join(identifier)
            .join("chats");
        if !chats_dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(chats_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if !name.starts_with("session-") || !name.ends_with(".json") {
                continue;
            }
            let Some(candidate) = read_gemini_chat_file(&path) else {
                continue;
            };
            if exclude_ids
                .map(|ids| ids.contains(&candidate.session_id))
                .unwrap_or(false)
            {
                continue;
            }
            candidates.push(candidate);
        }
    }

    candidates
}

pub(in crate::ai_sessions) fn resolve_gemini_session_id(
    working_dir: &str,
    launch_started_at_ms: i64,
) -> Option<String> {
    let candidates = collect_gemini_session_candidates(working_dir, None);
    select_gemini_session_for_create(&candidates, launch_started_at_ms)
}

pub(in crate::ai_sessions) fn resolve_gemini_session_id_for_existing(
    working_dir: &str,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
) -> Option<String> {
    let candidates = collect_gemini_session_candidates(working_dir, exclude_ids);
    select_gemini_session_for_existing(&candidates, created_at_ms)
}

pub(in crate::ai_sessions) fn resolve_gemini_session_id_for_pending_bind(
    working_dir: &str,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
) -> Option<String> {
    let created_at_ms = created_at_ms?;
    let candidates = collect_gemini_session_candidates(working_dir, exclude_ids);
    select_gemini_session_for_create(&candidates, created_at_ms)
}

pub(in crate::ai_sessions) fn resolve_claude_session_id(
    working_dir: &str,
    launch_started_at_ms: i64,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    let claude_dir = env
        .and_then(|e| e.get("CLAUDE_CONFIG_DIR"))
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_default();
            home.join(".claude")
        });
    let history_path = claude_dir.join("history.jsonl");

    let content = fs::read_to_string(history_path).ok()?;

    let mut candidates: Vec<(String, i64, String)> = content
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            let session_id = value.get("sessionId")?.as_str()?.to_string();
            let timestamp = value.get("timestamp")?.as_i64()?;
            let project = value.get("project")?.as_str()?.to_string();
            Some((session_id, timestamp, project))
        })
        .collect();

    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let normalized_working_dir = canonicalize_to_string(working_dir);
    for (session_id, timestamp, project) in candidates.iter().take(100) {
        if *timestamp < launch_started_at_ms - 15000 || *timestamp > launch_started_at_ms + 15000 {
            continue;
        }
        let normalized_project = canonicalize_to_string(project);
        if same_working_dir(&normalized_project, &normalized_working_dir) {
            return Some(session_id.clone());
        }
    }

    None
}

pub(in crate::ai_sessions) fn resolve_claude_session_id_for_existing(
    working_dir: &str,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
    env: Option<&HashMap<String, String>>,
) -> Option<String> {
    let claude_dir = env
        .and_then(|e| e.get("CLAUDE_CONFIG_DIR"))
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_default();
            home.join(".claude")
        });
    let history_path = claude_dir.join("history.jsonl");

    let content = fs::read_to_string(history_path).ok()?;

    let mut candidates: Vec<(String, i64, String)> = content
        .lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            let session_id = value.get("sessionId")?.as_str()?.to_string();
            let timestamp = value.get("timestamp")?.as_i64()?;
            let project = value.get("project")?.as_str()?.to_string();
            Some((session_id, timestamp, project))
        })
        .collect();

    candidates.sort_by(|a, b| b.1.cmp(&a.1));

    let normalized_working_dir = canonicalize_to_string(working_dir);
    for (session_id, timestamp, project) in candidates.iter().take(200) {
        if let Some(exclude) = exclude_ids {
            if exclude.contains(session_id) {
                continue;
            }
        }

        let normalized_project = canonicalize_to_string(project);
        if !same_working_dir(&normalized_project, &normalized_working_dir) {
            continue;
        }

        if let Some(created_at) = created_at_ms {
            if (*timestamp - created_at).abs() > 15000 {
                continue;
            }
        }

        return Some(session_id.clone());
    }

    None
}

pub(in crate::ai_sessions) fn gemini_project_identifiers(working_dir: &str) -> Vec<String> {
    let normalized_working_dir = canonicalize_to_string(working_dir);
    let mut identifiers = Vec::<String>::new();

    let mut check_dirs = Vec::new();
    let mut current = PathBuf::from(&normalized_working_dir);
    loop {
        check_dirs.push(current.to_string_lossy().to_string());
        if !current.pop() {
            break;
        }
    }

    let Some(home) = dirs::home_dir() else {
        return identifiers;
    };
    let projects_path = home.join(".gemini").join("projects.json");
    if let Ok(content) = fs::read_to_string(projects_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&content) {
            if let Some(projects) = value
                .get("projects")
                .and_then(|projects| projects.as_object())
            {
                for dir in &check_dirs {
                    if let Some(identifier) = projects.get(dir).and_then(|value| value.as_str()) {
                        identifiers.push(identifier.to_string());
                    }
                    for (project_path, identifier) in projects {
                        if same_working_dir(project_path, dir) {
                            if let Some(identifier) = identifier.as_str() {
                                identifiers.push(identifier.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(normalized_working_dir.as_bytes());
    identifiers.push(format!("{:x}", hasher.finalize()));

    // 也为所有的父目录计算后备的 hash
    for dir in &check_dirs {
        let mut h = Sha256::new();
        h.update(dir.as_bytes());
        identifiers.push(format!("{:x}", h.finalize()));
    }

    dedupe_strings(identifiers)
}

pub(in crate::ai_sessions) fn read_gemini_chat_file(path: &Path) -> Option<GeminiSessionCandidate> {
    let content = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    let session_id = value.get("sessionId").and_then(|v| v.as_str())?.to_string();
    let start_at_ms = value
        .get("startTime")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.created().ok())
                .map(system_time_to_epoch_millis)
        })
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
        })?;
    let updated_at_ms = value
        .get("lastUpdated")
        .and_then(|v| v.as_str())
        .and_then(parse_rfc3339_millis)
        .or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_epoch_millis)
        })?;
    Some(GeminiSessionCandidate {
        session_id,
        start_at_ms,
        updated_at_ms,
    })
}

#[derive(Debug, Clone)]
pub(in crate::ai_sessions) struct OpencodeStoragePaths {
    pub(in crate::ai_sessions) sessions_root: PathBuf,
    pub(in crate::ai_sessions) messages_root: PathBuf,
    pub(in crate::ai_sessions) projects_root: PathBuf,
}

pub(in crate::ai_sessions) fn candidate_opencode_storage_paths() -> Vec<OpencodeStoragePaths> {
    let mut roots = Vec::<PathBuf>::new();
    if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty())
    {
        roots.push(PathBuf::from(xdg_data_home).join("opencode"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local").join("share").join("opencode"));
    }

    let mut out = Vec::<OpencodeStoragePaths>::new();
    let mut seen = HashSet::<String>::new();
    for root in roots {
        let storage_root = if root
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "storage")
            .unwrap_or(false)
        {
            root.clone()
        } else {
            root.join("storage")
        };
        let key = fs::canonicalize(&storage_root)
            .unwrap_or_else(|_| storage_root.clone())
            .to_string_lossy()
            .to_string();
        if !seen.insert(key) {
            continue;
        }
        let candidate = OpencodeStoragePaths {
            sessions_root: storage_root.join("session"),
            messages_root: storage_root.join("message"),
            projects_root: storage_root.join("project"),
        };
        if candidate.sessions_root.is_dir()
            || candidate.messages_root.is_dir()
            || candidate.projects_root.is_dir()
        {
            out.push(candidate);
        }
    }

    out
}

pub(in crate::ai_sessions) fn select_opencode_session_id_from_messages_root(
    messages_root: &Path,
    working_dir: &str,
    launch_started_at_ms: Option<i64>,
    max_scan: usize,
) -> Option<(String, i64)> {
    if !messages_root.is_dir() {
        return None;
    }

    let normalized_working_dir = canonicalize_to_string(working_dir);
    let mut sessions = Vec::<(PathBuf, i64)>::new();
    let Ok(entries) = fs::read_dir(messages_root) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let modified_ms = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_to_epoch_millis)
            .unwrap_or(0);
        sessions.push((path, modified_ms));
    }
    sessions.sort_by(|a, b| b.1.cmp(&a.1));

    let mut best: Option<(String, i64)> = None;
    for (session_dir, _) in sessions.into_iter().take(max_scan) {
        let Some((session_id, created_at_ms, cwd)) = read_opencode_session_dir(&session_dir) else {
            continue;
        };
        if let Some(launch_started_at_ms) = launch_started_at_ms {
            if created_at_ms + 15_000 < launch_started_at_ms {
                continue;
            }
        }
        if let Some(cwd) = cwd {
            if same_working_dir(&cwd, &normalized_working_dir) {
                match &best {
                    Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
                    _ => best = Some((session_id, created_at_ms)),
                }
                continue;
            }
        }
        // Fallback: check if the session directory name matches the working directory
        // This handles cases where opencode creates session directories named after the project
        if let Some(session_dir_name) = session_dir.file_name().and_then(|n| n.to_str()) {
            let normalized_session_dir = canonicalize_to_string(session_dir_name);
            if same_working_dir(&normalized_session_dir, &normalized_working_dir) {
                match &best {
                    Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
                    _ => best = Some((session_id, created_at_ms)),
                }
            }
        }
    }

    best
}

pub(in crate::ai_sessions) fn resolve_opencode_session_id(
    working_dir: &str,
    launch_started_at_ms: i64,
) -> Option<String> {
    let mut best: Option<(String, i64)> = None;
    for storage_paths in candidate_opencode_storage_paths() {
        let Some((session_id, created_at_ms)) = select_opencode_session_id_from_messages_root(
            &storage_paths.messages_root,
            working_dir,
            Some(launch_started_at_ms),
            200,
        ) else {
            continue;
        };
        match &best {
            Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
            _ => best = Some((session_id, created_at_ms)),
        }
    }
    best.map(|(session_id, _)| session_id)
}

pub(in crate::ai_sessions) fn resolve_opencode_session_id_for_existing(
    working_dir: &str,
) -> Option<String> {
    let mut best: Option<(String, i64)> = None;
    for storage_paths in candidate_opencode_storage_paths() {
        let Some((session_id, created_at_ms)) = select_opencode_session_id_from_messages_root(
            &storage_paths.messages_root,
            working_dir,
            None,
            400,
        ) else {
            continue;
        };
        match &best {
            Some((_, best_created_at_ms)) if *best_created_at_ms >= created_at_ms => {}
            _ => best = Some((session_id, created_at_ms)),
        }
    }
    best.map(|(session_id, _)| session_id)
}

pub(in crate::ai_sessions) fn read_opencode_session_dir(
    session_dir: &Path,
) -> Option<(String, i64, Option<String>)> {
    let mut message_files = Vec::<(PathBuf, i64)>::new();
    let Ok(entries) = fs::read_dir(session_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !name.starts_with("msg_") || !name.ends_with(".json") {
            continue;
        }
        let modified_ms = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(system_time_to_epoch_millis)
            .unwrap_or(0);
        message_files.push((path, modified_ms));
    }
    message_files.sort_by(|a, b| b.1.cmp(&a.1));

    let fallback_session_id = session_dir
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())?;

    let mut best: Option<(String, i64, Option<String>)> = None;
    for (path, _) in message_files.into_iter().take(10) {
        let content = fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&content).ok()?;
        let session_id = value
            .get("sessionID")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .unwrap_or_else(|| fallback_session_id.clone());
        let created_at_ms = value
            .get("time")
            .and_then(|time| time.get("created"))
            .and_then(|created| created.as_i64())
            .or_else(|| {
                value
                    .get("updatedAt")
                    .and_then(|updated_at| updated_at.as_i64())
            })?;
        let cwd = value
            .get("path")
            .and_then(|path| path.get("cwd").or_else(|| path.get("root")))
            .and_then(|cwd| cwd.as_str())
            .map(|cwd| cwd.to_string())
            .or_else(|| {
                value
                    .get("directory")
                    .and_then(|dir| dir.as_str())
                    .map(|dir| dir.to_string())
            });
        match &best {
            Some((_, best_created_at_ms, best_cwd))
                if *best_created_at_ms > created_at_ms
                    || (*best_created_at_ms == created_at_ms && best_cwd.is_some()) => {}
            _ => best = Some((session_id, created_at_ms, cwd)),
        }
    }
    best
}
