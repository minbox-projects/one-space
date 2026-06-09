use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct DiffBlockData {
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ChangedFileData {
    pub path: String,
    pub status: String,
    pub is_binary: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TextDiffData {
    pub path: String,
    pub before_content: String,
    pub after_content: String,
    pub before_changed_lines: Vec<u32>,
    pub after_changed_lines: Vec<u32>,
}

pub(crate) fn normalize_text_content(content: String) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn read_markdown_for_compare(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(normalize_text_content)
}

fn lines_to_blocks(lines: &[u32], content: &str) -> Vec<DiffBlockData> {
    if lines.is_empty() {
        return vec![];
    }
    let all_lines: Vec<&str> = content.lines().collect();
    let mut blocks = vec![];
    let mut start = lines[0];
    let mut prev = lines[0];

    for &line in lines.iter().skip(1) {
        if line == prev + 1 {
            prev = line;
            continue;
        }
        let slice = (start..=prev)
            .filter_map(|ln| all_lines.get((ln.saturating_sub(1)) as usize).copied())
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(DiffBlockData {
            start_line: start,
            end_line: prev,
            content: slice,
        });
        start = line;
        prev = line;
    }

    let slice = (start..=prev)
        .filter_map(|ln| all_lines.get((ln.saturating_sub(1)) as usize).copied())
        .collect::<Vec<_>>()
        .join("\n");
    blocks.push(DiffBlockData {
        start_line: start,
        end_line: prev,
        content: slice,
    });
    blocks
}

pub(crate) fn calculate_changes(
    local_md: &str,
    remote_md: &str,
) -> (Vec<u32>, Vec<u32>, Vec<DiffBlockData>, Vec<DiffBlockData>) {
    let left: Vec<&str> = local_md.lines().collect();
    let right: Vec<&str> = remote_md.lines().collect();
    let max_len = left.len().max(right.len());
    let mut l_changed = vec![];
    let mut r_changed = vec![];
    for i in 0..max_len {
        let l = left.get(i).copied().unwrap_or("");
        let r = right.get(i).copied().unwrap_or("");
        if l != r {
            if i < left.len() {
                l_changed.push((i + 1) as u32);
            }
            if i < right.len() {
                r_changed.push((i + 1) as u32);
            }
        }
    }
    let l_blocks = lines_to_blocks(&l_changed, local_md);
    let r_blocks = lines_to_blocks(&r_changed, remote_md);
    (l_changed, r_changed, l_blocks, r_blocks)
}

fn read_text_file_for_diff(path: &Path) -> Result<Option<String>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.contains(&0) {
        return Ok(None);
    }
    match String::from_utf8(bytes) {
        Ok(content) => Ok(Some(normalize_text_content(content))),
        Err(_) => Ok(None),
    }
}

pub(crate) fn compare_snapshot_file_maps(
    before: HashMap<String, PathBuf>,
    after: HashMap<String, PathBuf>,
) -> Result<(Vec<ChangedFileData>, Vec<TextDiffData>), String> {
    let mut keys = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();

    let mut changed_files = vec![];
    let mut text_diffs = vec![];
    for rel in keys {
        let before_path = before.get(&rel);
        let after_path = after.get(&rel);
        let status = match (before_path, after_path) {
            (Some(_), None) => Some("deleted"),
            (None, Some(_)) => Some("added"),
            (Some(b), Some(a)) => {
                let b_content = fs::read(b).map_err(|e| e.to_string())?;
                let a_content = fs::read(a).map_err(|e| e.to_string())?;
                if b_content == a_content {
                    None
                } else {
                    Some("modified")
                }
            }
            (None, None) => None,
        };

        let Some(status) = status else {
            continue;
        };

        let before_text = if let Some(path) = before_path {
            read_text_file_for_diff(path)?
        } else {
            Some(String::new())
        };
        let after_text = if let Some(path) = after_path {
            read_text_file_for_diff(path)?
        } else {
            Some(String::new())
        };
        let is_binary = before_text.is_none() || after_text.is_none();

        changed_files.push(ChangedFileData {
            path: rel.clone(),
            status: status.to_string(),
            is_binary,
        });

        if !is_binary {
            let before_content = before_text.unwrap_or_default();
            let after_content = after_text.unwrap_or_default();
            let (before_changed_lines, after_changed_lines, _, _) =
                calculate_changes(&before_content, &after_content);
            text_diffs.push(TextDiffData {
                path: rel.clone(),
                before_content,
                after_content,
                before_changed_lines,
                after_changed_lines,
            });
        }
    }

    Ok((changed_files, text_diffs))
}
