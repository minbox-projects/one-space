#[derive(Debug, Clone, Default)]
struct SkillIndexes {
    installed_ids: HashSet<String>,
    installed_source_rel: HashSet<(String, String)>,
    catalog_by_source_ref: HashMap<(String, String), skills::CatalogSkill>,
    catalog_by_id: HashMap<String, skills::CatalogSkill>,
    catalog_by_rel_path: HashMap<String, skills::CatalogSkill>,
    repo_by_key: HashMap<String, skills::RepositorySkillView>,
    repo_by_skill_id: HashMap<String, skills::RepositorySkillView>,
    repo_by_source_rel: HashMap<(String, String), skills::RepositorySkillView>,
}

#[derive(Debug, Clone)]
enum ResolvedSkillTarget {
    Catalog {
        source_id: String,
        skill_ref: String,
        skill_id: String,
        source_rel_path: String,
        skill_name: String,
    },
    Repo {
        repo_key: String,
        skill_id: String,
        source_id: String,
        source_rel_path: String,
        skill_name: String,
    },
}

fn make_repo_key(source_id: &str, source_rel_path: &str) -> String {
    format!("{}::{}", source_id, source_rel_path)
}

fn parse_catalog_selector(input: &str) -> Option<(String, String)> {
    let prefix = "catalog::";
    let value = input.trim();
    if !value.starts_with(prefix) {
        return None;
    }
    let payload = &value[prefix.len()..];
    let mut parts = payload.splitn(2, "::");
    let source_id = parts.next()?.trim();
    let skill_ref = parts.next()?.trim();
    if source_id.is_empty() || skill_ref.is_empty() {
        return None;
    }
    Some((source_id.to_string(), skill_ref.to_string()))
}

fn parse_repo_selector(input: &str) -> Option<String> {
    let prefix = "repo::";
    let value = input.trim();
    if !value.starts_with(prefix) {
        return None;
    }
    let payload = value[prefix.len()..].trim();
    if payload.is_empty() {
        return None;
    }
    Some(payload.to_string())
}

fn parse_legacy_skill_ref(input: &str) -> Option<(String, String)> {
    let value = input.trim();
    if value.starts_with("catalog::") || value.starts_with("repo::") {
        return None;
    }
    let mut parts = value.splitn(2, "::");
    let source = parts.next()?.trim();
    let skill_ref = parts.next()?.trim();
    if source.is_empty() || skill_ref.is_empty() {
        return None;
    }
    Some((source.to_string(), skill_ref.to_string()))
}

fn repo_installed_for_tool(repo: &skills::RepositorySkillView, tool: &str) -> bool {
    match tool {
        "claude" => repo.installed.claude,
        "codex" => repo.installed.codex,
        "gemini" => repo.installed.gemini,
        "opencode" => repo.installed.opencode,
        _ => false,
    }
}

fn canonicalize_working_dir(working_dir: &str) -> Option<String> {
    let raw = working_dir.trim();
    if raw.is_empty() {
        return None;
    }
    fs::canonicalize(PathBuf::from(raw))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| Some(raw.to_string()))
}

fn install_scope_and_project_root(
    launch_scope: &str,
    working_dir: &str,
) -> (String, Option<String>) {
    if launch_scope == LAUNCH_SCOPE_STRICT {
        ("project".to_string(), canonicalize_working_dir(working_dir))
    } else {
        ("global".to_string(), None)
    }
}

fn build_skill_indexes(tool: &str, scope: &str, project_root: Option<&str>) -> SkillIndexes {
    let installed_records = skills::skills_list_installed(
        None,
        Some(scope.to_string()),
        project_root.map(|v| v.to_string()),
    )
    .map(|resp| {
        resp.data
            .into_iter()
            .filter(|record| record.model == tool)
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();
    let catalog = skills::skills_list_catalog(Some(tool.to_string()))
        .map(|resp| resp.data)
        .unwrap_or_default();
    let repo_list = skills::skills_repo_list(
        Some(false),
        Some(scope.to_string()),
        project_root.map(|v| v.to_string()),
    )
    .map(|resp| resp.data)
    .unwrap_or_default();

    let mut indexes = SkillIndexes::default();

    for record in installed_records {
        indexes.installed_ids.insert(record.id.clone());
        indexes
            .installed_source_rel
            .insert((record.source_id.clone(), record.source_rel_path.clone()));
    }

    for item in catalog {
        indexes.catalog_by_source_ref.insert(
            (item.source_id.clone(), item.rel_path.clone()),
            item.clone(),
        );
        indexes
            .catalog_by_source_ref
            .insert((item.source_id.clone(), item.id.clone()), item.clone());
        indexes.catalog_by_id.insert(item.id.clone(), item.clone());
        indexes
            .catalog_by_rel_path
            .insert(item.rel_path.clone(), item.clone());
    }

    for repo in repo_list {
        indexes
            .repo_by_source_rel
            .entry((repo.source_id.clone(), repo.source_rel_path.clone()))
            .or_insert_with(|| repo.clone());
        indexes
            .repo_by_skill_id
            .entry(repo.skill_id.clone())
            .or_insert_with(|| repo.clone());
        indexes.repo_by_key.insert(repo.repo_key.clone(), repo);
    }

    indexes
}

fn resolve_catalog_target(
    source_id: &str,
    skill_ref: &str,
    indexes: &SkillIndexes,
) -> Option<ResolvedSkillTarget> {
    let item = indexes
        .catalog_by_source_ref
        .get(&(source_id.to_string(), skill_ref.to_string()))?;
    Some(ResolvedSkillTarget::Catalog {
        source_id: item.source_id.clone(),
        skill_ref: item.rel_path.clone(),
        skill_id: item.id.clone(),
        source_rel_path: item.rel_path.clone(),
        skill_name: item.name.clone(),
    })
}

fn resolve_skill_target(raw: &str, indexes: &SkillIndexes) -> Option<ResolvedSkillTarget> {
    if let Some(repo_key) = parse_repo_selector(raw) {
        let repo = indexes.repo_by_key.get(&repo_key)?;
        return Some(ResolvedSkillTarget::Repo {
            repo_key: repo.repo_key.clone(),
            skill_id: repo.skill_id.clone(),
            source_id: repo.source_id.clone(),
            source_rel_path: repo.source_rel_path.clone(),
            skill_name: repo.name.clone(),
        });
    }

    if let Some((source_id, skill_ref)) = parse_catalog_selector(raw) {
        return resolve_catalog_target(&source_id, &skill_ref, indexes);
    }

    if let Some((source_id, skill_ref)) = parse_legacy_skill_ref(raw) {
        if let Some(catalog_target) = resolve_catalog_target(&source_id, &skill_ref, indexes) {
            return Some(catalog_target);
        }
        if let Some(repo) = indexes
            .repo_by_source_rel
            .get(&(source_id.clone(), skill_ref.clone()))
        {
            return Some(ResolvedSkillTarget::Repo {
                repo_key: repo.repo_key.clone(),
                skill_id: repo.skill_id.clone(),
                source_id: repo.source_id.clone(),
                source_rel_path: repo.source_rel_path.clone(),
                skill_name: repo.name.clone(),
            });
        }
    }

    if let Some(item) = indexes.catalog_by_id.get(raw) {
        return Some(ResolvedSkillTarget::Catalog {
            source_id: item.source_id.clone(),
            skill_ref: item.rel_path.clone(),
            skill_id: item.id.clone(),
            source_rel_path: item.rel_path.clone(),
            skill_name: item.name.clone(),
        });
    }
    if let Some(item) = indexes.catalog_by_rel_path.get(raw) {
        return Some(ResolvedSkillTarget::Catalog {
            source_id: item.source_id.clone(),
            skill_ref: item.rel_path.clone(),
            skill_id: item.id.clone(),
            source_rel_path: item.rel_path.clone(),
            skill_name: item.name.clone(),
        });
    }
    if let Some(repo) = indexes.repo_by_skill_id.get(raw) {
        return Some(ResolvedSkillTarget::Repo {
            repo_key: repo.repo_key.clone(),
            skill_id: repo.skill_id.clone(),
            source_id: repo.source_id.clone(),
            source_rel_path: repo.source_rel_path.clone(),
            skill_name: repo.name.clone(),
        });
    }

    None
}

fn target_installed(
    target: &ResolvedSkillTarget,
    installed_ids: &HashSet<String>,
    installed_source_rel: &HashSet<(String, String)>,
    repo_installed_by_key: &HashMap<String, bool>,
) -> bool {
    match target {
        ResolvedSkillTarget::Catalog {
            source_id,
            source_rel_path,
            skill_id,
            ..
        } => {
            installed_ids.contains(skill_id)
                || installed_source_rel.contains(&(source_id.clone(), source_rel_path.clone()))
        }
        ResolvedSkillTarget::Repo {
            repo_key,
            source_id,
            source_rel_path,
            skill_id,
            ..
        } => {
            repo_installed_by_key
                .get(repo_key)
                .copied()
                .unwrap_or(false)
                || installed_ids.contains(skill_id)
                || installed_source_rel.contains(&(source_id.clone(), source_rel_path.clone()))
        }
    }
}
