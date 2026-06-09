use super::{
    allowed_run_status, api_ok, load_runs, now_ts, save_runs, ApiOk, WorkflowRun,
    WorkflowRunDeleteInput, WorkflowRunListInput, WorkflowRunUpdateInput,
};
use serde_json::{json, Value};

#[tauri::command]
pub fn workflows_runs_list(
    input: Option<WorkflowRunListInput>,
) -> Result<ApiOk<Vec<WorkflowRun>>, String> {
    let payload = input.unwrap_or(WorkflowRunListInput {
        preset_id: None,
        limit: Some(100),
    });
    let mut runs = load_runs()?;
    if let Some(preset_id) = payload.preset_id {
        runs.retain(|r| r.preset_id == preset_id);
    }
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    if let Some(limit) = payload.limit {
        runs.truncate(limit.max(1));
    }
    api_ok(runs)
}

#[tauri::command]
pub fn workflows_run_update(input: WorkflowRunUpdateInput) -> Result<ApiOk<WorkflowRun>, String> {
    let status = input.status.trim().to_lowercase();
    if !allowed_run_status(&status) {
        return Err("invalid run status".to_string());
    }
    let mut runs = load_runs()?;
    let idx = runs
        .iter()
        .position(|r| r.id == input.run_id)
        .ok_or_else(|| "workflow run not found".to_string())?;

    runs[idx].status = status.clone();
    runs[idx].summary = input.summary.and_then(|s| {
        let v = s.trim().to_string();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    });
    runs[idx].error_message = input.error_message.and_then(|s| {
        let v = s.trim().to_string();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    });
    if status == "running" {
        runs[idx].ended_at = None;
    } else if runs[idx].ended_at.is_none() {
        runs[idx].ended_at = Some(now_ts());
    }

    let updated = runs[idx].clone();
    save_runs(&runs)?;
    api_ok(updated)
}

#[tauri::command]
pub fn workflows_run_delete(input: WorkflowRunDeleteInput) -> Result<ApiOk<Value>, String> {
    let run_id = input.run_id.trim().to_string();
    if run_id.is_empty() {
        return Err("run id required".to_string());
    }

    let mut runs = load_runs()?;
    let before = runs.len();
    runs.retain(|r| r.id != run_id);
    if runs.len() == before {
        return Err("workflow run not found".to_string());
    }

    save_runs(&runs)?;
    api_ok(json!({ "deleted": true }))
}
