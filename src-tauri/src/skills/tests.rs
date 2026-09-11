use super::*;
use crate::config::{SkillSourceConfig, StorageConfig};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
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

#[cfg(unix)]
#[test]
fn initialize_unified_skills_creates_claude_compatibility_symlink() {
    with_temp_home("claude-compat", |home| {
        let expected = home.join(".agents").join("skills");
        let compat = home.join(".claude").join("skills");

        initialize_unified_skills().expect("initialize skills");
        assert_eq!(fs::read_link(&compat).expect("claude compatibility link"), expected);
        initialize_unified_skills().expect("initialize skills is idempotent");
        assert_eq!(fs::read_link(&compat).expect("claude compatibility link"), expected);
    });
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
models: [antigravity]
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
models: [antigravity]
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
        "antigravity",
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

#[test]
fn antigravity_skill_paths_resolve_to_agents_and_gemini_config() {
    with_temp_home("antigravity-paths", |home| {
        let project_root = home.join("project");
        fs::create_dir_all(&project_root).expect("create project root");

        let project_dir =
            project_primary_dir("antigravity", &project_root).expect("project primary dir");
        assert_eq!(project_dir, project_root.join(".agents").join("skills"));

        let global_dir = mirror_dir("antigravity").expect("global mirror dir");
        let expected_global = home.join(".gemini").join("config").join("skills");
        assert_eq!(
            global_dir,
            fs::canonicalize(&expected_global).expect("canonical global skills dir")
        );
    });
}

// ---------------------------------------------------------------------------
// Four-tool compatibility matrix (REQ-001 / AC-001 / AC-009)
//
// Public boundary: the Skills backend reports, for every supported tool
// (claude, opencode, codex, antigravity), how that tool reads Skills relative
// to the canonical `~/.agents/skills` directory. Each result must be exactly
// one of DirectUnified, CompatibilityPath, or Unsupported, and must carry
// observable evidence. A tool that does not read `~/.agents/skills` directly
// must never be reported as directly supported (AC-009).
//
// These tests are RED against the current implementation: the compatibility
// matrix API consumed by later operations is defined by Step 1 and does not
// exist yet.
// ---------------------------------------------------------------------------

#[test]
fn compatibility_matrix_has_exactly_one_result_per_supported_tool() {
    let matrix = compatibility_matrix();

    let mut tools = matrix
        .iter()
        .map(|result| result.tool.clone())
        .collect::<Vec<_>>();
    tools.sort();
    assert_eq!(
        tools,
        vec![
            "antigravity".to_string(),
            "claude".to_string(),
            "codex".to_string(),
            "opencode".to_string(),
        ],
        "every supported tool must have exactly one compatibility result"
    );
}

#[test]
fn compatibility_results_use_only_the_three_supported_kinds() {
    for result in compatibility_matrix() {
        match &result.kind {
            CompatibilityKind::DirectUnified
            | CompatibilityKind::CompatibilityPath
            | CompatibilityKind::Unsupported => {}
        }
    }
}

#[test]
fn compatibility_results_carry_observable_evidence() {
    let matrix = compatibility_matrix();
    assert!(!matrix.is_empty(), "compatibility matrix must not be empty");
    for result in &matrix {
        assert!(
            !result.evidence.is_empty(),
            "tool {} must include observable evidence",
            result.tool
        );
        for evidence in &result.evidence {
            assert!(
                !evidence.trim().is_empty(),
                "tool {} must not include blank evidence entries",
                result.tool
            );
        }
    }
}

#[test]
fn claude_is_reported_as_compatibility_path_not_direct_unified() {
    // The frozen plan routes Claude through the ~/.claude/skills projection
    // (REQ-005 / AC-005), so Claude must never claim direct unified support.
    let claude = compatibility_matrix()
        .into_iter()
        .find(|result| result.tool == "claude")
        .expect("claude result must exist");
    assert!(
        matches!(&claude.kind, CompatibilityKind::CompatibilityPath),
        "claude must be classified as compatibility-path support"
    );
    let evidence = claude.evidence.join("\n");
    assert!(
        evidence.contains(".claude") && evidence.contains("skills"),
        "claude evidence must reference the ~/.claude/skills projection path"
    );
}

#[test]
fn direct_unified_claims_must_cite_the_unified_directory() {
    // AC-009: a tool that does not directly read ~/.agents/skills must not be
    // reported as directly supported. Any DirectUnified claim therefore needs
    // evidence that names the unified directory.
    for result in compatibility_matrix() {
        if matches!(&result.kind, CompatibilityKind::DirectUnified) {
            let evidence = result.evidence.join("\n");
            assert!(
                evidence.contains(".agents") && evidence.contains("skills"),
                "direct unified support for {} must cite ~/.agents/skills evidence",
                result.tool
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical unified Skills directory (REQ-002 / AC-002 / AC-008)
//
// Public boundary: installation, scanning, synchronization, and displayed
// records all use `~/.agents/skills` as the canonical source/destination
// instead of a tool-specific or OneSpace-internal maintenance directory.
//
// These tests are RED against the current implementation: empty
// initialization, global install, scan, sync, and display still resolve to the
// internal per-model maintenance directory.
// ---------------------------------------------------------------------------

fn unified_skills_root(home: &Path) -> std::path::PathBuf {
    home.join(".agents").join("skills")
}

fn assert_under_unified(home: &Path, path: &Path, context: &str) {
    let unified = unified_skills_root(home);
    let unified_canonical = fs::canonicalize(&unified).unwrap_or_else(|_| unified.clone());
    let path_canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    assert!(
        path_canonical.starts_with(&unified_canonical),
        "{context}: expected {path:?} under unified {} but resolved to {path_canonical:?}",
        unified.display()
    );
}

#[test]
fn global_installation_uses_the_unified_agents_directory_for_every_model() {
    with_temp_home("unified-install-target", |home| {
        let unified = unified_skills_root(home);
        for model in MODELS {
            let (primary, _compat) = resolve_skill_target_dir(model, INSTALL_SCOPE_GLOBAL, None)
                .unwrap_or_else(|err| panic!("resolve global target for {model}: {err}"));
            assert_under_unified(home, &primary, "global install target");
            assert!(
                unified.exists(),
                "empty initialization must create the unified directory {} for {model}",
                unified.display()
            );
            assert!(
                !primary.starts_with(home.join(".config").join("onespace")),
                "global install target {primary:?} must not use the internal maintenance directory"
            );
        }
    });
}

#[test]
fn global_scan_surfaces_skills_from_the_unified_agents_directory() {
    with_temp_home("unified-scan", |home| {
        let unified = unified_skills_root(home);
        let skill_dir = unified.join("git-commit");
        write_skill_dir(&skill_dir, "git-commit", "Git Commit", "Unified copy");

        let mut state = SkillsLocalState::default();
        rebuild_local_installed_from_models(&mut state).expect("rebuild global installed skills");

        let found = state
            .skills
            .iter()
            .find(|skill| skill.dir_name == "git-commit")
            .expect("a skill stored in the unified directory must be scanned");
        let target = found
            .target_path
            .as_deref()
            .expect("scanned skill must report a target path");
        assert_under_unified(home, Path::new(target), "scanned target path");
    });
}

#[test]
fn global_record_display_path_resolves_under_the_unified_agents_directory() {
    with_temp_home("unified-display", |home| {
        let record = SkillRecord {
            id: "official-git-commit".to_string(),
            dir_name: "git-commit".to_string(),
            model: "codex".to_string(),
            models: vec!["codex".to_string()],
            name: "Git Commit".to_string(),
            description: "Unified copy".to_string(),
            source_id: "official".to_string(),
            source_rel_path: "git-commit".to_string(),
            installed_at: 1,
            updated_at: None,
            last_synced_at: None,
            local_hash: String::new(),
            remote_hash: None,
            has_update: false,
            icon_seed: "official".to_string(),
            scope: INSTALL_SCOPE_GLOBAL.to_string(),
            project_root: None,
            target_path: None,
        };

        let local_dir = record_local_dir(&record).expect("resolve displayed local directory");
        assert_under_unified(home, &local_dir, "displayed local directory");
    });
}

#[test]
fn global_sync_projects_unified_skills_into_tool_directories() {
    with_temp_home("unified-sync", |home| {
        let unified = unified_skills_root(home);
        let skill_dir = unified.join("git-commit");
        write_skill_dir(&skill_dir, "git-commit", "Git Commit", "Unified copy");

        reconcile_one_model("codex", INSTALL_SCOPE_GLOBAL, None).expect("reconcile codex global");

        let projected = home.join(".codex").join("skills").join("git-commit");
        assert!(
            projected.join("SKILL.md").exists(),
            "sync must project the unified skill into {}, found none",
            projected.display()
        );
        assert!(
            skill_dir.join("SKILL.md").exists(),
            "the canonical unified copy must remain after syncing"
        );
    });
}

// ---------------------------------------------------------------------------
// Unified Skills migration and conflict backups (REQ-003 / AC-003,
// REQ-004 / AC-004)
//
// Public boundary: `initialize_unified_skills` performs the first
// initialization described by the frozen plan. Tool-specific Skills are
// migrated into `~/.agents/skills` with their content preserved. When the same
// Skill name already exists in the unified directory, the unified version stays
// authoritative and the tool-specific version is preserved under
// `~/.agents/skills/.backups/<tool>/<skill>/`, keyed by content so that an
// unchanged conflict never creates another backup. The returned
// `SkillsInitResult` exposes per-Skill outcomes whose status identifies a
// conflict.
//
// These tests are RED against the current implementation: no initialization
// entry point, migration result, or `.backups` handling exists yet.
// ---------------------------------------------------------------------------

fn find_file_with_content(root: &Path, expected: &str) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_with_content(&path, expected) {
                return Some(found);
            }
        } else if fs::read_to_string(&path)
            .map(|content| content == expected)
            .unwrap_or(false)
        {
            return Some(path);
        }
    }
    None
}

fn snapshot_file_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, current: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if let Ok(content) = fs::read(&path) {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(path.as_path())
                    .to_string_lossy()
                    .to_string();
                out.push((relative, content));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn first_initialization_migrates_non_conflicting_tool_skill_into_unified_directory() {
    with_temp_home("step3-migrate", |home| {
        let source_dir = home.join(".codex").join("skills").join("git-commit");
        write_skill_dir(&source_dir, "git-commit", "Git Commit", "Tool-specific copy");
        let source_md =
            fs::read_to_string(source_dir.join("SKILL.md")).expect("read tool-specific skill");

        let result = initialize_unified_skills().expect("initialize unified skills");

        let migrated_dir = unified_skills_root(home).join("git-commit");
        assert!(
            migrated_dir.join("SKILL.md").exists(),
            "a non-conflicting tool Skill must be migrated into {}",
            migrated_dir.display()
        );
        assert_eq!(
            fs::read_to_string(migrated_dir.join("SKILL.md")).expect("read migrated skill"),
            source_md,
            "migration must preserve the tool-specific content"
        );

        let outcome = result
            .outcomes
            .iter()
            .find(|outcome| outcome.tool == "codex" && outcome.skill == "git-commit")
            .expect("result must report the migrated Skill");
        assert!(
            matches!(&outcome.status, SkillMigrationStatus::Migrated),
            "non-conflicting migration must be reported as migrated, got {:?}",
            outcome.status
        );
    });
}

#[test]
fn first_initialization_keeps_unified_on_conflict_and_backs_up_tool_copy() {
    with_temp_home("step3-conflict", |home| {
        let unified_dir = unified_skills_root(home).join("git-commit");
        write_skill_dir(&unified_dir, "git-commit", "Git Commit", "Unified version");
        let unified_md =
            fs::read_to_string(unified_dir.join("SKILL.md")).expect("read unified skill");

        let source_dir = home.join(".codex").join("skills").join("git-commit");
        write_skill_dir(&source_dir, "git-commit", "Git Commit", "Tool-specific version");
        let source_md =
            fs::read_to_string(source_dir.join("SKILL.md")).expect("read tool-specific skill");

        let result = initialize_unified_skills().expect("initialize unified skills");

        assert_eq!(
            fs::read_to_string(unified_dir.join("SKILL.md")).expect("read unified skill"),
            unified_md,
            "the unified version must remain authoritative on conflict"
        );

        let backup_root = unified_skills_root(home)
            .join(".backups")
            .join("codex")
            .join("git-commit");
        assert!(
            backup_root.exists(),
            "the conflicting tool copy must be preserved under {}",
            backup_root.display()
        );
        assert!(
            find_file_with_content(&backup_root, &source_md).is_some(),
            "the backup under {} must contain the tool-specific content",
            backup_root.display()
        );

        let outcome = result
            .outcomes
            .iter()
            .find(|outcome| outcome.tool == "codex" && outcome.skill == "git-commit")
            .expect("result must report the conflicting Skill");
        assert!(
            matches!(&outcome.status, SkillMigrationStatus::ConflictBackedUp),
            "the result must identify the conflict, got {:?}",
            outcome.status
        );
        let backup_path = outcome
            .backup_path
            .as_deref()
            .expect("a conflict outcome must report its backup path");
        let reported = Path::new(backup_path);
        assert!(
            reported.starts_with(&backup_root),
            "reported backup path {backup_path} must live under {}",
            backup_root.display()
        );
        assert!(
            reported.exists(),
            "reported backup path {backup_path} must exist on disk"
        );
    });
}

#[test]
fn repeated_initialization_does_not_duplicate_unchanged_conflict_backup() {
    with_temp_home("step3-backup-idempotent", |home| {
        let unified_dir = unified_skills_root(home).join("git-commit");
        write_skill_dir(&unified_dir, "git-commit", "Git Commit", "Unified version");

        let source_dir = home.join(".codex").join("skills").join("git-commit");
        write_skill_dir(&source_dir, "git-commit", "Git Commit", "Tool-specific version");

        initialize_unified_skills().expect("first initialization");
        let backup_root = unified_skills_root(home)
            .join(".backups")
            .join("codex")
            .join("git-commit");
        assert!(
            backup_root.exists(),
            "first initialization must create a conflict backup"
        );
        let first_snapshot = snapshot_file_tree(&backup_root);

        initialize_unified_skills().expect("second initialization");
        let second_snapshot = snapshot_file_tree(&backup_root);

        assert_eq!(
            first_snapshot, second_snapshot,
            "an unchanged conflict must not create another backup or alter the existing one"
        );
    });
}

// ---------------------------------------------------------------------------
// Claude compatibility path (REQ-005 / AC-005, REQ-006 / AC-006)
//
// Public boundary: `initialize_unified_skills` keeps Claude working by making
// `~/.claude/skills` a symlink that resolves to the canonical `~/.agents/skills`
// directory when that path is free. Every occupied-path collision - ordinary
// file, ordinary directory, wrong symlink, or broken symlink - must be preserved
// exactly as the user left it and reported as an actionable failure that names
// the conflicting Claude path instead of being silently replaced.
//
// These tests are RED against the current implementation: initialization only
// migrates Skills and never creates, inspects, or reports on ~/.claude/skills.
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn claude_skills_path(home: &Path) -> std::path::PathBuf {
    home.join(".claude").join("skills")
}

#[cfg(unix)]
fn assert_actionable_claude_failure(result: Result<SkillsInitResult, String>) {
    let message = match result {
        Ok(_) => {
            panic!("an occupied ~/.claude/skills path must be rejected with an actionable failure")
        }
        Err(message) => message,
    };
    let lowered = message.to_lowercase();
    assert!(
        lowered.contains("claude") && lowered.contains("skills"),
        "the failure must name the conflicting Claude Skills path, got: {message:?}"
    );
}

#[cfg(unix)]
#[test]
fn claude_compat_initialization_creates_skills_symlink_to_unified_directory() {
    with_temp_home("step4-claude-symlink", |home| {
        let claude = claude_skills_path(home);
        let unified = unified_skills_root(home);
        assert!(
            fs::symlink_metadata(&claude).is_err(),
            "precondition: {} must not exist before initialization",
            claude.display()
        );

        initialize_unified_skills().expect("initialization must succeed for a free Claude path");

        let meta = fs::symlink_metadata(&claude).unwrap_or_else(|err| {
            panic!("{} must exist after initialization: {err}", claude.display())
        });
        assert!(
            meta.file_type().is_symlink(),
            "{} must be a symlink, got {:?}",
            claude.display(),
            meta.file_type()
        );
        let resolved = fs::canonicalize(&claude).expect("Claude symlink must resolve");
        let unified_resolved =
            fs::canonicalize(&unified).expect("unified Skills directory must exist");
        assert_eq!(
            resolved,
            unified_resolved,
            "{} must resolve to the unified directory {}",
            claude.display(),
            unified.display()
        );
    });
}

#[cfg(unix)]
#[test]
fn claude_compat_initialization_preserves_occupied_file_and_reports_failure() {
    with_temp_home("step4-claude-file", |home| {
        let claude = claude_skills_path(home);
        fs::create_dir_all(claude.parent().expect("Claude parent directory"))
            .expect("create .claude");
        fs::write(&claude, "user-owned file\n").expect("write occupied file");

        assert_actionable_claude_failure(initialize_unified_skills());

        let meta = fs::symlink_metadata(&claude).expect("occupied file must remain");
        assert!(
            meta.file_type().is_file() && !meta.file_type().is_symlink(),
            "{} must remain an ordinary file",
            claude.display()
        );
        assert_eq!(
            fs::read_to_string(&claude).expect("read occupied file"),
            "user-owned file\n",
            "the occupied file content must be preserved"
        );
    });
}

#[cfg(unix)]
#[test]
fn claude_compat_initialization_preserves_occupied_directory_and_reports_failure() {
    with_temp_home("step4-claude-directory", |home| {
        let claude = claude_skills_path(home);
        fs::create_dir_all(&claude).expect("create occupied directory");
        fs::write(claude.join("marker.txt"), "user-owned directory\n").expect("write marker");

        assert_actionable_claude_failure(initialize_unified_skills());

        let meta = fs::symlink_metadata(&claude).expect("occupied directory must remain");
        assert!(
            meta.file_type().is_dir() && !meta.file_type().is_symlink(),
            "{} must remain an ordinary directory",
            claude.display()
        );
        assert_eq!(
            fs::read_to_string(claude.join("marker.txt")).expect("read marker"),
            "user-owned directory\n",
            "the occupied directory content must be preserved"
        );
    });
}

#[cfg(unix)]
#[test]
fn claude_compat_initialization_preserves_wrong_symlink_and_reports_failure() {
    with_temp_home("step4-claude-wrong-symlink", |home| {
        let claude = claude_skills_path(home);
        fs::create_dir_all(claude.parent().expect("Claude parent directory"))
            .expect("create .claude");
        let wrong_target = home.join(".claude").join("other-skills");
        fs::create_dir_all(&wrong_target).expect("create wrong target");
        symlink(&wrong_target, &claude).expect("create wrong symlink");

        assert_actionable_claude_failure(initialize_unified_skills());

        let meta = fs::symlink_metadata(&claude).expect("wrong symlink must remain");
        assert!(
            meta.file_type().is_symlink(),
            "{} must remain a symlink",
            claude.display()
        );
        assert_eq!(
            fs::read_link(&claude).expect("read wrong symlink"),
            wrong_target,
            "the wrong symlink target must not be rewritten"
        );
        assert_eq!(
            fs::canonicalize(&claude).expect("wrong symlink must still resolve"),
            fs::canonicalize(&wrong_target).expect("wrong target must exist"),
            "the wrong symlink must not be repointed at the unified directory"
        );
    });
}

#[cfg(unix)]
#[test]
fn claude_compat_initialization_preserves_broken_symlink_and_reports_failure() {
    with_temp_home("step4-claude-broken-symlink", |home| {
        let claude = claude_skills_path(home);
        fs::create_dir_all(claude.parent().expect("Claude parent directory"))
            .expect("create .claude");
        let missing_target = home.join(".claude").join("missing-skills");
        symlink(&missing_target, &claude).expect("create broken symlink");

        assert_actionable_claude_failure(initialize_unified_skills());

        let meta = fs::symlink_metadata(&claude).expect("broken symlink must remain");
        assert!(
            meta.file_type().is_symlink(),
            "{} must remain a symlink",
            claude.display()
        );
        assert_eq!(
            fs::read_link(&claude).expect("read broken symlink"),
            missing_target,
            "the broken symlink target must not be rewritten"
        );
        assert!(
            !claude.exists(),
            "the broken symlink must stay broken instead of being repointed"
        );
    });
}

// ---------------------------------------------------------------------------
// Idempotent repeated initialization and truthful partial failures
// (REQ-007 / AC-007, REQ-008 / AC-010)
//
// Public boundary: `initialize_unified_skills` must be stable when it runs
// again without source changes. A Skill migrated on the first run is identical
// in the unified directory and in its tool directory, so the second run must
// not reclassify it as a conflict, add a backup, change unified content, or
// replace the correct Claude compatibility symlink. When a later item fails,
// the successful migration must remain on disk and the returned failure must
// identify the failed Skill and its path instead of a bare OS error.
//
// These tests are RED against the current implementation: a repeated run backs
// up every already-migrated Skill as a new conflict, and a failed copy aborts
// with an OS error that names neither the item nor its path.
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn repeated_initialization_keeps_migrated_skill_backup_free_and_symlink_stable() {
    with_temp_home("step5-idempotent", |home| {
        use std::os::unix::fs::MetadataExt;

        let source_dir = home.join(".codex").join("skills").join("git-commit");
        write_skill_dir(&source_dir, "git-commit", "Git Commit", "Tool-specific copy");

        let unified = unified_skills_root(home);
        let claude = claude_skills_path(home);

        initialize_unified_skills().expect("first initialization");
        assert!(
            unified.join("git-commit").join("SKILL.md").exists(),
            "the non-conflicting Skill must be migrated on the first run"
        );
        assert!(
            !unified
                .join(".backups")
                .join("codex")
                .join("git-commit")
                .exists(),
            "a non-conflicting migration must not create a conflict backup"
        );

        let first_tree = snapshot_file_tree(&unified);
        let first_target = fs::read_link(&claude).expect("claude compatibility symlink");
        let first_inode = fs::symlink_metadata(&claude)
            .expect("claude compatibility symlink metadata")
            .ino();

        initialize_unified_skills().expect("second initialization");

        assert_eq!(
            snapshot_file_tree(&unified),
            first_tree,
            "repeated initialization must not add a backup or change unified content"
        );
        assert_eq!(
            fs::read_link(&claude).expect("claude compatibility symlink"),
            first_target,
            "a correct Claude symlink must not be repointed"
        );
        assert_eq!(
            fs::symlink_metadata(&claude)
                .expect("claude compatibility symlink metadata")
                .ino(),
            first_inode,
            "a correct Claude symlink must not be replaced"
        );
    });
}

#[cfg(unix)]
#[test]
fn partial_migration_failure_names_failed_item_and_keeps_successful_migration() {
    use std::os::unix::fs::PermissionsExt;

    with_temp_home("step5-partial-failure", |home| {
        // "antigravity" is processed before "codex", so this migration succeeds
        // before the failing item is reached.
        let good_source = home
            .join(".gemini")
            .join("config")
            .join("skills")
            .join("good-skill");
        write_skill_dir(&good_source, "good-skill", "Good Skill", "Migratable copy");

        let bad_source = home.join(".codex").join("skills").join("bad-skill");
        write_skill_dir(&bad_source, "bad-skill", "Bad Skill", "Unreadable copy");
        let mut unreadable = fs::metadata(&bad_source)
            .expect("bad skill metadata")
            .permissions();
        unreadable.set_mode(0o000);
        fs::set_permissions(&bad_source, unreadable).expect("make bad skill unreadable");

        let result = initialize_unified_skills();

        // Restore permissions before asserting so the temp HOME can be removed.
        let mut readable = fs::metadata(&bad_source)
            .expect("bad skill metadata")
            .permissions();
        readable.set_mode(0o755);
        fs::set_permissions(&bad_source, readable).expect("restore bad skill permissions");

        let unified = unified_skills_root(home);
        assert!(
            unified.join("good-skill").join("SKILL.md").exists(),
            "a successful migration must remain after a later item fails"
        );

        let message = match result {
            Ok(value) => panic!(
                "a failed migration item must not be reported as complete success, got {:?}",
                value.outcomes
            ),
            Err(message) => message,
        };
        let source_path = bad_source.to_string_lossy().to_string();
        assert!(
            message.contains("bad-skill")
                && (message.contains("codex")
                    || message.contains(&source_path)
                    || message.contains(".agents")),
            "the failure must identify the failed Skill item and its path, got: {message:?}"
        );
    });
}
