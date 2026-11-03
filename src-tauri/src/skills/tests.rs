use super::*;
use crate::config::{SkillSourceConfig, StorageConfig};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

/// Retains the global `HOME` lock on purpose: the skills code resolves tool
/// directories such as `~/.agents/skills` and the `~/.claude/skills` symlink
/// through `dirs::home_dir()`, which the thread-local `get_app_dir()` override
/// does not cover. A process-wide `HOME` change plus the shared mutex is
/// required to keep these directory-based cases isolated and serialized.
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
    let baseline_mtime = fs::metadata(&skill_md)
        .expect("metadata before")
        .modified()
        .expect("mtime before");

    // The previous version slept 1.2s and rewrote identical bytes so the file
    // mtime would advance. Set a different mtime explicitly instead: the content
    // stays byte-identical while the directory hash must remain unchanged.
    let changed_mtime = baseline_mtime
        .checked_add(Duration::from_secs(60))
        .expect("changed mtime");
    let file = fs::OpenOptions::new()
        .write(true)
        .open(&skill_md)
        .expect("open skill markdown");
    file.set_modified(changed_mtime).expect("set changed mtime");
    drop(file);

    let observed_mtime = fs::metadata(&skill_md)
        .expect("metadata after")
        .modified()
        .expect("mtime after");
    assert_ne!(
        baseline_mtime, observed_mtime,
        "the explicit mtime change must be observable on disk"
    );

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

        let global_old_dir = global_skills_dir()
            .expect("global skills dir")
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
                    model: GLOBAL_SKILL_MODEL.to_string(),
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
        assert_eq!(
            result.synced_models,
            vec!["global".to_string(), "codex".to_string()]
        );

        let new_global_dir = global_skills_dir()
            .expect("global skills dir")
            .join("git-commit-v2");
        let new_project_dir = project_primary_dir("codex", &project_root)
            .expect("project primary")
            .join("git-commit-v2");
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
            fs::read_to_string(project_compat_dir.join("SKILL.md"))
                .expect("project compat updated"),
            expected_md
        );
        assert!(!home.join(".claude").join("skills").exists());
        assert!(!home.join(".codex").join("skills").exists());
        assert!(!home.join(".gemini").join("config").join("skills").exists());
        assert!(!home
            .join(".config")
            .join("opencode")
            .join("skills")
            .exists());
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

        let installed_dir = global_skills_dir()
            .expect("global skills dir")
            .join("git-commit");
        write_skill_dir(&installed_dir, "git-commit", "Git Commit", "Old version");
        let conflicting_dir = global_skills_dir()
            .expect("global skills dir")
            .join("shared-dir");
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
                    model: GLOBAL_SKILL_MODEL.to_string(),
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
                    model: GLOBAL_SKILL_MODEL.to_string(),
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
        let installed_dir = global_skills_dir()
            .expect("global skills dir")
            .join("git-commit");
        write_skill_dir(&installed_dir, "git-commit", "Git Commit", "Installed");

        save_local_skills_state(SkillsLocalState {
            skills: vec![SkillRecord {
                id: "official-git-commit".to_string(),
                dir_name: "git-commit".to_string(),
                model: GLOBAL_SKILL_MODEL.to_string(),
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
            Some(GLOBAL_SKILL_MODEL.to_string()),
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
        rebuild_global_installed_from_unified(&mut state).expect("rebuild global installed skills");

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
fn has_dir_name_conflict_is_global_across_models() {
    let state = SkillsLocalState {
        skills: vec![
            SkillRecord {
                id: "legacy-1".to_string(),
                dir_name: "git-commit".to_string(),
                model: GLOBAL_SKILL_MODEL.to_string(),
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
                scope: INSTALL_SCOPE_PROJECT.to_string(),
                project_root: Some("/tmp/project".to_string()),
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
        Some("legacy-1"),
    ));
    assert!(has_dir_name_conflict(
        &state,
        "claude",
        INSTALL_SCOPE_PROJECT,
        Some("/tmp/project"),
        "git-commit",
        Some("other-id"),
    ));
    assert!(!has_dir_name_conflict(
        &state,
        "codex",
        INSTALL_SCOPE_PROJECT,
        Some("/tmp/project"),
        "git-commit",
        Some("other-id"),
    ));
}

#[test]
fn global_install_target_is_unified_for_every_model_without_tool_projections() {
    with_temp_home("global-target-only", |home| {
        let unified = unified_skills_root(home);
        for model in MODELS {
            let (primary, compat) = resolve_skill_target_dir(model, INSTALL_SCOPE_GLOBAL, None)
                .unwrap_or_else(|err| panic!("resolve global target for {model}: {err}"));
            assert_under_unified(home, &primary, "global install target");
            assert!(
                compat.is_empty(),
                "global install for {model} must not project into tool directories"
            );
        }
        assert!(unified.exists());
        assert!(!home.join(".claude").join("skills").exists());
        assert!(!home.join(".codex").join("skills").exists());
        assert!(!home.join(".gemini").join("config").join("skills").exists());
        assert!(!home
            .join(".config")
            .join("opencode")
            .join("skills")
            .exists());
    });
}

#[test]
fn canonical_display_count_counts_unified_skill_once() {
    with_temp_home("canonical-count", |home| {
        write_skill_dir(
            &unified_skills_root(home).join("git-commit"),
            "git-commit",
            "Git Commit",
            "Unified copy",
        );

        let count =
            skills_installed_count_all_scopes().expect("count installed skills across scopes");
        assert_eq!(
            count, 1,
            "one unified skill must be displayed once, not once per supported tool"
        );
    });
}

#[test]
fn local_sync_registers_tool_and_unified_skills_without_writing() {
    with_temp_home("local-sync-discover", |home| {
        let unified = unified_skills_root(home);
        write_skill_dir(
            &unified.join("unified-skill"),
            "unified-skill",
            "Unified",
            "From unified",
        );
        let claude_dir = home.join(".claude").join("skills").join("claude-skill");
        let codex_dir = home.join(".codex").join("skills").join("codex-skill");
        let antigravity_dir = home
            .join(".gemini")
            .join("config")
            .join("skills")
            .join("antigravity-skill");
        let opencode_dir = home
            .join(".config")
            .join("opencode")
            .join("skills")
            .join("opencode-skill");
        write_skill_dir(&claude_dir, "claude-skill", "Claude", "From claude");
        write_skill_dir(&codex_dir, "codex-skill", "Codex", "From codex");
        write_skill_dir(
            &antigravity_dir,
            "antigravity-skill",
            "Antigravity",
            "From antigravity",
        );
        write_skill_dir(&opencode_dir, "opencode-skill", "OpenCode", "From opencode");

        let mut shared_state = SkillsState::default();
        let local_state = SkillsLocalState::default();
        assert!(sync_local_repositories(&mut shared_state, &local_state).expect("sync local"));

        assert_eq!(shared_state.repositories.len(), 5);
        for repo in &shared_state.repositories {
            assert_eq!(repo.source_type, "local_import");
            let source_path = repo.source_path.as_deref().expect("discovered source path");
            assert!(Path::new(source_path).join("SKILL.md").is_file());
            assert!(
                !repo_storage_dir(&repo.repo_key)
                    .expect("repo storage path")
                    .exists(),
                "discovery must not copy skill content into repository storage"
            );
        }
        for path in [&claude_dir, &codex_dir, &antigravity_dir, &opencode_dir] {
            assert!(
                path.join("SKILL.md").is_file(),
                "discovery must never remove local tool skills"
            );
        }
        assert!(
            !sync_local_repositories(&mut shared_state, &local_state)
                .expect("idempotent local sync"),
            "an unchanged local skill set must not rewrite repository state"
        );
    });
}

#[test]
fn local_sync_refreshes_and_removes_discovered_repositories() {
    with_temp_home("local-sync-refresh", |home| {
        let claude_dir = home.join(".claude").join("skills").join("claude-skill");
        let codex_dir = home.join(".codex").join("skills").join("codex-skill");
        write_skill_dir(&claude_dir, "claude-skill", "Claude", "Initial");
        write_skill_dir(&codex_dir, "codex-skill", "Codex", "Removed later");

        let mut shared_state = SkillsState::default();
        let local_state = SkillsLocalState::default();
        assert!(sync_local_repositories(&mut shared_state, &local_state).expect("first sync"));
        let first_hash = shared_state
            .repositories
            .iter()
            .find(|repo| repo.source_rel_path == "claude-skill")
            .and_then(|repo| repo.hash.clone())
            .expect("claude hash");
        let repo_snapshot = repo_storage_dir(&make_repo_key("local", "claude-skill"))
            .expect("local repository snapshot");
        replace_dir_atomic(&claude_dir, &repo_snapshot).expect("seed local repository snapshot");

        write_skill_dir(&claude_dir, "claude-skill", "Claude", "Updated");
        fs::remove_dir_all(&codex_dir).expect("remove codex skill");
        assert!(sync_local_repositories(&mut shared_state, &local_state).expect("second sync"));

        assert_eq!(shared_state.repositories.len(), 1);
        let repo = &shared_state.repositories[0];
        assert_eq!(repo.source_rel_path, "claude-skill");
        assert_ne!(repo.hash.as_deref(), Some(first_hash.as_str()));
        assert!(claude_dir.join("SKILL.md").is_file());
        let snapshot_markdown = fs::read_to_string(repo_snapshot.join("SKILL.md"))
            .expect("refreshed repository snapshot markdown");
        assert!(
            snapshot_markdown.contains("Updated"),
            "local sync must refresh an existing repository snapshot from the source"
        );
    });
}

#[test]
fn skills_commands_registration_reflects_local_sync() {
    const RUN_APP_SOURCE: &str = include_str!("../app_runtime/run_app.rs");
    let handler = RUN_APP_SOURCE
        .split_once(".invoke_handler")
        .map(|(_, rest)| rest)
        .expect("tauri invoke_handler block must exist");

    assert!(
        handler.contains("skills::skills_sync_local"),
        "local discovery must be registered as a Tauri command"
    );
    for removed in [
        "skills::skills_initialize_unified",
        "skills::skills_compatibility_get",
        "skills::skills_rescan_mirror",
        "skills::skills_reconcile",
    ] {
        assert!(
            !handler.contains(removed),
            "{removed} must not be registered after the global projection refactor"
        );
    }
}

#[test]
fn local_rescan_and_count_ignore_stage_copies_of_skills() {
    with_temp_home("rescan-skip-stage", |home| {
        let unified = unified_skills_root(home);
        write_skill_dir(
            &unified.join("git-commit"),
            "git-commit",
            "Git Commit",
            "Unified version",
        );
        write_skill_dir(
            &unified.join(".stage-42"),
            "git-commit",
            "Git Commit",
            "Stage copy",
        );

        let mut state = SkillsLocalState::default();
        rebuild_global_installed_from_unified(&mut state).expect("rebuild installed");
        let names: Vec<_> = state.skills.iter().map(|s| s.dir_name.clone()).collect();
        assert_eq!(
            names,
            vec!["git-commit".to_string()],
            "a stage copy must not be recorded as an installed skill"
        );

        let count =
            skills_installed_count_all_scopes().expect("count installed skills across scopes");
        assert_eq!(
            count, 1,
            "a stage copy must not inflate the installed skill count"
        );
    });
}

#[test]
fn directory_hash_ignores_internal_artifacts() {
    with_temp_home("hash-ignore-internal", |home| {
        let plain = home.join("plain");
        let with_internal = home.join("with-internal");
        write_skill_dir(&plain, "git-commit", "Git Commit", "Same content");
        write_skill_dir(&with_internal, "git-commit", "Git Commit", "Same content");
        write_skill_dir(
            &with_internal.join(".backups").join("codex").join("x"),
            "x",
            "X",
            "Junk backup",
        );
        fs::create_dir_all(with_internal.join(".stage-9")).expect("create stage");
        assert_eq!(
            hash_dir(&plain).expect("hash plain"),
            hash_dir(&with_internal).expect("hash with internal artifacts"),
            "internal artifacts must not affect the directory hash"
        );
    });
}

// ---------------------------------------------------------------------------
// Dev/release environment isolation (REQ-001 / AC-001)
//
// Observable boundary: the Skills local cache base root must be derived from the
// profile app directory so a dev build never shares caches with the release
// profile.
//
// The delivered contract routes `skills_local_cache_base_root_at` through an
// explicit app dir (`<app_dir>/skills`), and `skills_local_cache_base_root()` is
// bound to `config::get_app_dir()`, so the local cache follows the profile.
// ---------------------------------------------------------------------------

/// Removes a temporary profile home on drop so a failing assertion cannot leak it.
struct SkillsProfileTempHome(PathBuf);

impl Drop for SkillsProfileTempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn skills_local_cache_base_root_follows_profile_app_dir() {
    let home = PathBuf::from("/tmp/onespace-skills-profile-home");
    assert_eq!(
        skills_local_cache_base_root_at(&crate::config::app_dir_for(&home, false)),
        home.join(".config").join("onespace").join("skills")
    );
    assert_eq!(
        skills_local_cache_base_root_at(&crate::config::app_dir_for(&home, true)),
        home.join(".config").join("onespace-dev").join("skills")
    );

    let temp_home = std::env::temp_dir().join(format!(
        "onespace-skills-profile-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let _cleanup = SkillsProfileTempHome(temp_home.clone());
    fs::create_dir_all(&temp_home).expect("create temp home");
    let _guard = crate::config::test_home::TestHomeGuard::set(&temp_home);

    let base = skills_local_cache_base_root().expect("resolve skills local cache base");
    assert_eq!(
        base,
        temp_home.join(".config").join("onespace").join("skills"),
        "the skills local cache must resolve under the release profile app dir in tests"
    );
    let app_dir = crate::config::get_app_dir().expect("resolve profile app dir");
    assert!(
        base.starts_with(&app_dir),
        "skills local cache base {} must live under the profile app dir {}",
        base.display(),
        app_dir.display()
    );

    drop(_guard);
}
