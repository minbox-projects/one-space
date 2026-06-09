#[tauri::command]
pub fn skills_local_scan(input: LocalScanInput) -> Result<ApiOk<Vec<LocalSkillCandidate>>, String> {
    let root_can = resolve_scan_root(&input.root_path)?;
    let list = scan_local_candidates(&root_can)?;
    let shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let revision = combined_revision(&shared_state, &local_state);
    api_ok(list, revision)
}

#[tauri::command]
pub async fn skills_repo_import_folder(
    app: tauri::AppHandle,
    input: RepoImportFolderInput,
) -> Result<ApiOk<RepoImportFolderResult>, String> {
    let folder_can = resolve_scan_root(&input.folder_path)?;
    let dedupe_key = format!(
        "repo_import_folder:{}",
        sha256_hex(&folder_can.to_string_lossy())
    );
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            return Err("skills/import_busy".to_string());
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let skill_md = folder_can.join("SKILL.md");
    if !skill_md.exists() {
        return Err("skills/invalid_skill_dir".to_string());
    }
    let md_content = fs::read_to_string(&skill_md).map_err(|e| e.to_string())?;
    let dir_name = read_required_skill_dir_name(&folder_can)?;
    let (name, description, declared_models) = parse_skill_md(&md_content, &[]);
    let source_id = local_source_id(&folder_can);
    let source_rel_path = ".".to_string();
    let skill_id = local_skill_id(&source_id, &source_rel_path);

    let mut shared_state = load_skills_state()?;
    let local_state = load_local_skills_state()?;
    let record = upsert_repository_from_dir(
        &mut shared_state,
        &folder_can,
        &source_id,
        &source_rel_path,
        &skill_id,
        &dir_name,
        "local_import",
        &name,
        &description,
        &declared_models,
        &source_id,
        Some(folder_can.to_string_lossy().to_string()),
        None,
        false,
    )?;
    let _ = upsert_repo_dir_name(
        &mut shared_state,
        &source_id,
        &source_rel_path,
        &skill_id,
        &dir_name,
    );
    shared_state = save_skills_state(shared_state)?;
    trigger_storage_sync(app, "skills_repo_import_folder");

    let result = RepoImportFolderResult {
        repo_key: record.repo_key,
        skill_id: record.skill_id,
        source_id: record.source_id,
        source_rel_path: record.source_rel_path,
    };
    api_ok(result, combined_revision(&shared_state, &local_state))
}

#[tauri::command]
pub async fn skills_local_import(
    app: tauri::AppHandle,
    input: LocalImportInput,
) -> Result<ApiOk<LocalImportResult>, String> {
    let root_can = resolve_scan_root(&input.root_path)?;
    let source_id = local_source_id(&root_can);
    let dedupe_key = format!("local_import:{}", source_id);
    let _job = match acquire_job_key(dedupe_key)? {
        Some(v) => v,
        None => {
            let shared_state = load_skills_state()?;
            let local_state = load_local_skills_state()?;
            return api_ok(
                LocalImportResult::default(),
                combined_revision(&shared_state, &local_state),
            );
        }
    };
    let _guard = job_lock().lock().map_err(|e| e.to_string())?;

    let mut models = vec![];
    let mut model_seen = HashSet::new();
    for model in &input.models {
        if !MODELS.contains(&model.as_str()) {
            return Err(format!("unsupported model: {}", model));
        }
        if model_seen.insert(model.clone()) {
            models.push(model.clone());
        }
    }
    if models.is_empty() {
        return Err("skills/models_required".to_string());
    }
    if input.selections.is_empty() {
        return Err("skills/selections_required".to_string());
    }

    let candidates = scan_local_candidates(&root_can)?;
    let mut candidate_map: HashMap<String, LocalSkillCandidate> = HashMap::new();
    for c in candidates {
        candidate_map.insert(c.rel_path.clone(), c);
    }

    let mut shared_state = load_skills_state()?;
    let mut local_state = load_local_skills_state()?;
    let mut result = LocalImportResult::default();
    let mut shared_changed = false;
    let mut local_changed = false;

    for selection in &input.selections {
        let strategy = selection.conflict_strategy.trim().to_lowercase();
        if strategy != "overwrite" && strategy != "skip" {
            for model in &models {
                result.failed.push(LocalImportFailed {
                    rel_path: selection.rel_path.clone(),
                    skill_id: None,
                    model: model.clone(),
                    reason: "invalid_conflict_strategy".to_string(),
                });
            }
            continue;
        }

        let Some(candidate) = candidate_map.get(&selection.rel_path) else {
            for model in &models {
                result.failed.push(LocalImportFailed {
                    rel_path: selection.rel_path.clone(),
                    skill_id: None,
                    model: model.clone(),
                    reason: "skill_not_found".to_string(),
                });
            }
            continue;
        };

        let src = if candidate.rel_path == "." {
            root_can.clone()
        } else {
            root_can.join(&candidate.rel_path)
        };
        if !src.join("SKILL.md").exists() {
            for model in &models {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: "skills/invalid_skill_dir".to_string(),
                });
            }
            continue;
        }
        let candidate_dir_name = match read_required_skill_dir_name(&src) {
            Ok(name) => name,
            Err(err) => {
                for model in &models {
                    result.failed.push(LocalImportFailed {
                        rel_path: candidate.rel_path.clone(),
                        skill_id: Some(candidate.skill_id.clone()),
                        model: model.clone(),
                        reason: err.clone(),
                    });
                }
                continue;
            }
        };

        let repo_key = make_repo_key(&source_id, &candidate.rel_path);
        let repo_exists = shared_state
            .repositories
            .iter()
            .any(|r| r.repo_key == repo_key);
        let repo_record = match upsert_repository_from_dir(
            &mut shared_state,
            &src,
            &source_id,
            &candidate.rel_path,
            &candidate.skill_id,
            &candidate_dir_name,
            "local_import",
            &candidate.name,
            &candidate.description,
            &candidate.declared_models,
            &source_id,
            Some(src.to_string_lossy().to_string()),
            None,
            true,
        ) {
            Ok(v) => v,
            Err(err) => {
                for model in &models {
                    result.failed.push(LocalImportFailed {
                        rel_path: candidate.rel_path.clone(),
                        skill_id: Some(candidate.skill_id.clone()),
                        model: model.clone(),
                        reason: err.clone(),
                    });
                }
                continue;
            }
        };
        shared_changed = true;
        shared_changed = upsert_repo_dir_name(
            &mut shared_state,
            &source_id,
            &candidate.rel_path,
            &candidate.skill_id,
            &candidate_dir_name,
        ) || shared_changed;
        if !repo_exists {
            result.repo_added.push(LocalImportRepoAdded {
                repo_key: repo_record.repo_key.clone(),
                skill_id: repo_record.skill_id.clone(),
                source_id: repo_record.source_id.clone(),
                source_rel_path: repo_record.source_rel_path.clone(),
            });
        }

        let repo_src = repo_storage_dir(&repo_record.repo_key)?;
        for model in &models {
            let model_root = model_dir(model)?;
            let dest = model_root.join(&candidate_dir_name);
            ensure_within(&model_root, &dest)?;
            let existing_same_id = local_state
                .skills
                .iter()
                .any(|s| s.model == *model && s.id == candidate.skill_id);
            if strategy == "skip" && existing_same_id {
                result.skipped.push(LocalImportSkipped {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: candidate.skill_id.clone(),
                    model: model.clone(),
                    reason: "conflict_exists".to_string(),
                });
                continue;
            }
            if let Err(err) = ensure_model_dir_name_available(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &candidate_dir_name,
                Some(candidate.skill_id.as_str()),
            ) {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: err,
                });
                continue;
            }
            if let Err(err) = remove_existing_record_dir_if_moved(
                &local_state,
                model,
                INSTALL_SCOPE_GLOBAL,
                None,
                &candidate.skill_id,
                &dest,
            ) {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: err,
                });
                continue;
            }

            if let Err(err) = replace_dir_atomic(&repo_src, &dest) {
                result.failed.push(LocalImportFailed {
                    rel_path: candidate.rel_path.clone(),
                    skill_id: Some(candidate.skill_id.clone()),
                    model: model.clone(),
                    reason: err,
                });
                continue;
            }

            let local_hash = match hash_dir(&dest) {
                Ok(hash) => hash,
                Err(err) => {
                    result.failed.push(LocalImportFailed {
                        rel_path: candidate.rel_path.clone(),
                        skill_id: Some(candidate.skill_id.clone()),
                        model: model.clone(),
                        reason: err,
                    });
                    continue;
                }
            };

            local_state.skills.retain(|s| {
                !(s.model == *model
                    && s.id == candidate.skill_id
                    && record_scope(s) == INSTALL_SCOPE_GLOBAL)
            });
            let record = SkillRecord {
                id: candidate.skill_id.clone(),
                dir_name: candidate_dir_name.clone(),
                model: model.clone(),
                models: candidate.declared_models.clone(),
                name: candidate.name.clone(),
                description: candidate.description.clone(),
                source_id: source_id.clone(),
                source_rel_path: candidate.rel_path.clone(),
                installed_at: now_ts(),
                updated_at: None,
                last_synced_at: None,
                local_hash,
                remote_hash: None,
                has_update: false,
                icon_seed: source_id.clone(),
                scope: INSTALL_SCOPE_GLOBAL.to_string(),
                project_root: None,
                target_path: Some(dest.to_string_lossy().to_string()),
            };
            local_state.skills.push(record.clone());
            result.installed.push(record);
            local_changed = true;
        }
    }

    if shared_changed {
        shared_state = save_skills_state(shared_state)?;
    }
    if local_changed {
        local_state = save_local_skills_state(local_state)?;
    }
    for model in &models {
        let _ = reconcile_internal(Some(model), Some(INSTALL_SCOPE_GLOBAL), None);
    }
    if shared_changed {
        trigger_storage_sync(app, "skills_local_import");
    }
    api_ok(result, combined_revision(&shared_state, &local_state))
}
