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
) -> ProviderRecord {
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
    ProviderRecord {
        core: ProviderCore {
            id: id.to_string(),
            name: name.to_string(),
            tool: "codex".to_string(),
            api_key: key.to_string(),
            code: None,
            base_url: Some(base_url.to_string()),
            model: Some(model.to_string()),
        },
        runtime_policy: ProviderRuntimePolicy {
            approval_policy: Some("never".to_string()),
            sandbox_mode: Some("workspace-write".to_string()),
        },
        favorite_at: None,
        tool_config,
        ..ProviderRecord::default()
    }
}

pub(super) fn rendered_content(outputs: &[(PathBuf, String)], suffix: &str) -> String {
    outputs
        .iter()
        .find(|(path, _)| path.ends_with(suffix))
        .map(|(_, content)| content.clone())
        .unwrap_or_else(|| panic!("missing rendered output for {}", suffix))
}
