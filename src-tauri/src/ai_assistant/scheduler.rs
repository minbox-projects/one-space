pub fn init_scheduler(app: tauri::AppHandle) {
    if SCHEDULER_STARTED.set(()).is_err() {
        return;
    }

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        // Handle misfire on startup
        if let Ok(state) = load_state() {
            let now = now_ts();
            let misfired: Vec<(String, String)> = state
                .schedules
                .iter()
                .filter(|schedule| {
                    schedule.enabled
                        && schedule.next_run_at.unwrap_or(0) < now
                        && schedule.last_run_at.unwrap_or(0) < schedule.next_run_at.unwrap_or(0)
                })
                .map(|schedule| (schedule.id.clone(), schedule.misfire_policy.clone()))
                .collect();

            for (schedule_id, policy) in misfired {
                match policy.as_str() {
                    "immediate" => {
                        let _ = trigger_schedule_run(app_clone.clone(), schedule_id).await;
                    }
                    "next_window" => {
                        if let Ok(mut state) = load_state() {
                            if let Some(schedule) =
                                state.schedules.iter_mut().find(|s| s.id == schedule_id)
                            {
                                schedule.next_run_at = compute_next_run_at(
                                    &schedule.trigger,
                                    now,
                                    schedule.timezone.as_deref(),
                                );
                                schedule.last_status = Some("misfire_rescheduled".to_string());
                                let _ = save_state(&state);
                            }
                        }
                    }
                    _ => { /* skip - do nothing */ }
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        loop {
            let now = now_ts();
            let due = load_state()
                .map(|state| {
                    state
                        .schedules
                        .iter()
                        .filter(|schedule| {
                            schedule.enabled && schedule.next_run_at.unwrap_or(0) <= now
                        })
                        .map(|schedule| schedule.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for schedule_id in due {
                let _ = trigger_schedule_run(app_clone.clone(), schedule_id).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
    });
}
