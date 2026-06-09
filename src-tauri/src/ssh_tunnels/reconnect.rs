fn running_auto_reconnect_candidate_ids() -> Result<Vec<String>, String> {
    let records = load_records()?;
    let reconnect_enabled_ids = records
        .iter()
        .filter(|record| record.auto_reconnect)
        .map(|record| record.id.clone())
        .collect::<HashSet<_>>();
    let manager = runtime_manager().lock().map_err(|e| e.to_string())?;
    Ok(manager
        .keys()
        .filter(|id| reconnect_enabled_ids.contains(*id))
        .cloned()
        .collect())
}

fn try_begin_reconnect_reconcile(reason: &str) -> bool {
    let now = now_ts();
    let last = LAST_RECONNECT_RECONCILE_AT.load(Ordering::Relaxed);
    if now.saturating_sub(last) < RECONNECT_RECONCILE_COOLDOWN.as_secs() {
        log::debug!(
            "SSH tunnel auto reconnect reconcile skipped for {} due to cooldown",
            reason
        );
        return false;
    }
    if RECONNECT_RECONCILE_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        log::debug!(
            "SSH tunnel auto reconnect reconcile skipped for {} because one is already running",
            reason
        );
        return false;
    }
    LAST_RECONNECT_RECONCILE_AT.store(now, Ordering::Relaxed);
    true
}

fn reconcile_auto_reconnect(app: AppHandle, reason: &'static str) {
    if !try_begin_reconnect_reconcile(reason) {
        return;
    }

    let result = (|| -> Result<(), String> {
        let candidate_ids = running_auto_reconnect_candidate_ids()?;
        if candidate_ids.is_empty() {
            log::debug!(
                "SSH tunnel auto reconnect reconcile found no candidates for {}",
                reason
            );
            return Ok(());
        }
        log::info!(
            "SSH tunnel auto reconnect reconcile started for {} with {} candidate(s)",
            reason,
            candidate_ids.len()
        );
        for id in candidate_ids {
            if let Err(error) = connect_internal(app.clone(), id.clone(), false) {
                let _ = update_record_error(&id, &error);
                if let Ok(Some(record)) = load_record_by_id(&id) {
                    record_tunnel_failure(&app, &record, &error, "auto-reconnect");
                }
                log::warn!(
                    "SSH tunnel auto reconnect failed for {} after {}: {}",
                    id,
                    reason,
                    error
                );
            }
        }
        Ok(())
    })();

    if let Err(error) = result {
        log::warn!(
            "SSH tunnel auto reconnect reconcile failed for {}: {}",
            reason,
            error
        );
    }
    RECONNECT_RECONCILE_RUNNING.store(false, Ordering::Release);
}

fn schedule_auto_reconnect_reconcile(app: AppHandle, reason: &'static str, delay: Duration) {
    thread::spawn(move || {
        thread::sleep(delay);
        reconcile_auto_reconnect(app, reason);
    });
}

pub fn start_sleep_resume_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut last_seen = SystemTime::now();
        loop {
            thread::sleep(SLEEP_RESUME_HEARTBEAT_INTERVAL);
            let now = SystemTime::now();
            let elapsed = now
                .duration_since(last_seen)
                .unwrap_or(SLEEP_RESUME_HEARTBEAT_INTERVAL);
            last_seen = now;
            if elapsed >= SLEEP_RESUME_GAP_THRESHOLD {
                schedule_auto_reconnect_reconcile(
                    app.clone(),
                    "sleep-gap-heartbeat",
                    RECONNECT_RESUME_DELAY,
                );
            }
        }
    });
}

#[cfg(target_os = "macos")]
pub fn start_system_wake_observer(app: AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;

    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    let block = RcBlock::new(move |_notification: NonNull<NSNotification>| {
        schedule_auto_reconnect_reconcile(
            app.clone(),
            "macos-wake-notification",
            RECONNECT_RESUME_DELAY,
        );
    });
    let wake_notification = unsafe { NSWorkspaceDidWakeNotification };
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(wake_notification),
            None,
            None,
            &block,
        )
    };

    // Keep the observer and block alive for the process lifetime.
    std::mem::forget(observer);
    std::mem::forget(block);
}

#[cfg(not(target_os = "macos"))]
pub fn start_system_wake_observer(_app: AppHandle) {}
