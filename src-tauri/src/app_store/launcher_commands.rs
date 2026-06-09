use super::{
    api_error, api_ok, get_meta, is_valid_launcher_type, launcher_record_from_import_input,
    launcher_to_legacy, load_launcher_state, merge_launcher_items, next_launcher_pin_order,
    normalize_app_target, normalize_launcher_pin_order, now_ts, resolve_app_icon_data_url,
    run_migration_impl, sanitize_launcher_record, save_launcher_state, sort_launcher_items,
    try_open_application, ApiErr, ApiMeta, ApiOk, LauncherItemInput, LauncherRecord, StorageEngine,
    LAUNCHER_EXPORT_VERSION,
};
use serde_json::{json, Value};
use std::fs::{self};
use std::path::Path;
use std::process::Command;

#[tauri::command]
pub fn launcher_list() -> Result<ApiOk<Vec<Value>>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    sort_launcher_items(&mut state.items);
    api_ok(
        state.items.iter().map(launcher_to_legacy).collect(),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn launcher_upsert(_app: tauri::AppHandle, item: Value) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let obj = item
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "launcher item must be object"))?;

    let req_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let req_name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let req_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_type").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let req_target = obj
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("command").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let req_pinned = obj.get("pinned").and_then(|v| v.as_bool());
    let req_pin_order = obj
        .get("pin_order")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let req_trusted = obj.get("trusted").and_then(|v| v.as_bool());

    let now = now_ts();
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let item_id = req_id
        .clone()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let existing = state.items.iter().find(|it| it.id == item_id).cloned();

    let mut record = LauncherRecord {
        id: item_id,
        name: req_name
            .or_else(|| existing.as_ref().map(|it| it.name.clone()))
            .unwrap_or_default(),
        item_type: req_type
            .or_else(|| existing.as_ref().map(|it| it.item_type.clone()))
            .unwrap_or_default(),
        target: req_target
            .or_else(|| existing.as_ref().map(|it| it.target.clone()))
            .unwrap_or_default(),
        pinned: req_pinned
            .unwrap_or_else(|| existing.as_ref().map(|it| it.pinned).unwrap_or(false)),
        pin_order: req_pin_order
            .unwrap_or_else(|| existing.as_ref().map(|it| it.pin_order).unwrap_or(0)),
        launch_count: existing.as_ref().map(|it| it.launch_count).unwrap_or(0),
        last_launched_at: existing.as_ref().and_then(|it| it.last_launched_at),
        trusted: req_trusted
            .unwrap_or_else(|| existing.as_ref().map(|it| it.trusted).unwrap_or(false)),
        created_at: existing.as_ref().map(|it| it.created_at).unwrap_or(now),
        updated_at: now,
    };
    if let Err(err) = sanitize_launcher_record(&mut record) {
        if let Some(old) = &existing {
            if record.name.trim().is_empty() {
                record.name = old.name.clone();
            }
            if record.target.trim().is_empty() {
                record.target = old.target.clone();
            }
            if !is_valid_launcher_type(&record.item_type) {
                record.item_type = old.item_type.clone();
            }
            sanitize_launcher_record(&mut record).map_err(|e| api_error("invalid_payload", e))?;
        } else {
            return Err(api_error("invalid_payload", err));
        }
    }

    if record.pinned {
        let was_pinned = existing.as_ref().map(|it| it.pinned).unwrap_or(false);
        if !was_pinned && req_pin_order.is_none() {
            record.pin_order = next_launcher_pin_order(&state.items);
        }
    } else {
        record.pin_order = 0;
    }

    if let Some(pos) = state.items.iter().position(|it| it.id == record.id) {
        state.items[pos] = record.clone();
    } else {
        state.items.push(record.clone());
    }

    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        launcher_to_legacy(&record),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_delete(
    _app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    state.items.retain(|it| it.id != item_id);
    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "deleted": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_reorder(
    _app: tauri::AppHandle,
    ids: Vec<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;

    let mut ordered_ids: Vec<String> = ids
        .into_iter()
        .filter(|id| state.items.iter().any(|it| it.id == *id && it.pinned))
        .collect();
    let current_pinned: Vec<String> = state
        .items
        .iter()
        .filter(|it| it.pinned)
        .map(|it| it.id.clone())
        .collect();
    for id in current_pinned {
        if !ordered_ids.iter().any(|x| x == &id) {
            ordered_ids.push(id);
        }
    }

    for item in state.items.iter_mut() {
        if !item.pinned {
            continue;
        }
        if let Some(pos) = ordered_ids.iter().position(|id| id == &item.id) {
            item.pin_order = pos as u32;
            item.updated_at = now_ts();
        }
    }

    normalize_launcher_pin_order(&mut state.items);
    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "reordered": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_mark_launched(
    _app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let now = now_ts();
    let mut found = false;
    for item in state.items.iter_mut() {
        if item.id == item_id {
            item.launch_count = item.launch_count.saturating_add(1);
            item.last_launched_at = Some(now);
            item.updated_at = now;
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "launcher item not found"));
    }

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "launched": true }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub async fn launcher_set_trust(
    _app: tauri::AppHandle,
    payload: Value,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_id = obj
        .get("itemId")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if item_id.is_empty() {
        return Err(api_error("invalid_payload", "itemId required"));
    }
    let trusted = obj
        .get("trusted")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| api_error("invalid_payload", "trusted bool required"))?;
    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let mut found = false;
    for item in state.items.iter_mut() {
        if item.id == item_id {
            if item.item_type != "script" {
                return Err(api_error(
                    "invalid_payload",
                    "only script item supports trust switch",
                ));
            }
            item.trusted = trusted;
            item.updated_at = now_ts();
            found = true;
            break;
        }
    }

    if !found {
        return Err(api_error("not_found", "launcher item not found"));
    }

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({ "trusted": trusted }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn launcher_export(
    output_path: String,
    item_ids: Option<Vec<String>>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }
    let state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let selected_ids = item_ids.unwrap_or_default();
    let mut exported: Vec<LauncherRecord> = state
        .items
        .iter()
        .filter(|item| selected_ids.is_empty() || selected_ids.iter().any(|id| id == &item.id))
        .cloned()
        .collect();
    sort_launcher_items(&mut exported);

    let payload = json!({
        "version": LAUNCHER_EXPORT_VERSION,
        "exported_at": now_ts(),
        "items": exported,
    });

    let content = serde_json::to_string_pretty(&payload)
        .map_err(|e| api_error("serialize_error", e.to_string()))?;
    StorageEngine::atomic_write(Path::new(&output_path), &content)
        .map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({
            "path": output_path,
            "count": payload
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0)
        }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub async fn launcher_import(
    _app: tauri::AppHandle,
    import_path: String,
    mode: Option<String>,
) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let raw = fs::read_to_string(&import_path).map_err(|e| api_error("io_error", e.to_string()))?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|e| api_error("invalid_payload", e.to_string()))?;
    let items_val = parsed
        .get("items")
        .and_then(|v| v.as_array().cloned())
        .or_else(|| parsed.as_array().cloned())
        .ok_or_else(|| api_error("invalid_payload", "import payload must contain items array"))?;

    let now = now_ts();
    let mut imported_records: Vec<LauncherRecord> = Vec::new();
    for item in items_val {
        let input: LauncherItemInput = serde_json::from_value(item)
            .map_err(|e| api_error("invalid_payload", format!("invalid launcher item: {}", e)))?;
        let mut record = launcher_record_from_import_input(input, now)
            .map_err(|e| api_error("invalid_payload", e))?;
        record.updated_at = now;
        imported_records.push(record);
    }
    let imported_count = imported_records.len();

    let mut state = load_launcher_state().map_err(|e| api_error("io_error", e))?;
    let mode = mode.unwrap_or_else(|| "merge".to_string()).to_lowercase();
    if mode == "replace" {
        state.items = imported_records;
    } else {
        merge_launcher_items(&mut state.items, imported_records);
    }
    normalize_launcher_pin_order(&mut state.items);

    let schema = save_launcher_state(&state).map_err(|e| api_error("io_error", e))?;

    api_ok(
        json!({
            "imported": true,
            "mode": mode,
            "count": imported_count,
            "total": state.items.len()
        }),
        ApiMeta {
            schema_version: schema.schema_version,
            revision: schema.revision,
        },
    )
}

#[tauri::command]
pub fn launcher_execute(payload: Value) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let obj = payload
        .as_object()
        .cloned()
        .ok_or_else(|| api_error("invalid_payload", "payload must be object"))?;
    let item_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("item_type").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let target = obj
        .get("target")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("command").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();

    if target.is_empty() {
        return Err(api_error("invalid_payload", "launcher target required"));
    }
    if !is_valid_launcher_type(&item_type) || item_type == "internal" {
        return Err(api_error(
            "invalid_payload",
            "unsupported launcher type for execute",
        ));
    }

    let run_result: Result<(), String> = match item_type.as_str() {
        "url" | "folder" => crate::open_path_with_system(&target),
        "app" => match normalize_app_target(&target) {
            Ok(app_name) => try_open_application(&app_name),
            Err(e) => Err(e),
        },
        "script" => Command::new("sh")
            .arg("-c")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Err("unsupported launcher type".to_string()),
    };

    run_result.map_err(|e| api_error("launch_failed", e))?;
    api_ok(
        json!({ "launched": true }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}

#[tauri::command]
pub fn launcher_resolve_app_icon(target: String) -> Result<ApiOk<Value>, ApiErr> {
    if let Err(e) = run_migration_impl() {
        return Err(api_error("migration_failed", e));
    }

    let normalized_target =
        normalize_app_target(&target).unwrap_or_else(|_| target.trim().to_string());
    let data_url = resolve_app_icon_data_url(&normalized_target);

    api_ok(
        json!({ "data_url": data_url }),
        get_meta().map_err(|e| api_error("io_error", e))?,
    )
}
