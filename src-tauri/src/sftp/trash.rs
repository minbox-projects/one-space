use super::paths::quote_remote_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command as StdCommand;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpDeleteResult {
    pub original_path: String,
    pub trash_path: Option<String>,
    pub is_permanent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SftpTrashItem {
    pub id: String,
    pub name: String,
    pub original_path: String,
    pub trash_path: String,
    pub size: u64,
    pub is_dir: bool,
    pub deleted_at: String,
}

#[tauri::command]
pub async fn sftp_delete_entry(
    host: String,
    remote_path: String,
    permanent: Option<bool>,
) -> Result<SftpDeleteResult, String> {
    let trimmed = remote_path.trim();
    if trimmed.is_empty()
        || trimmed == "/"
        || trimmed == "~"
        || trimmed == "$HOME"
        || trimmed == "."
        || trimmed == ".."
    {
        return Err("Refusing to delete root or home directory".to_string());
    }

    let is_permanent = permanent.unwrap_or(false);

    if is_permanent {
        let path_quoted = quote_remote_path(trimmed);
        let remote_cmd = format!("rm -rf -- {}", path_quoted);

        let output = StdCommand::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=6")
            .arg(&host)
            .arg(&remote_cmd)
            .output()
            .map_err(|e| format!("Failed to delete entry over SSH: {e}"))?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(if err_msg.is_empty() {
                format!("SSH exit code {}", output.status.code().unwrap_or(-1))
            } else {
                err_msg.to_string()
            });
        }

        return Ok(SftpDeleteResult {
            original_path: trimmed.to_string(),
            trash_path: None,
            is_permanent: true,
        });
    }

    // 默认移入回收站
    let file_name = trimmed.rsplit('/').next().unwrap_or(trimmed);
    let now_ts = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    };
    let trash_name = format!("{}_{}", now_ts, file_name);
    let trash_item_rel = format!("~/.onespace_trash/{}", trash_name);
    let trash_item_quoted = quote_remote_path(&trash_item_rel);
    let path_quoted = quote_remote_path(trimmed);
    let trash_name_quoted = quote_remote_path(&trash_name);

    let remote_cmd = format!(
        "mkdir -p \"$HOME/.onespace_trash\" 2>/dev/null && mv -- {} {} && printf '%s\\t%s\\t%s\\n' {} {} '{}' >> \"$HOME/.onespace_trash/.manifest\"",
        path_quoted, trash_item_quoted, trash_name_quoted, path_quoted, now_ts
    );

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to move file to trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    Ok(SftpDeleteResult {
        original_path: trimmed.to_string(),
        trash_path: Some(trash_item_rel),
        is_permanent: false,
    })
}

pub fn parse_trash_output(stdout: &str) -> Vec<SftpTrashItem> {
    let mut manifest_map: HashMap<String, (String, String)> = HashMap::new();
    let mut items = Vec::new();

    let parts: Vec<&str> = stdout.split("===LS===").collect();
    let manifest_part = if parts.len() > 1 {
        parts[0].replace("===MANIFEST===", "")
    } else {
        String::new()
    };
    let ls_part = if parts.len() > 1 { parts[1] } else { stdout };

    for line in manifest_part.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<&str> = trimmed.split('\t').collect();
        if fields.len() >= 2 {
            let id = fields[0].trim().to_string();
            let orig = fields[1].trim().to_string();
            let date = if fields.len() >= 3 {
                fields[2].trim().to_string()
            } else {
                "Unknown".to_string()
            };
            manifest_map.insert(id, (orig, date));
        }
    }

    for line in ls_part.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("total ") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }

        let is_dir = cols[0].starts_with('d');
        let size: u64 = cols[4].parse().unwrap_or(0);
        let (modified, file_name) = if cols.len() >= 9 {
            let mod_time = format!("{} {} {}", cols[5], cols[6], cols[7]);
            let name = cols[8..].join(" ");
            (mod_time, name)
        } else {
            ("Unknown".to_string(), cols[cols.len() - 1].to_string())
        };

        if file_name == "." || file_name == ".." || file_name.starts_with(".manifest") {
            continue;
        }

        let (orig_path, deleted_time, orig_name) = if let Some((orig, d_time)) = manifest_map.get(&file_name) {
            let name = orig.rsplit('/').next().unwrap_or(&file_name).to_string();
            (orig.clone(), d_time.clone(), name)
        } else {
            let clean_name = if let Some(idx) = file_name.find('_') {
                if file_name[..idx].chars().all(|c| c.is_ascii_digit()) {
                    file_name[idx + 1..].to_string()
                } else {
                    file_name.clone()
                }
            } else {
                file_name.clone()
            };
            (format!("~/{}", clean_name), modified.clone(), clean_name)
        };

        items.push(SftpTrashItem {
            id: file_name.clone(),
            name: orig_name,
            original_path: orig_path,
            trash_path: format!("~/.onespace_trash/{}", file_name),
            size,
            is_dir,
            deleted_at: deleted_time,
        });
    }

    items.sort_by(|a, b| b.id.cmp(&a.id));
    items
}

#[tauri::command]
pub async fn sftp_list_trash(host: String) -> Result<Vec<SftpTrashItem>, String> {
    let remote_cmd = "TRASH_DIR=\"$HOME/.onespace_trash\"; if [ -d \"$TRASH_DIR\" ]; then echo '===MANIFEST==='; cat \"$TRASH_DIR/.manifest\" 2>/dev/null; echo '===LS==='; (ls -la --time-style=long-iso \"$TRASH_DIR\" 2>/dev/null || ls -la \"$TRASH_DIR\"); fi";

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(remote_cmd)
        .output()
        .map_err(|e| format!("Failed to list trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    Ok(parse_trash_output(&stdout_str))
}

#[tauri::command]
pub async fn sftp_restore_trash_item(
    host: String,
    trash_name: String,
) -> Result<String, String> {
    let clean_id = trash_name.trim();
    if clean_id.is_empty() || clean_id.contains('/') || clean_id.contains("..") {
        return Err("Invalid trash item identifier".to_string());
    }

    let id_quoted = quote_remote_path(clean_id);
    let remote_cmd = format!(
        "TRASH_DIR=\"$HOME/.onespace_trash\"; MANIFEST=\"$TRASH_DIR/.manifest\"; TARGET_FILE=\"$TRASH_DIR/\"{}; ORIG=$(grep -F -m 1 {} \"$MANIFEST\" 2>/dev/null | cut -f2); if [ -z \"$ORIG\" ]; then CLEAN=$(echo {} | sed 's/^[0-9]*_//'); ORIG=\"$HOME/$CLEAN\"; fi; mkdir -p \"$(dirname \"$ORIG\")\" 2>/dev/null && mv -- \"$TARGET_FILE\" \"$ORIG\" && if [ -f \"$MANIFEST\" ]; then grep -v -F {} \"$MANIFEST\" > \"$MANIFEST.tmp\" 2>/dev/null && mv \"$MANIFEST.tmp\" \"$MANIFEST\"; fi; echo \"$ORIG\"",
        id_quoted, id_quoted, id_quoted, id_quoted
    );

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to restore trash item over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let restored_path = stdout_str.lines().last().unwrap_or("").trim().to_string();
    if restored_path.is_empty() {
        return Err("Failed to resolve restored path".to_string());
    }

    Ok(restored_path)
}

#[tauri::command]
pub async fn sftp_clear_trash(host: String) -> Result<(), String> {
    let remote_cmd = "TRASH_DIR=\"$HOME/.onespace_trash\"; if [ -d \"$TRASH_DIR\" ]; then rm -rf \"$TRASH_DIR\"/* \"$TRASH_DIR\"/.* 2>/dev/null; mkdir -p \"$TRASH_DIR\" 2>/dev/null; fi";

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(remote_cmd)
        .output()
        .map_err(|e| format!("Failed to clear trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn sftp_restore_entry(
    host: String,
    trash_path: String,
    original_path: String,
) -> Result<(), String> {
    let clean_trash = trash_path.trim();
    let clean_orig = original_path.trim();
    if clean_trash.is_empty() || clean_orig.is_empty() {
        return Err("Trash path and original path cannot be empty".to_string());
    }

    let trash_quoted = quote_remote_path(clean_trash);
    let orig_quoted = quote_remote_path(clean_orig);

    let remote_cmd = format!(
        "mkdir -p \"$(dirname {})\" 2>/dev/null && mv -- {} {}",
        orig_quoted, trash_quoted, orig_quoted
    );

    let output = StdCommand::new("ssh")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg("ConnectTimeout=6")
        .arg(&host)
        .arg(&remote_cmd)
        .output()
        .map_err(|e| format!("Failed to restore file from trash over SSH: {e}"))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(if err_msg.is_empty() {
            format!("SSH exit code {}", output.status.code().unwrap_or(-1))
        } else {
            err_msg.to_string()
        });
    }

    Ok(())
}
