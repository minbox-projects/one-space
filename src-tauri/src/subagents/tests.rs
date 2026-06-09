#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    fn test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_temp_home<T>(label: &str, f: impl FnOnce(&Path) -> T) -> T {
        let _guard = test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home_raw = std::env::temp_dir().join(format!(
            "onespace-subagents-test-{}-{}-{}",
            label,
            std::process::id(),
            now_ts()
        ));
        fs::create_dir_all(&temp_home_raw).expect("create temp home");
        let temp_home = fs::canonicalize(&temp_home_raw).expect("canonical temp home");
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &temp_home);
        let result = f(&temp_home);
        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&temp_home_raw);
        result
    }

    fn write_subagent_dir(dir: &Path, frontmatter_name: &str, title: &str, description: &str) {
        fs::create_dir_all(dir).expect("create subagent dir");
        let markdown = format!(
            "---\nname: {}\ndescription: {}\nmodels: [codex]\n---\n# {}\n\n{}\n",
            frontmatter_name, description, title, description
        );
        fs::write(dir.join("AGENT.md"), markdown).expect("write subagent markdown");
    }

    #[test]
    fn hash_dir_ignores_file_mtime_when_content_is_unchanged() {
        let unique = format!(
            "onespace-subagents-hash-{}-{}",
            std::process::id(),
            now_ts()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("create temp test dir");
        let subagent_md = root.join("AGENT.md");
        fs::write(&subagent_md, "hello\nworld\n").expect("write initial content");

        let before = hash_dir(&root).expect("hash before");
        std::thread::sleep(Duration::from_millis(1200));
        fs::write(&subagent_md, "hello\nworld\n").expect("rewrite same content");
        let after = hash_dir(&root).expect("hash after");

        assert_eq!(before, after);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn current_installed_subagents_scans_project_scope_without_local_state_records() {
        with_temp_home("project-scan", |home| {
            let project_root = home.join("project");
            fs::create_dir_all(&project_root).expect("create project root");
            let installed_dir = project_primary_dir("codex", &project_root)
                .expect("project primary dir")
                .join("code-reviewer");
            write_subagent_dir(
                &installed_dir,
                "code-reviewer",
                "Code Reviewer",
                "Project copy",
            );

            let local_hash = hash_dir(&installed_dir).expect("project hash");
            let project_root_value = fs::canonicalize(&project_root)
                .expect("canonical project root")
                .to_string_lossy()
                .to_string();
            let records = current_installed_subagents(
                &SubagentsLocalState::default(),
                &SubagentsSyncState {
                    status: "done".to_string(),
                    last_error: None,
                    last_sync_at: Some(1),
                    sources: vec![],
                    catalog: vec![CatalogSubagent {
                        source_id: "official".to_string(),
                        id: "official-code-reviewer".to_string(),
                        rel_path: "automation/code-reviewer".to_string(),
                        dir_name: "code-reviewer".to_string(),
                        name: "Code Reviewer".to_string(),
                        description: "remote copy".to_string(),
                        models: vec!["codex".to_string()],
                        model: Some("sonnet".to_string()),
                        tools: vec!["Read".to_string()],
                        remote_hash: local_hash,
                        icon_seed: "official".to_string(),
                        first_seen_at: Some(1),
                    }],
                },
                &StorageConfig::default(),
                Some("codex"),
                INSTALL_SCOPE_PROJECT,
                Some(project_root_value.as_str()),
            )
            .expect("scan project installed subagents");

            assert_eq!(records.len(), 1);
            let subagent = &records[0];
            assert_eq!(subagent.id, "official-code-reviewer");
            assert_eq!(subagent.source_id, "official");
            assert_eq!(subagent.source_rel_path, "automation/code-reviewer");
            assert_eq!(subagent.scope, INSTALL_SCOPE_PROJECT);
            assert_eq!(
                subagent.project_root.as_deref(),
                Some(project_root_value.as_str())
            );
            assert_eq!(
                subagent.target_path.as_deref(),
                Some(installed_dir.to_string_lossy().as_ref())
            );
        });
    }

    #[test]
    fn parse_subagent_md_prefers_frontmatter_for_name_and_description() {
        let md = r#"---
name: Frontmatter Subagent
description: Description from frontmatter
models: [codex]
---
# Header Name
Paragraph description.
"#;
        let (name, description, models) = parse_subagent_md(md, &[]);
        assert_eq!(name, "Frontmatter Subagent");
        assert_eq!(description, "Description from frontmatter");
        assert_eq!(models, vec!["codex".to_string()]);
    }

    #[test]
    fn parse_subagent_frontmatter_meta_reads_model_and_tools_from_top_block() {
        let md = r#"---
name: api-designer
description: API design helper
tools: Read, Write, Edit, Bash, Glob, Grep
model: sonnet
---
# Should not affect frontmatter meta
"#;
        let (model, tools) = parse_subagent_frontmatter_meta(md);
        assert_eq!(model.as_deref(), Some("sonnet"));
        assert_eq!(
            tools,
            vec![
                "Read".to_string(),
                "Write".to_string(),
                "Edit".to_string(),
                "Bash".to_string(),
                "Glob".to_string(),
                "Grep".to_string(),
            ]
        );
    }

    #[test]
    fn parse_required_subagent_dir_name_accepts_frontmatter_name() {
        let md = r#"---
name: git-commit
description: Description from frontmatter
models: [gemini]
---
First line.
Second line.
"#;
        let dir_name = parse_required_subagent_dir_name(md).expect("should parse dir name");
        assert_eq!(dir_name, "git-commit");
    }

    #[test]
    fn parse_required_subagent_dir_name_rejects_missing_frontmatter_name() {
        let md = r#"---
models: [gemini]
---
# Header Name
First line.
"#;
        let err = parse_required_subagent_dir_name(md).expect_err("missing name should fail");
        assert_eq!(err, "subagents/invalid_frontmatter_name");
    }

    #[test]
    fn parse_required_subagent_dir_name_rejects_invalid_name() {
        let md = r#"---
name: Git Commit
description: desc
---
# Header Name
"#;
        let err = parse_required_subagent_dir_name(md).expect_err("invalid name should fail");
        assert_eq!(err, "subagents/invalid_frontmatter_name");
    }

    #[test]
    fn replace_source_entry_atomic_materializes_markdown_file() {
        let unique = format!(
            "onespace-subagents-file-entry-{}-{}",
            std::process::id(),
            now_ts()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("create temp root");
        let src_md = root.join("single.md");
        let content = r#"---
name: single-agent
models: [codex]
---
# Single Agent
Description.
"#;
        fs::write(&src_md, content).expect("write source markdown");

        let dst = root.join("installed").join("single-agent");
        replace_source_entry_atomic(&src_md, &dst).expect("materialize markdown file");
        let generated = fs::read_to_string(dst.join("AGENT.md")).expect("read generated AGENT.md");
        assert_eq!(generated, content);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_source_catalog_includes_markdown_file_entries() {
        let unique = format!(
            "onespace-subagents-scan-md-{}-{}",
            std::process::id(),
            now_ts()
        );
        let repo_root = std::env::temp_dir().join(unique);
        let categories = repo_root.join("categories").join("automation");
        fs::create_dir_all(&categories).expect("create categories dir");
        fs::write(
            categories.join("reviewer.md"),
            r#"---
name: reviewer-agent
models: [claude, codex]
description: review helper
---
# Reviewer Agent
Review markdown.
"#,
        )
        .expect("write markdown subagent");

        let source = SubagentSourceConfig {
            id: "awesome".to_string(),
            name: "Awesome".to_string(),
            repo_url: "https://example.com/repo.git".to_string(),
            branch: Some("main".to_string()),
            base_dir: Some("/categories".to_string()),
            enabled: true,
            default_models: vec!["claude".to_string()],
        };

        let catalog = scan_source_catalog(&repo_root, &source).expect("scan source catalog");
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].rel_path, "automation/reviewer.md");
        assert_eq!(catalog[0].dir_name, "reviewer-agent");
        assert!(catalog[0].models.contains(&"claude".to_string()));
        assert!(catalog[0].models.contains(&"codex".to_string()));
        let _ = fs::remove_dir_all(&repo_root);
    }

    #[test]
    fn has_dir_name_conflict_detects_same_model_only() {
        let state = SubagentsLocalState {
            subagents: vec![
                SubagentRecord {
                    id: "legacy-1".to_string(),
                    dir_name: "git-commit".to_string(),
                    model: "codex".to_string(),
                    models: vec![],
                    name: "n".to_string(),
                    description: "d".to_string(),
                    source_id: "local".to_string(),
                    source_rel_path: "a".to_string(),
                    installed_at: 0,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: "".to_string(),
                    remote_hash: None,
                    has_update: false,
                    icon_seed: "".to_string(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: None,
                },
                SubagentRecord {
                    id: "legacy-2".to_string(),
                    dir_name: "git-commit".to_string(),
                    model: "claude".to_string(),
                    models: vec![],
                    name: "n".to_string(),
                    description: "d".to_string(),
                    source_id: "local".to_string(),
                    source_rel_path: "b".to_string(),
                    installed_at: 0,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: "".to_string(),
                    remote_hash: None,
                    has_update: false,
                    icon_seed: "".to_string(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: None,
                },
            ],
            revision: 0,
            last_rescan_at: None,
        };

        assert!(has_dir_name_conflict(
            &state,
            "codex",
            INSTALL_SCOPE_GLOBAL,
            None,
            "git-commit",
            Some("other-id"),
        ));
        assert!(!has_dir_name_conflict(
            &state,
            "gemini",
            INSTALL_SCOPE_GLOBAL,
            None,
            "git-commit",
            Some("other-id"),
        ));
        assert!(!has_dir_name_conflict(
            &state,
            "codex",
            INSTALL_SCOPE_GLOBAL,
            None,
            "git-commit",
            Some("legacy-1"),
        ));
    }

    #[test]
    fn hydrate_local_records_from_catalog_recovers_remote_metadata() {
        let mut state = SubagentsLocalState {
            subagents: vec![SubagentRecord {
                id: "git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                model: "codex".to_string(),
                models: vec!["codex".to_string()],
                name: "Git Commit".to_string(),
                description: "local copy".to_string(),
                source_id: "local".to_string(),
                source_rel_path: "git-commit".to_string(),
                installed_at: 0,
                updated_at: None,
                last_synced_at: None,
                local_hash: "same-hash".to_string(),
                remote_hash: None,
                has_update: false,
                icon_seed: "git-commit".to_string(),
                scope: INSTALL_SCOPE_GLOBAL.to_string(),
                project_root: None,
                target_path: None,
            }],
            revision: 0,
            last_rescan_at: None,
        };
        let sync_state = SubagentsSyncState {
            status: "done".to_string(),
            last_error: None,
            last_sync_at: Some(1),
            sources: vec![],
            catalog: vec![CatalogSubagent {
                source_id: "official".to_string(),
                id: "official-git-commit".to_string(),
                rel_path: "automation/git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                name: "Git Commit".to_string(),
                description: "remote copy".to_string(),
                models: vec!["codex".to_string()],
                model: Some("sonnet".to_string()),
                tools: vec!["Read".to_string(), "Edit".to_string()],
                remote_hash: "same-hash".to_string(),
                icon_seed: "official".to_string(),
                first_seen_at: Some(1),
            }],
        };

        hydrate_local_records_from_catalog(&mut state, &sync_state);

        let subagent = &state.subagents[0];
        assert_eq!(subagent.id, "official-git-commit");
        assert_eq!(subagent.source_id, "official");
        assert_eq!(subagent.source_rel_path, "automation/git-commit");
        assert_eq!(subagent.remote_hash.as_deref(), Some("same-hash"));
        assert!(!subagent.has_update);
        assert_eq!(subagent.icon_seed, "official");
    }

    #[test]
    fn normalize_repositories_dedupes_same_repo_key_records() {
        let mut state = SubagentsState {
            subagents: vec![],
            repositories: vec![
                RepositoryRecord {
                    repo_key: "official::api-designer".to_string(),
                    subagent_id: "official-api-designer".to_string(),
                    dir_name: "api-designer".to_string(),
                    source_id: "official".to_string(),
                    source_rel_path: "api-designer".to_string(),
                    source_type: "remote".to_string(),
                    source_path: None,
                    name: "API Designer".to_string(),
                    description: "older".to_string(),
                    models: vec!["codex".to_string()],
                    model: None,
                    tools: vec![],
                    icon_seed: "official".to_string(),
                    hash: Some("hash-a".to_string()),
                    created_at: 10,
                    updated_at: Some(20),
                    ever_installed: false,
                },
                RepositoryRecord {
                    repo_key: "official::api-designer".to_string(),
                    subagent_id: "official-api-designer".to_string(),
                    dir_name: "api-designer".to_string(),
                    source_id: "official".to_string(),
                    source_rel_path: "api-designer".to_string(),
                    source_type: "remote".to_string(),
                    source_path: Some("/tmp/agent".to_string()),
                    name: "API Designer".to_string(),
                    description: "newer".to_string(),
                    models: vec!["codex".to_string(), "claude".to_string()],
                    model: Some("sonnet".to_string()),
                    tools: vec!["Read".to_string()],
                    icon_seed: "official".to_string(),
                    hash: Some("hash-b".to_string()),
                    created_at: 12,
                    updated_at: Some(50),
                    ever_installed: true,
                },
            ],
            revision: 0,
            last_rescan_at: None,
            last_sync_at: None,
            errors: vec![],
        };

        let changed = normalize_repositories(&mut state);
        assert!(changed);
        assert_eq!(state.repositories.len(), 1);
        let repo = &state.repositories[0];
        assert_eq!(repo.repo_key, "official::api-designer");
        assert_eq!(repo.created_at, 10);
        assert_eq!(repo.updated_at, Some(50));
        assert!(repo.ever_installed);
    }

    #[test]
    fn normalize_repositories_removes_transient_local_mirror_records() {
        let mut state = SubagentsState {
            subagents: vec![],
            repositories: vec![
                RepositoryRecord {
                    repo_key: "local::api-designer".to_string(),
                    subagent_id: "local-api-designer".to_string(),
                    dir_name: "api-designer".to_string(),
                    source_id: "local".to_string(),
                    source_rel_path: "api-designer".to_string(),
                    source_type: "local_import".to_string(),
                    source_path: Some("/tmp/model/api-designer".to_string()),
                    name: "API Designer".to_string(),
                    description: "mirror transient".to_string(),
                    models: vec!["codex".to_string()],
                    model: None,
                    tools: vec![],
                    icon_seed: "local".to_string(),
                    hash: Some("hash-local".to_string()),
                    created_at: 1,
                    updated_at: Some(2),
                    ever_installed: true,
                },
                RepositoryRecord {
                    repo_key: "official::api-designer".to_string(),
                    subagent_id: "official-api-designer".to_string(),
                    dir_name: "api-designer".to_string(),
                    source_id: "official".to_string(),
                    source_rel_path: "api-designer".to_string(),
                    source_type: "remote".to_string(),
                    source_path: None,
                    name: "API Designer".to_string(),
                    description: "official".to_string(),
                    models: vec!["codex".to_string()],
                    model: None,
                    tools: vec![],
                    icon_seed: "official".to_string(),
                    hash: Some("hash-remote".to_string()),
                    created_at: 3,
                    updated_at: Some(4),
                    ever_installed: true,
                },
            ],
            revision: 0,
            last_rescan_at: None,
            last_sync_at: None,
            errors: vec![],
        };

        let changed = normalize_repositories(&mut state);
        assert!(changed);
        assert_eq!(state.repositories.len(), 1);
        assert_eq!(state.repositories[0].repo_key, "official::api-designer");
        assert_eq!(state.repositories[0].source_id, "official");
    }
}
