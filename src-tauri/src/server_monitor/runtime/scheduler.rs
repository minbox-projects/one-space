//! Interval and daily scheduling, target mutation and settings changes.

use super::collector::{
    build_custom_record, same_source, validate_target, COLLECTION_LIMIT_ERROR,
};
use super::state::{
    daily_slot_eligible, invalidate_target_sessions, local_slot_ms, today_string,
    DailyQueueEntry, ServerMonitorRuntime,
};
use crate::server_monitor::storage::ServerMonitorConfig;
use crate::server_monitor::types::{
    ServerMonitorScheduleMode, ServerMonitorSettings, ServerMonitorState, ServerMonitorTargetDraft,
    ServerMonitorTargetRecord, SessionLifetime,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

pub(super) const DAILY_SLOT_WINDOW_MS: i64 = 60_000;
const TICK_INTERVAL: Duration = Duration::from_secs(1);

impl ServerMonitorRuntime {
    /// Starts the scheduler once and arms the initial interval collection.
    pub(crate) fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let now = self.clock.now_ms();
        if let Ok(mut inner) = self.inner.lock() {
            let mode = inner.config.settings.schedule_mode;
            if inner.config.settings.auto_collect_enabled {
                match mode {
                    ServerMonitorScheduleMode::Interval => {
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, now);
                        }
                    }
                    ServerMonitorScheduleMode::Daily => {
                        for id in all_target_ids(&inner.config) {
                            inner.daily_armed_after_ms.insert(id, now);
                        }
                    }
                }
            }
            let retention = inner.config.settings.retention_days;
            drop(inner);
            let today = today_string(now);
            let _ = self.store.prune(retention, now, &today);
        }
        let runtime = self.clone();
        std::thread::spawn(move || {
            while !runtime.shutting_down.load(Ordering::SeqCst) {
                let now = runtime.clock.now_ms();
                let _ = runtime.tick(now);
                runtime.send_keepalives();
                std::thread::sleep(TICK_INTERVAL);
            }
        });
    }

    pub(crate) fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if let Ok(mut inner) = self.inner.lock() {
            let persistent = inner.sessions.drain().map(|(_, session)| session).collect::<Vec<_>>();
            let owned = inner
                .owned_sessions
                .drain()
                .flat_map(|(_, sessions)| sessions)
                .collect::<Vec<_>>();
            let mut ids = inner.inflight.keys().cloned().collect::<Vec<_>>();
            ids.extend(inner.config.targets.iter().map(|target| target.id.clone()));
            for id in ids {
                *inner.generation.entry(id).or_insert(0) += 1;
            }
            inner.interval_due.clear();
            inner.daily_armed_after_ms.clear();
            inner.daily_queue.clear();
            drop(inner);
            for session in persistent.into_iter().chain(owned) {
                session.invalidate();
            }
        }
    }

    fn send_keepalives(&self) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let sessions = match self.inner.lock() {
            Ok(inner) => inner.sessions.values().cloned().collect::<Vec<_>>(),
            Err(_) => return,
        };
        let now = std::time::Instant::now();
        for session in sessions {
            session.keepalive_if_due(now);
        }
    }

    /// Runs one scheduler tick. Claim write failures are returned to the caller.
    pub(crate) fn tick(self: &Arc<Self>, now_ms: i64) -> Result<(), String> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Ok(());
        }
        let (mode, auto_enabled, targets) = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            (
                inner.config.settings.schedule_mode,
                inner.config.settings.auto_collect_enabled,
                inner.config.targets.clone(),
            )
        };
        if !auto_enabled {
            return Ok(());
        }

        match mode {
            ServerMonitorScheduleMode::Interval => {
                let due = {
                    let inner = self.inner.lock().map_err(|error| error.to_string())?;
                    targets
                        .iter()
                        .filter(|target| {
                            target.enabled
                                && !target.blocked
                                && !inner.inflight.contains_key(&target.id)
                                && inner
                                    .interval_due
                                    .get(&target.id)
                                    .copied()
                                    .map(|value| value <= now_ms)
                                    .unwrap_or(false)
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                };
                let interval_minutes = self.settings_snapshot().interval_minutes;
                let expected = Some(u64::from(interval_minutes * 60));
                for target in due {
                    match self.begin_collection(
                        target,
                        SessionLifetime::Persistent,
                        ServerMonitorScheduleMode::Interval,
                        expected,
                    ) {
                        Ok(_) => {}
                        Err(error) if error == COLLECTION_LIMIT_ERROR => break,
                        Err(error) => {
                            self.set_error(error);
                            break;
                        }
                    }
                }
            }
            ServerMonitorScheduleMode::Daily => {
                self.process_daily_queue(now_ms)?;
                self.collect_due_daily_targets(now_ms, &targets)?;
            }
        }
        Ok(())
    }

    /// Starts already-claimed slots that were waiting for collection capacity.
    fn process_daily_queue(self: &Arc<Self>, now_ms: i64) -> Result<(), String> {
        let mut ready = Vec::new();
        let mut keep = Vec::new();
        {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            if inner.config.settings.schedule_mode != ServerMonitorScheduleMode::Daily
                || !inner.config.settings.auto_collect_enabled
            {
                inner.daily_queue.clear();
                return Ok(());
            }
            if inner.last_queue_tick_ms == Some(now_ms) {
                return Ok(());
            }
            inner.last_queue_tick_ms = Some(now_ms);
            let queued = std::mem::take(&mut inner.daily_queue);
            for entry in queued {
                if now_ms < entry.slot_ms {
                    // A clock rollback must not start a future slot early.
                    keep.push(entry);
                    continue;
                }
                if now_ms >= entry.slot_ms + DAILY_SLOT_WINDOW_MS {
                    // The one-minute opportunity passed; the claim stays consumed.
                    continue;
                }
                let Some(target) = inner
                    .config
                    .targets
                    .iter()
                    .find(|target| target.id == entry.target_id)
                    .cloned()
                else {
                    continue;
                };
                if !target.enabled || target.blocked {
                    continue;
                }
                if inner.inflight.contains_key(&entry.target_id) {
                    keep.push(entry);
                    continue;
                }
                ready.push((entry, target));
            }
            inner.daily_queue = keep;
        }
        for (entry, target) in ready {
            match self.begin_collection(
                target,
                SessionLifetime::OneShot,
                ServerMonitorScheduleMode::Daily,
                None,
            ) {
                Ok(_) => {}
                Err(error) if error == COLLECTION_LIMIT_ERROR => {
                    if let Ok(mut inner) = self.inner.lock() {
                        inner.daily_queue.push(entry);
                    }
                }
                Err(error) => self.set_error(error),
            }
        }
        Ok(())
    }

    fn collect_due_daily_targets(
        self: &Arc<Self>,
        now_ms: i64,
        targets: &[ServerMonitorTargetRecord],
    ) -> Result<(), String> {
        let (times, armed, busy) = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            (
                inner.config.settings.daily_times.clone(),
                inner.daily_armed_after_ms.clone(),
                inner
                    .inflight
                    .keys()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>(),
            )
        };
        let date = today_string(now_ms);
        for target in targets
            .iter()
            .filter(|target| target.enabled && !target.blocked)
        {
            let armed_after = armed.get(&target.id).copied();
            for planned_time in &times {
                let Some(slot_ms) = local_slot_ms(&date, planned_time) else {
                    // Nonexistent local times (for example a spring-forward gap)
                    // are skipped rather than shifted.
                    continue;
                };
                if now_ms < slot_ms || now_ms >= slot_ms + DAILY_SLOT_WINDOW_MS {
                    continue;
                }
                if !daily_slot_eligible(slot_ms, armed_after) {
                    continue;
                }
                if busy.contains(&target.id) {
                    // A manual collection already covers this slot. Claiming it
                    // prevents a second collection after the manual result lands.
                    self.claim_daily_slot(&target.id, &date, planned_time, now_ms)?;
                    break;
                }
                let claimed = self.claim_daily_slot(&target.id, &date, planned_time, now_ms)?;
                if claimed {
                    match self.begin_collection(
                        target.clone(),
                        SessionLifetime::OneShot,
                        ServerMonitorScheduleMode::Daily,
                        None,
                    ) {
                        Ok(_) => {}
                        Err(error) if error == COLLECTION_LIMIT_ERROR => {
                            if let Ok(mut inner) = self.inner.lock() {
                                inner.daily_queue.push(DailyQueueEntry {
                                    target_id: target.id.clone(),
                                    slot_ms,
                                });
                            }
                        }
                        Err(error) => self.set_error(error),
                    }
                }
                break;
            }
        }
        Ok(())
    }

    /// Claims one daily slot, publishing and returning storage failures.
    pub(super) fn claim_daily_slot(
        &self,
        target_id: &str,
        date: &str,
        planned_time: &str,
        now_ms: i64,
    ) -> Result<bool, String> {
        match self
            .store
            .claim_slot(target_id, date, planned_time, now_ms)
        {
            Ok(crate::server_monitor::types::SlotClaimOutcome::Claimed) => Ok(true),
            Ok(crate::server_monitor::types::SlotClaimOutcome::AlreadyClaimed) => Ok(false),
            Err(error) => {
                self.set_error(error.clone());
                Err(error)
            }
        }
    }

    fn settings_snapshot(&self) -> ServerMonitorSettings {
        self.inner
            .lock()
            .map(|inner| inner.config.settings.clone())
            .unwrap_or_default()
    }

    pub(crate) fn upsert_target(
        self: &Arc<Self>,
        draft: ServerMonitorTargetDraft,
    ) -> Result<ServerMonitorState, String> {
        let now = self.clock.now_ms();
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let existing_index = draft
            .id
            .as_ref()
            .and_then(|id| config.targets.iter().position(|target| &target.id == id));
        let existing = existing_index.map(|index| config.targets[index].clone());

        let id = draft
            .id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let name = draft.name.trim().to_string();
        let custom = draft
            .custom
            .as_ref()
            .map(|custom| build_custom_record(custom, existing.as_ref()));
        let record = ServerMonitorTargetRecord {
            id: id.clone(),
            name,
            enabled: draft.enabled,
            source_kind: draft.source_kind,
            saved_host_name: draft
                .saved_host_name
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            custom,
            created_at_ms: existing
                .as_ref()
                .map(|record| record.created_at_ms)
                .unwrap_or(now),
            updated_at_ms: now,
            // Any accepted edit clears the previous error and retries explicitly.
            blocked: false,
            last_error_kind: None,
            last_error: None,
            last_success_at_ms: existing
                .as_ref()
                .and_then(|record| record.last_success_at_ms),
        };
        validate_target(&record)?;
        if config
            .targets
            .iter()
            .enumerate()
            .any(|(index, target)| Some(index) != existing_index && target.id == record.id)
        {
            return Err("Target id must be unique".to_string());
        }

        let source_changed = existing
            .as_ref()
            .map(|previous| !same_source(previous, &record))
            .unwrap_or(false);
        let was_disabled = existing
            .as_ref()
            .map(|previous| !previous.enabled)
            .unwrap_or(true);
        let was_blocked = existing
            .as_ref()
            .map(|previous| previous.blocked)
            .unwrap_or(false);
        let disabling = existing
            .as_ref()
            .map(|previous| previous.enabled && !record.enabled)
            .unwrap_or(false);
        if let Some(index) = existing_index {
            config.targets[index] = record.clone();
        } else {
            config.targets.push(record.clone());
        }
        self.store.save_config(&config)?;

        inner.config = config;
        inner.error = None;
        let reschedule =
            existing.is_none() || source_changed || was_disabled || was_blocked || disabling;
        if source_changed || disabling {
            *inner.generation.entry(id.clone()).or_insert(0) += 1;
            invalidate_target_sessions(&mut inner, &id);
            inner
                .daily_queue
                .retain(|entry| entry.target_id != id);
        }
        let mode = inner.config.settings.schedule_mode;
        let auto_enabled = inner.config.settings.auto_collect_enabled;
        if disabling || !record.enabled {
            inner.interval_due.remove(&id);
            inner.daily_armed_after_ms.remove(&id);
        } else {
            match mode {
                ServerMonitorScheduleMode::Interval if auto_enabled => {
                    if reschedule {
                        inner.interval_due.insert(id.clone(), now);
                    }
                }
                ServerMonitorScheduleMode::Interval => {
                    inner.interval_due.remove(&id);
                }
                ServerMonitorScheduleMode::Daily if auto_enabled => {
                    if reschedule {
                        inner.daily_armed_after_ms.insert(id.clone(), now);
                    }
                }
                ServerMonitorScheduleMode::Daily => {}
            }
        }
        drop(inner);
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn delete_target(
        self: &Arc<Self>,
        target_id: &str,
    ) -> Result<ServerMonitorState, String> {
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let before = config.targets.len();
        config.targets.retain(|target| target.id != target_id);
        if config.targets.len() == before {
            return Ok(self.snapshot());
        }
        self.store.save_config(&config)?;
        inner.config = config;
        inner.error = None;
        inner.interval_due.remove(target_id);
        inner.daily_armed_after_ms.remove(target_id);
        inner
            .daily_queue
            .retain(|entry| entry.target_id != target_id);
        inner.inflight.remove(target_id);
        *inner.generation.entry(target_id.to_string()).or_insert(0) += 1;
        invalidate_target_sessions(&mut inner, target_id);
        drop(inner);
        self.store.delete_target_data(target_id)?;
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn set_target_enabled(
        self: &Arc<Self>,
        target_id: &str,
        enabled: bool,
    ) -> Result<ServerMonitorState, String> {
        let now = self.clock.now_ms();
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let Some(target) = config
            .targets
            .iter_mut()
            .find(|target| target.id == target_id)
        else {
            return Err("Server monitor target not found".to_string());
        };
        target.enabled = enabled;
        target.updated_at_ms = now;
        if enabled {
            // Resuming is an explicit retry: clear the prior block state.
            target.blocked = false;
            target.last_error_kind = None;
            target.last_error = None;
        }
        self.store.save_config(&config)?;
        inner.config = config;
        inner.error = None;
        if enabled {
            let mode = inner.config.settings.schedule_mode;
            let auto_enabled = inner.config.settings.auto_collect_enabled;
            inner
                .daily_queue
                .retain(|entry| entry.target_id != target_id);
            match mode {
                ServerMonitorScheduleMode::Interval if auto_enabled => {
                    inner.interval_due.insert(target_id.to_string(), now);
                }
                ServerMonitorScheduleMode::Interval => {
                    inner.interval_due.remove(target_id);
                }
                ServerMonitorScheduleMode::Daily if auto_enabled => {
                    inner
                        .daily_armed_after_ms
                        .insert(target_id.to_string(), now);
                }
                ServerMonitorScheduleMode::Daily => {}
            }
        } else {
            inner.interval_due.remove(target_id);
            inner.daily_armed_after_ms.remove(target_id);
            inner
                .daily_queue
                .retain(|entry| entry.target_id != target_id);
            *inner.generation.entry(target_id.to_string()).or_insert(0) += 1;
            invalidate_target_sessions(&mut inner, target_id);
        }
        drop(inner);
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn save_settings(
        self: &Arc<Self>,
        settings: ServerMonitorSettings,
    ) -> Result<ServerMonitorState, String> {
        settings.validate()?;
        let now = self.clock.now_ms();
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let previous = config.settings.clone();
        config.settings = settings.clone();
        self.store.save_config(&config)?;
        inner.config = config;
        inner.error = None;
        let mode_changed = previous.schedule_mode != settings.schedule_mode;
        let auto_disabled = previous.auto_collect_enabled && !settings.auto_collect_enabled;
        let auto_enabled = !previous.auto_collect_enabled && settings.auto_collect_enabled;
        let interval_changed = previous.interval_minutes != settings.interval_minutes;
        if mode_changed || auto_disabled || !settings.auto_collect_enabled {
            let persistent = inner
                .sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>();
            let owned = inner
                .owned_sessions
                .drain()
                .flat_map(|(_, sessions)| sessions)
                .collect::<Vec<_>>();
            inner.interval_due.clear();
            inner.daily_armed_after_ms.clear();
            inner.daily_queue.clear();
            for id in all_target_ids(&inner.config) {
                *inner.generation.entry(id).or_insert(0) += 1;
            }
            drop(inner);
            for session in persistent.into_iter().chain(owned) {
                session.invalidate();
            }
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            if settings.auto_collect_enabled {
                match settings.schedule_mode {
                    ServerMonitorScheduleMode::Interval => {
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, now);
                        }
                    }
                    ServerMonitorScheduleMode::Daily => {
                        for id in all_target_ids(&inner.config) {
                            inner.daily_armed_after_ms.insert(id, now);
                        }
                    }
                }
            }
            drop(inner);
        } else {
            match settings.schedule_mode {
                ServerMonitorScheduleMode::Interval => {
                    if auto_enabled {
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, now);
                        }
                    } else if interval_changed {
                        let next = now + i64::from(settings.interval_minutes) * 60_000;
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, next);
                        }
                    }
                }
                ServerMonitorScheduleMode::Daily => {
                    if settings.auto_collect_enabled {
                        for id in all_target_ids(&inner.config) {
                            inner.daily_armed_after_ms.insert(id, now);
                        }
                    }
                    inner.daily_queue.clear();
                }
            }
            drop(inner);
        }
        let today = today_string(now);
        let _ = self.store.prune(settings.retention_days, now, &today);
        self.publish();
        Ok(self.snapshot())
    }
}

fn scheduled_target_ids(config: &ServerMonitorConfig) -> Vec<String> {
    config
        .targets
        .iter()
        .filter(|target| target.enabled && !target.blocked)
        .map(|target| target.id.clone())
        .collect()
}

fn all_target_ids(config: &ServerMonitorConfig) -> Vec<String> {
    config
        .targets
        .iter()
        .map(|target| target.id.clone())
        .collect()
}
