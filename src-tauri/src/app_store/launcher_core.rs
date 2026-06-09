use super::{LauncherItemInput, LauncherRecord, LAUNCHER_TYPES};
#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{json, Value};
use std::fs::{self};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(in crate::app_store) fn launcher_to_legacy(record: &LauncherRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "type": record.item_type,
        "target": record.target,
        "pinned": record.pinned,
        "pin_order": record.pin_order,
        "launch_count": record.launch_count,
        "last_launched_at": record.last_launched_at,
        "trusted": record.trusted,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}

pub(in crate::app_store) fn is_valid_launcher_type(item_type: &str) -> bool {
    LAUNCHER_TYPES.contains(&item_type)
}

pub(in crate::app_store) fn sanitize_launcher_record(
    record: &mut LauncherRecord,
) -> Result<(), String> {
    record.name = record.name.trim().to_string();
    record.target = record.target.trim().to_string();
    record.item_type = record.item_type.trim().to_lowercase();
    if record.id.trim().is_empty() {
        record.id = uuid::Uuid::new_v4().to_string();
    }
    if record.name.is_empty() {
        return Err("launcher name required".to_string());
    }
    if record.target.is_empty() {
        return Err("launcher target required".to_string());
    }
    if !is_valid_launcher_type(&record.item_type) {
        return Err(format!("invalid launcher type: {}", record.item_type));
    }
    if record.item_type == "app" {
        record.target = normalize_app_target(&record.target)?;
    }
    if record.item_type != "script" {
        record.trusted = true;
    }
    if !record.pinned {
        record.pin_order = 0;
    }
    Ok(())
}

pub(in crate::app_store) fn normalize_app_target(raw: &str) -> Result<String, String> {
    let mut target = raw.trim().to_string();
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("open -a ") {
        target = target[8..].trim().to_string();
    } else if lower.starts_with("open -a") {
        target = target[7..].trim().to_string();
    }
    target = target
        .trim()
        .trim_matches(is_wrapped_quote_char)
        .trim()
        .to_string();
    if target.is_empty() {
        return Err("app target required".to_string());
    }
    Ok(target)
}

pub(in crate::app_store) fn is_wrapped_quote_char(c: char) -> bool {
    matches!(c, '"' | '\'' | '`' | '“' | '”' | '‘' | '’')
}

pub(in crate::app_store) fn launcher_application_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
}

pub(in crate::app_store) fn resolve_application_bundle_path(app_name: &str) -> Option<PathBuf> {
    let trimmed = app_name.trim();
    if trimmed.is_empty() {
        return None;
    }

    let direct = PathBuf::from(trimmed);
    if direct.exists()
        && direct
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
    {
        return Some(direct);
    }

    let normalized = trimmed.trim_end_matches(".app");
    let normalized_lower = normalized.to_lowercase();
    if normalized_lower.is_empty() {
        return None;
    }

    for root in launcher_application_roots() {
        let exact = root.join(format!("{}.app", normalized));
        if exact.exists() {
            return Some(exact);
        }
    }

    for root in launcher_application_roots() {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if !ext.eq_ignore_ascii_case("app") {
                continue;
            }
            let file_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if file_name.contains(&normalized_lower) || normalized_lower.contains(&file_name) {
                return Some(path);
            }
        }
    }

    None
}

pub(in crate::app_store) fn normalize_icon_candidate_name(raw: &str) -> Option<String> {
    let name = raw.trim().trim_matches(is_wrapped_quote_char).trim();
    if name.is_empty() {
        return None;
    }
    if name.to_ascii_lowercase().ends_with(".icns") {
        return Some(name.to_string());
    }
    Some(format!("{}.icns", name))
}

pub(in crate::app_store) fn push_icon_candidate(candidates: &mut Vec<String>, raw: Option<&str>) {
    let Some(value) = raw else {
        return;
    };
    let Some(normalized) = normalize_icon_candidate_name(value) else {
        return;
    };
    if !candidates
        .iter()
        .any(|item| item.eq_ignore_ascii_case(&normalized))
    {
        candidates.push(normalized);
    }
}

pub(in crate::app_store) fn extract_icon_candidates_from_plist_json(plist: &Value) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    push_icon_candidate(
        &mut candidates,
        plist.get("CFBundleIconFile").and_then(|v| v.as_str()),
    );
    push_icon_candidate(
        &mut candidates,
        plist.get("CFBundleIconName").and_then(|v| v.as_str()),
    );

    if let Some(icon_files) = plist
        .pointer("/CFBundleIcons/CFBundlePrimaryIcon/CFBundleIconFiles")
        .and_then(|v| v.as_array())
    {
        for item in icon_files.iter().rev() {
            push_icon_candidate(&mut candidates, item.as_str());
        }
    }

    if let Some(icon_files) = plist.get("CFBundleIconFiles").and_then(|v| v.as_array()) {
        for item in icon_files.iter().rev() {
            push_icon_candidate(&mut candidates, item.as_str());
        }
    }

    push_icon_candidate(&mut candidates, Some("AppIcon"));
    candidates
}

pub(in crate::app_store) fn find_icns_path(
    resources_dir: &Path,
    candidates: &[String],
) -> Option<PathBuf> {
    if !resources_dir.is_dir() {
        return None;
    }

    for candidate in candidates {
        let path = resources_dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    let mut available_icons: Vec<PathBuf> = fs::read_dir(resources_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("icns"))
                .unwrap_or(false)
        })
        .collect();
    available_icons.sort();

    for candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        if let Some(path) = available_icons.iter().find(|path| {
            path.file_name()
                .and_then(|s| s.to_str())
                .map(|name| name.to_lowercase() == candidate_lower)
                .unwrap_or(false)
        }) {
            return Some(path.clone());
        }
    }

    if let Some(path) = available_icons.iter().find(|path| {
        path.file_name()
            .and_then(|s| s.to_str())
            .map(|name| name.to_ascii_lowercase().contains("appicon"))
            .unwrap_or(false)
    }) {
        return Some(path.clone());
    }

    available_icons.into_iter().next()
}

#[cfg(target_os = "macos")]
pub(in crate::app_store) fn read_info_plist_json(app_bundle_path: &Path) -> Option<Value> {
    let info_plist = app_bundle_path.join("Contents").join("Info.plist");
    let output = Command::new("plutil")
        .arg("-convert")
        .arg("json")
        .arg("-o")
        .arg("-")
        .arg(info_plist)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice::<Value>(&output.stdout).ok()
}

#[cfg(target_os = "macos")]
pub(in crate::app_store) fn convert_icns_to_png_data_url(icns_path: &Path) -> Option<String> {
    let output_path = std::env::temp_dir().join(format!(
        "onespace-launcher-icon-{}.png",
        uuid::Uuid::new_v4()
    ));
    let status = Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(icns_path)
        .arg("--out")
        .arg(&output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        let _ = fs::remove_file(&output_path);
        return None;
    }
    let png = fs::read(&output_path).ok();
    let _ = fs::remove_file(&output_path);
    png.map(|bytes| format!("data:image/png;base64,{}", BASE64_STANDARD.encode(bytes)))
}

#[cfg(target_os = "macos")]
pub(in crate::app_store) fn resolve_app_icon_data_url(app_name: &str) -> Option<String> {
    let app_bundle_path = resolve_application_bundle_path(app_name)?;
    let resources_dir = app_bundle_path.join("Contents").join("Resources");

    let candidates = read_info_plist_json(&app_bundle_path)
        .map(|plist| extract_icon_candidates_from_plist_json(&plist))
        .unwrap_or_else(Vec::new);
    let icns_path = find_icns_path(&resources_dir, &candidates)?;
    convert_icns_to_png_data_url(&icns_path)
}

#[cfg(not(target_os = "macos"))]
pub(in crate::app_store) fn resolve_app_icon_data_url(_app_name: &str) -> Option<String> {
    None
}

pub(in crate::app_store) fn try_open_application(app_name: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if Command::new("open").arg("-a").arg(app_name).spawn().is_ok() {
            return Ok(());
        }

        if let Some(path) = resolve_application_bundle_path(app_name) {
            Command::new("open")
                .arg(&path)
                .spawn()
                .map_err(|e| e.to_string())?;
            return Ok(());
        }

        Err(format!("Unable to find application named '{}'", app_name))
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", app_name])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new(app_name)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

pub(in crate::app_store) fn normalize_launcher_pin_order(items: &mut [LauncherRecord]) {
    let mut pinned_idx: Vec<usize> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| if item.pinned { Some(idx) } else { None })
        .collect();
    pinned_idx.sort_by_key(|idx| items[*idx].pin_order);
    for (order, idx) in pinned_idx.into_iter().enumerate() {
        items[idx].pin_order = order as u32;
    }
}

pub(in crate::app_store) fn sort_launcher_items(items: &mut [LauncherRecord]) {
    normalize_launcher_pin_order(items);
    items.sort_by(|a, b| {
        if a.pinned != b.pinned {
            return if a.pinned {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if a.pinned && b.pinned {
            return a.pin_order.cmp(&b.pin_order);
        }
        b.last_launched_at
            .unwrap_or(0)
            .cmp(&a.last_launched_at.unwrap_or(0))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

pub(in crate::app_store) fn next_launcher_pin_order(items: &[LauncherRecord]) -> u32 {
    items
        .iter()
        .filter(|item| item.pinned)
        .map(|item| item.pin_order)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

pub(in crate::app_store) fn merge_launcher_items(
    existing: &mut Vec<LauncherRecord>,
    imported: Vec<LauncherRecord>,
) {
    for incoming in imported {
        if let Some(idx) = existing.iter().position(|it| it.id == incoming.id) {
            existing[idx] = incoming;
        } else {
            existing.push(incoming);
        }
    }
}

pub(in crate::app_store) fn launcher_record_from_import_input(
    input: LauncherItemInput,
    now: u64,
) -> Result<LauncherRecord, String> {
    let mut record = LauncherRecord {
        id: input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name,
        item_type: input.item_type,
        target: input.target,
        pinned: input.pinned.unwrap_or(false),
        pin_order: input.pin_order.unwrap_or(0),
        launch_count: input.launch_count.unwrap_or(0),
        last_launched_at: input.last_launched_at,
        trusted: input.trusted.unwrap_or(false),
        created_at: input.created_at.unwrap_or(now),
        updated_at: input.updated_at.unwrap_or(now),
    };
    sanitize_launcher_record(&mut record)?;
    Ok(record)
}
