use super::*;

pub(super) fn make_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "onespace-app-store-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ))
}

pub(super) fn write_test_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, content).expect("write file");
}

/// Retains the global `HOME` lock on purpose. This helper is shared by app_store
/// test modules whose callers include tool projection (Claude / Codex /
/// Antigravity configs written under the real home via `dirs::home_dir()`),
/// which the thread-local `get_app_dir()` override does not cover. Migrating the
/// shared helper would require changing those caller test modules, which are
/// outside this task's write scope, so the group stays serialized.
pub(super) fn with_temp_dir<T>(name: &str, f: impl FnOnce(&Path) -> T) -> T {
    let _guard = crate::lock_test_home_env();
    let temp_home = make_temp_dir(name);
    fs::create_dir_all(&temp_home).expect("create temp home");
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &temp_home);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&temp_home)));
    if let Some(home) = original_home {
        std::env::set_var("HOME", home);
    } else {
        std::env::remove_var("HOME");
    }
    let _ = fs::remove_dir_all(&temp_home);
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub(super) fn codex_provider(
    id: &str,
    name: &str,
    key: &str,
    base_url: &str,
    model: &str,
) -> ServiceProviderRecord {
    let mut tool_config = Map::new();
    tool_config.insert(
        "wire_api".to_string(),
        Value::String("responses".to_string()),
    );
    tool_config.insert(
        "model_reasoning_effort".to_string(),
        Value::String("high".to_string()),
    );
    tool_config.insert(
        "approval_policy".to_string(),
        Value::String("never".to_string()),
    );
    tool_config.insert(
        "sandbox_mode".to_string(),
        Value::String("workspace-write".to_string()),
    );
    ServiceProviderRecord {
        id: id.to_string(),
        name: name.to_string(),
        tool: "codex".to_string(),
        api_key: key.to_string(),
        base_url: Some(base_url.to_string()),
        model: Some(model.to_string()),
        tool_config,
        ..ServiceProviderRecord::default()
    }
}

pub(super) fn rendered_content(outputs: &[(PathBuf, String)], suffix: &str) -> String {
    outputs
        .iter()
        .find(|(path, _)| path.ends_with(suffix))
        .map(|(_, content)| content.clone())
        .unwrap_or_else(|| panic!("missing rendered output for {}", suffix))
}
