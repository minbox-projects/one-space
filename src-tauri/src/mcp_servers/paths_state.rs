static JOB_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNNING_JOB_KEYS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn job_lock() -> &'static Mutex<()> {
    JOB_LOCK.get_or_init(|| Mutex::new(()))
}

fn running_job_keys() -> &'static Mutex<HashSet<String>> {
    RUNNING_JOB_KEYS.get_or_init(|| Mutex::new(HashSet::new()))
}

struct JobKeyGuard {
    key: String,
}

impl Drop for JobKeyGuard {
    fn drop(&mut self) {
        if let Ok(mut running) = running_job_keys().lock() {
            running.remove(&self.key);
        }
    }
}

fn acquire_job_key(key: impl Into<String>) -> Result<Option<JobKeyGuard>, String> {
    let key = key.into();
    let mut running = running_job_keys().lock().map_err(|e| e.to_string())?;
    if running.contains(&key) {
        return Ok(None);
    }
    running.insert(key.clone());
    Ok(Some(JobKeyGuard { key }))
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn api_ok<T>(data: T) -> Result<ApiOk<T>, String> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            revision: 0,
            ts: now_ts(),
        },
    })
}

fn get_mcp_servers_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data").join("mcp");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.json"))
}

fn get_legacy_mcp_servers_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    Ok(data_dir.join("mcp_servers.json"))
}

fn get_claude_mcp_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".claude.json"))
}

fn get_workspace_claude_mcp_path(project_root: &str) -> PathBuf {
    PathBuf::from(project_root).join(".mcp.json")
}

fn get_codex_mcp_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".codex").join("config.toml"))
}

fn get_workspace_codex_mcp_path(project_root: &str) -> PathBuf {
    PathBuf::from(project_root)
        .join(".codex")
        .join("config.toml")
}

fn get_gemini_mcp_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".gemini").join("settings.json"))
}

fn get_workspace_gemini_mcp_path(project_root: &str) -> PathBuf {
    PathBuf::from(project_root)
        .join(".gemini")
        .join("settings.json")
}

fn get_opencode_mcp_primary_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".opencode").join("mcp.json"))
}

fn get_workspace_opencode_mcp_path(project_root: &str) -> PathBuf {
    PathBuf::from(project_root).join("opencode.json")
}

fn get_opencode_mcp_compat_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    Ok(home.join(".config").join("opencode").join("opencode.json"))
}

fn get_local_install_state_path() -> Result<PathBuf, String> {
    let app_dir = crate::config::get_app_dir()?;
    let dir = app_dir.join("mcp");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("local_install_state.json"))
}

fn get_updates_state_path() -> Result<PathBuf, String> {
    let app_dir = crate::config::get_app_dir()?;
    let dir = app_dir.join("mcp");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("updates_state.json"))
}

fn trigger_storage_sync(app: tauri::AppHandle, reason: &str) {
    let reason = reason.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = crate::app_store::sync_enqueue(app, reason).await;
    });
}
