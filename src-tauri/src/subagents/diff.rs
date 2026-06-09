fn subagent_has_markdown_update(subagent: &SubagentRecord, cfg: &StorageConfig) -> Option<bool> {
    let local = record_local_dir(subagent).ok()?.join("AGENT.md");
    let local_md = crate::managed_assets::read_markdown_for_compare(&local)?;
    let source = get_source(cfg, &subagent.source_id)?;
    let remote_entry = source_subagent_abs_path(source, &subagent.source_rel_path).ok()?;
    let remote_md = read_markdown_from_source_entry(&remote_entry)
        .ok()
        .map(crate::managed_assets::normalize_text_content)?;
    Some(local_md != remote_md)
}

fn calculate_changes(
    local_md: &str,
    remote_md: &str,
) -> (Vec<u32>, Vec<u32>, Vec<DiffBlock>, Vec<DiffBlock>) {
    let (before_lines, after_lines, before_blocks, after_blocks) =
        crate::managed_assets::calculate_changes(local_md, remote_md);
    (
        before_lines,
        after_lines,
        before_blocks
            .into_iter()
            .map(|block| DiffBlock {
                start_line: block.start_line,
                end_line: block.end_line,
                content: block.content,
            })
            .collect(),
        after_blocks
            .into_iter()
            .map(|block| DiffBlock {
                start_line: block.start_line,
                end_line: block.end_line,
                content: block.content,
            })
            .collect(),
    )
}

fn collect_file_map(root: &Path) -> Result<HashMap<String, PathBuf>, String> {
    let mut rel_files = vec![];
    if !root.exists() {
        return Ok(HashMap::new());
    }
    collect_files(root, root, &mut rel_files)?;
    let mut out = HashMap::new();
    for rel in rel_files {
        let normalized = normalize_rel_path(&rel);
        out.insert(normalized, root.join(&rel));
    }
    Ok(out)
}

fn compare_snapshot_dirs(
    before_dir: Option<&Path>,
    after_dir: &Path,
) -> Result<(Vec<ReloadChangedFile>, Vec<ReloadTextDiff>), String> {
    let before = if let Some(dir) = before_dir {
        collect_file_map(dir)?
    } else {
        HashMap::new()
    };
    let after = collect_file_map(after_dir)?;

    let (changed_files, text_diffs) = crate::managed_assets::compare_snapshot_file_maps(before, after)?;
    Ok((
        changed_files
            .into_iter()
            .map(|file| ReloadChangedFile {
                path: file.path,
                status: file.status,
                is_binary: file.is_binary,
            })
            .collect(),
        text_diffs
            .into_iter()
            .map(|diff| ReloadTextDiff {
                path: diff.path,
                before_content: diff.before_content,
                after_content: diff.after_content,
                before_changed_lines: diff.before_changed_lines,
                after_changed_lines: diff.after_changed_lines,
            })
            .collect(),
    ))
}
