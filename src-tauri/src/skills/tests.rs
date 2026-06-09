use super::*;
use crate::config::{SkillSourceConfig, StorageConfig};
use std::fs;
use std::path::Path;
use std::time::Duration;

fn with_temp_home<T>(label: &str, f: impl FnOnce(&Path) -> T) -> T {
    let _guard = crate::lock_test_home_env();
    let temp_home_raw = std::env::temp_dir().join(format!(
        "onespace-skills-test-{}-{}-{}",
        label,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&temp_home_raw).expect("create temp home");
    let temp_home = fs::canonicalize(&temp_home_raw).expect("canonical temp home");
    let previous_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &temp_home);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&temp_home)));
    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(&temp_home_raw);
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn write_skill_dir(dir: &Path, frontmatter_name: &str, title: &str, description: &str) {
    fs::create_dir_all(dir).expect("create skill dir");
    let markdown = format!(
        "---\nname: {}\ndescription: {}\nmodels: [codex]\n---\n# {}\n\n{}\n",
        frontmatter_name, description, title, description
    );
    fs::write(dir.join("SKILL.md"), markdown).expect("write skill markdown");
}

#[test]
fn current_installed_skills_scans_project_scope_without_local_state_records() {
    with_temp_home("project-scan", |home| {
        let project_root = home.join("project");
        fs::create_dir_all(&project_root).expect("create project root");
        let installed_dir = project_primary_dir("codex", &project_root)
            .expect("project primary dir")
            .join("git-commit");
        write_skill_dir(&installed_dir, "git-commit", "Git Commit", "Project copy");

        let local_hash = hash_dir(&installed_dir).expect("project hash");
        let project_root_value = fs::canonicalize(&project_root)
            .expect("canonical project root")
            .to_string_lossy()
            .to_string();
        let records = current_installed_skills(
            &SkillsLocalState::default(),
            &SkillsSyncState {
                status: "done".to_string(),
                last_error: None,
                last_sync_at: Some(1),
                sources: vec![],
                catalog: vec![CatalogSkill {
                    source_id: "official".to_string(),
                    id: "official-git-commit".to_string(),
                    rel_path: "automation/git-commit".to_string(),
                    dir_name: "git-commit".to_string(),
                    name: "Git Commit".to_string(),
                    description: "remote copy".to_string(),
                    models: vec!["codex".to_string()],
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
        .expect("scan project installed skills");

        assert_eq!(records.len(), 1);
        let skill = &records[0];
        assert_eq!(skill.id, "official-git-commit");
        assert_eq!(skill.source_id, "official");
        assert_eq!(skill.source_rel_path, "automation/git-commit");
        assert_eq!(skill.scope, INSTALL_SCOPE_PROJECT);
        assert_eq!(
            skill.project_root.as_deref(),
            Some(project_root_value.as_str())
        );
        assert_eq!(
            skill.target_path.as_deref(),
            Some(installed_dir.to_string_lossy().as_ref())
        );
    });
}

#[test]
fn hash_dir_ignores_file_mtime_when_content_is_unchanged() {
    let unique = format!("onespace-skills-hash-{}-{}", std::process::id(), now_ts());
    let root = std::env::temp_dir().join(unique);
    fs::create_dir_all(&root).expect("create temp test dir");
    let skill_md = root.join("SKILL.md");
    fs::write(&skill_md, "hello\nworld\n").expect("write initial content");

    let before = hash_dir(&root).expect("hash before");
    std::thread::sleep(Duration::from_millis(1200));
    fs::write(&skill_md, "hello\nworld\n").expect("rewrite same content");
    let after = hash_dir(&root).expect("hash after");

    assert_eq!(before, after);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_skill_md_prefers_title_for_name_and_frontmatter_for_description() {
    let md = r#"---
name: Frontmatter Skill
description: Description from frontmatter
models: [codex]
---
# Header Name
Paragraph description.
"#;
    let (name, description, models) = parse_skill_md(md, &[]);
    assert_eq!(name, "Header Name");
    assert_eq!(description, "Description from frontmatter");
    assert_eq!(models, vec!["codex".to_string()]);
}

#[test]
fn parse_required_skill_dir_name_accepts_frontmatter_name() {
    let md = r#"---
name: git-commit
description: Description from frontmatter
models: [gemini]
---
First line.
Second line.
"#;
    let dir_name = parse_required_skill_dir_name(md).expect("should parse dir name");
    assert_eq!(dir_name, "git-commit");
}

#[test]
fn parse_required_skill_dir_name_rejects_missing_frontmatter_name() {
    let md = r#"---
models: [gemini]
---
# Header Name
First line.
"#;
    let err = parse_required_skill_dir_name(md).expect_err("missing name should fail");
    assert_eq!(err, "skills/invalid_frontmatter_name");
}

#[test]
fn parse_required_skill_dir_name_rejects_invalid_name() {
    let md = r#"---
name: Git Commit
description: desc
---
# Header Name
"#;
    let err = parse_required_skill_dir_name(md).expect_err("invalid name should fail");
    assert_eq!(err, "skills/invalid_frontmatter_name");
}

#[test]
fn has_dir_name_conflict_detects_same_model_only() {
    let state = SkillsLocalState {
        skills: vec![
            SkillRecord {
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
            SkillRecord {
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
    let mut state = SkillsLocalState {
        skills: vec![SkillRecord {
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
    let sync_state = SkillsSyncState {
        status: "done".to_string(),
        last_error: None,
        last_sync_at: Some(1),
        sources: vec![],
        catalog: vec![CatalogSkill {
            source_id: "official".to_string(),
            id: "official-git-commit".to_string(),
            rel_path: "automation/git-commit".to_string(),
            dir_name: "git-commit".to_string(),
            name: "Git Commit".to_string(),
            description: "remote copy".to_string(),
            models: vec!["codex".to_string()],
            remote_hash: "same-hash".to_string(),
            icon_seed: "official".to_string(),
            first_seen_at: Some(1),
        }],
    };

    hydrate_local_records_from_catalog(&mut state, &sync_state);

    let skill = &state.skills[0];
    assert_eq!(skill.id, "official-git-commit");
    assert_eq!(skill.source_id, "official");
    assert_eq!(skill.source_rel_path, "automation/git-commit");
    assert_eq!(skill.remote_hash.as_deref(), Some("same-hash"));
    assert!(!skill.has_update);
    assert_eq!(skill.icon_seed, "official");
}

#[test]
fn repository_has_remote_source_update_detects_source_changes() {
    with_temp_home("remote-update", |_| {
        let source = SkillSourceConfig {
            id: "official".to_string(),
            name: "Official".to_string(),
            repo_url: "https://example.invalid/official.git".to_string(),
            branch: None,
            base_dir: None,
            enabled: true,
            default_models: vec!["codex".to_string()],
        };
        let source_dir = skills_cache_root()
            .expect("skills cache")
            .join("official")
            .join("git-commit");
        write_skill_dir(&source_dir, "git-commit", "Git Commit", "Initial");

        let repo_key = make_repo_key("official", "git-commit");
        let repo_snapshot = repo_storage_dir(&repo_key).expect("repo snapshot");
        replace_dir_atomic(&source_dir, &repo_snapshot).expect("snapshot source");
        let snapshot_hash = hash_dir(&repo_snapshot).expect("snapshot hash");
        let cfg = StorageConfig {
            skills_sources: vec![source],
            ..StorageConfig::default()
        };
        let repo = RepositoryRecord {
            repo_key,
            skill_id: "official-git-commit".to_string(),
            dir_name: "git-commit".to_string(),
            source_id: "official".to_string(),
            source_rel_path: "git-commit".to_string(),
            source_type: "remote".to_string(),
            source_path: Some(source_dir.to_string_lossy().to_string()),
            name: "Git Commit".to_string(),
            description: "Initial".to_string(),
            models: vec!["codex".to_string()],
            icon_seed: "official".to_string(),
            hash: Some(snapshot_hash),
            created_at: 1,
            updated_at: Some(1),
            ever_installed: true,
        };

        assert!(!repository_has_remote_source_update(&repo, &cfg));

        write_skill_dir(&source_dir, "git-commit", "Git Commit", "Updated");

        assert!(repository_has_remote_source_update(&repo, &cfg));
    });
}

#[test]
fn apply_repository_update_syncs_global_and_project_targets_and_moves_dir_name() {
    with_temp_home("apply-update", |home| {
        let repo_key = make_repo_key("official", "git-commit");
        let repo_snapshot = repo_storage_dir(&repo_key).expect("repo snapshot");
        write_skill_dir(&repo_snapshot, "git-commit", "Git Commit", "Old version");

        let source_dir = home.join("source").join("git-commit");
        write_skill_dir(&source_dir, "git-commit-v2", "Git Commit", "New version");

        let project_root = home.join("project");
        fs::create_dir_all(&project_root).expect("create project root");
        let project_root_value = fs::canonicalize(&project_root)
            .expect("canonical project root")
            .to_string_lossy()
            .to_string();

        let global_old_dir = model_dir("codex")
            .expect("global model dir")
            .join("git-commit");
        write_skill_dir(&global_old_dir, "git-commit", "Git Commit", "Old version");
        let project_old_dir = project_primary_dir("codex", &project_root)
            .expect("project primary")
            .join("git-commit");
        write_skill_dir(&project_old_dir, "git-commit", "Git Commit", "Old version");

        let old_repo_hash = hash_dir(&repo_snapshot).expect("old repo hash");
        let mut shared_state = SkillsState {
            repositories: vec![RepositoryRecord {
                repo_key: repo_key.clone(),
                skill_id: "official-git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                source_id: "official".to_string(),
                source_rel_path: "git-commit".to_string(),
                source_type: "remote".to_string(),
                source_path: Some(source_dir.to_string_lossy().to_string()),
                name: "Git Commit".to_string(),
                description: "Old version".to_string(),
                models: vec!["codex".to_string()],
                icon_seed: "official".to_string(),
                hash: Some(old_repo_hash),
                created_at: 1,
                updated_at: Some(1),
                ever_installed: true,
            }],
            revision: 0,
            last_rescan_at: None,
            last_sync_at: None,
            errors: vec![],
            ..SkillsState::default()
        };
        let mut local_state = SkillsLocalState {
            skills: vec![
                SkillRecord {
                    id: "official-git-commit".to_string(),
                    dir_name: "git-commit".to_string(),
                    model: "codex".to_string(),
                    models: vec!["codex".to_string()],
                    name: "Git Commit".to_string(),
                    description: "Old version".to_string(),
                    source_id: "official".to_string(),
                    source_rel_path: "git-commit".to_string(),
                    installed_at: 1,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: hash_dir(&global_old_dir).expect("global hash"),
                    remote_hash: None,
                    has_update: true,
                    icon_seed: "official".to_string(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: Some(global_old_dir.to_string_lossy().to_string()),
                },
                SkillRecord {
                    id: "official-git-commit".to_string(),
                    dir_name: "git-commit".to_string(),
                    model: "codex".to_string(),
                    models: vec!["codex".to_string()],
                    name: "Git Commit".to_string(),
                    description: "Old version".to_string(),
                    source_id: "official".to_string(),
                    source_rel_path: "git-commit".to_string(),
                    installed_at: 1,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: hash_dir(&project_old_dir).expect("project hash"),
                    remote_hash: None,
                    has_update: true,
                    icon_seed: "official".to_string(),
                    scope: INSTALL_SCOPE_PROJECT.to_string(),
                    project_root: Some(project_root_value.clone()),
                    target_path: Some(project_old_dir.to_string_lossy().to_string()),
                },
            ],
            revision: 0,
            last_rescan_at: None,
        };

        let result = apply_repository_update_from_dir(
            &mut shared_state,
            &mut local_state,
            &repo_key,
            Some(repo_snapshot.as_path()),
            &source_dir,
            true,
        )
        .expect("apply repository update");

        assert_eq!(result.synced_targets.len(), 2);
        assert_eq!(result.synced_models, vec!["codex".to_string()]);

        let new_global_dir = model_dir("codex")
            .expect("global model dir")
            .join("git-commit-v2");
        let new_project_dir = project_primary_dir("codex", &project_root)
            .expect("project primary")
            .join("git-commit-v2");
        let global_mirror_dir = home.join(".codex").join("skills").join("git-commit-v2");
        let project_compat_dir = project_root
            .join(".codex")
            .join("skills")
            .join("git-commit-v2");
        let expected_md =
            fs::read_to_string(source_dir.join("SKILL.md")).expect("expected markdown");

        assert!(!global_old_dir.exists());
        assert!(!project_old_dir.exists());
        assert_eq!(
            fs::read_to_string(new_global_dir.join("SKILL.md")).expect("global updated"),
            expected_md
        );
        assert_eq!(
            fs::read_to_string(new_project_dir.join("SKILL.md")).expect("project updated"),
            expected_md
        );
        assert_eq!(
            fs::read_to_string(global_mirror_dir.join("SKILL.md")).expect("global mirror updated"),
            expected_md
        );
        assert_eq!(
            fs::read_to_string(project_compat_dir.join("SKILL.md"))
                .expect("project compat updated"),
            expected_md
        );
        assert!(local_state.skills.iter().all(|skill| {
            skill.dir_name == "git-commit-v2" && !skill.has_update && skill.remote_hash.is_some()
        }));
    });
}

#[test]
fn apply_repository_update_rolls_back_snapshot_when_target_sync_fails() {
    with_temp_home("apply-rollback", |_| {
        let repo_key = make_repo_key("official", "git-commit");
        let repo_snapshot = repo_storage_dir(&repo_key).expect("repo snapshot");
        write_skill_dir(&repo_snapshot, "git-commit", "Git Commit", "Old version");
        let old_snapshot_md =
            fs::read_to_string(repo_snapshot.join("SKILL.md")).expect("old snapshot markdown");

        let source_dir = skills_cache_root()
            .expect("skills cache")
            .join("official")
            .join("git-commit");
        write_skill_dir(&source_dir, "shared-dir", "Git Commit", "New version");

        let installed_dir = model_dir("codex").expect("model dir").join("git-commit");
        write_skill_dir(&installed_dir, "git-commit", "Git Commit", "Old version");
        let conflicting_dir = model_dir("codex").expect("model dir").join("shared-dir");
        write_skill_dir(&conflicting_dir, "shared-dir", "Other Skill", "Conflicting");
        let conflicting_md =
            fs::read_to_string(conflicting_dir.join("SKILL.md")).expect("conflicting markdown");

        let old_repo_hash = hash_dir(&repo_snapshot).expect("old repo hash");
        let mut shared_state = SkillsState {
            repositories: vec![RepositoryRecord {
                repo_key: repo_key.clone(),
                skill_id: "official-git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                source_id: "official".to_string(),
                source_rel_path: "git-commit".to_string(),
                source_type: "remote".to_string(),
                source_path: Some(source_dir.to_string_lossy().to_string()),
                name: "Git Commit".to_string(),
                description: "Old version".to_string(),
                models: vec!["codex".to_string()],
                icon_seed: "official".to_string(),
                hash: Some(old_repo_hash),
                created_at: 1,
                updated_at: Some(1),
                ever_installed: true,
            }],
            revision: 0,
            last_rescan_at: None,
            last_sync_at: None,
            errors: vec![],
            ..SkillsState::default()
        };
        let mut local_state = SkillsLocalState {
            skills: vec![
                SkillRecord {
                    id: "official-git-commit".to_string(),
                    dir_name: "git-commit".to_string(),
                    model: "codex".to_string(),
                    models: vec!["codex".to_string()],
                    name: "Git Commit".to_string(),
                    description: "Old version".to_string(),
                    source_id: "official".to_string(),
                    source_rel_path: "git-commit".to_string(),
                    installed_at: 1,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: hash_dir(&installed_dir).expect("installed hash"),
                    remote_hash: None,
                    has_update: true,
                    icon_seed: "official".to_string(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: Some(installed_dir.to_string_lossy().to_string()),
                },
                SkillRecord {
                    id: "conflict-skill".to_string(),
                    dir_name: "shared-dir".to_string(),
                    model: "codex".to_string(),
                    models: vec!["codex".to_string()],
                    name: "Conflict".to_string(),
                    description: "Conflicting".to_string(),
                    source_id: "official".to_string(),
                    source_rel_path: "other".to_string(),
                    installed_at: 1,
                    updated_at: None,
                    last_synced_at: None,
                    local_hash: hash_dir(&conflicting_dir).expect("conflicting hash"),
                    remote_hash: None,
                    has_update: false,
                    icon_seed: "official".to_string(),
                    scope: INSTALL_SCOPE_GLOBAL.to_string(),
                    project_root: None,
                    target_path: Some(conflicting_dir.to_string_lossy().to_string()),
                },
            ],
            revision: 0,
            last_rescan_at: None,
        };

        let err = apply_repository_update_from_dir(
            &mut shared_state,
            &mut local_state,
            &repo_key,
            Some(repo_snapshot.as_path()),
            &source_dir,
            true,
        )
        .expect_err("update should fail on dir name conflict");

        assert_eq!(err, "skills/dir_name_conflict");
        assert_eq!(
            fs::read_to_string(repo_snapshot.join("SKILL.md")).expect("restored snapshot"),
            old_snapshot_md
        );
        assert_eq!(
            fs::read_to_string(installed_dir.join("SKILL.md")).expect("installed unchanged"),
            old_snapshot_md
        );
        assert_eq!(
            fs::read_to_string(conflicting_dir.join("SKILL.md"))
                .expect("conflicting skill unchanged"),
            conflicting_md
        );
    });
}

#[test]
fn skills_list_installed_clears_legacy_has_update_flags() {
    with_temp_home("installed-no-update-flag", |_| {
        let installed_dir = model_dir("codex").expect("model dir").join("git-commit");
        write_skill_dir(&installed_dir, "git-commit", "Git Commit", "Installed");

        save_local_skills_state(SkillsLocalState {
            skills: vec![SkillRecord {
                id: "official-git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                model: "codex".to_string(),
                models: vec!["codex".to_string()],
                name: "Git Commit".to_string(),
                description: "Installed".to_string(),
                source_id: "official".to_string(),
                source_rel_path: "git-commit".to_string(),
                installed_at: 1,
                updated_at: None,
                last_synced_at: None,
                local_hash: hash_dir(&installed_dir).expect("installed hash"),
                remote_hash: Some("remote-hash".to_string()),
                has_update: true,
                icon_seed: "official".to_string(),
                scope: INSTALL_SCOPE_GLOBAL.to_string(),
                project_root: None,
                target_path: Some(installed_dir.to_string_lossy().to_string()),
            }],
            revision: 0,
            last_rescan_at: None,
        })
        .expect("save local state");

        let result = skills_list_installed(
            Some("codex".to_string()),
            Some(INSTALL_SCOPE_GLOBAL.to_string()),
            None,
        )
        .expect("list installed");

        assert_eq!(result.data.len(), 1);
        assert!(!result.data[0].has_update);

        let persisted = load_local_skills_state().expect("load local state");
        assert_eq!(persisted.skills.len(), 1);
        assert!(!persisted.skills[0].has_update);
    });
}
