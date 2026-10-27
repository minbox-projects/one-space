//! Collection orchestration: starting, running and finishing one collection.

use super::state::{
    daily_slot_eligible, evaluate_alert, local_slot_ms, report_sample, today_string,
    CollectionReport, InFlight, ServerMonitorRuntime, CANCELLED_MESSAGE,
};
use crate::server_monitor::transport::{CollectionError, MonitorSession};
use crate::server_monitor::types::{
    CollectionFailureKind, CollectionOutcome, MonitorAuth, MonitorConnectRequest, MonitorSource,
    MonitorTransportErrorKind, ServerMonitorCustomDraft, ServerMonitorCustomRecord,
    ServerMonitorOutcome, ServerMonitorScheduleMode, ServerMonitorSourceKind,
    ServerMonitorState, ServerMonitorTargetDraft, ServerMonitorTargetRecord,
    ServerMonitorTestResult, ServerMonitorTunnelDraft, ServerMonitorTunnelRecord,
    SessionLifetime,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;

const MAX_ACTIVE_COLLECTIONS: usize = 4;
pub(super) const COLLECTION_LIMIT_ERROR: &str = "Server monitor collection limit reached";

impl ServerMonitorRuntime {
    pub(super) fn begin_collection(
        self: &Arc<Self>,
        target: ServerMonitorTargetRecord,
        lifetime: SessionLifetime,
        mode: ServerMonitorScheduleMode,
        expected_interval_seconds: Option<u64>,
    ) -> Result<Arc<InFlight>, String> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err("Server monitor runtime is shutting down".to_string());
        }

        let (handle, session, generation, collected_at_ms) = {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            if !inner
                .config
                .targets
                .iter()
                .any(|candidate| candidate.id == target.id)
            {
                return Err("Server monitor target not found".to_string());
            }
            if let Some(existing) = inner.inflight.get(&target.id) {
                return Ok(existing.clone());
            }
            if inner.active >= MAX_ACTIVE_COLLECTIONS {
                return Err(COLLECTION_LIMIT_ERROR.to_string());
            }
            let request = connect_request(&target)?;
            let generation = inner.generation.get(&target.id).copied().unwrap_or(0);
            let session = match lifetime {
                SessionLifetime::Persistent => inner
                    .sessions
                    .get(&target.id)
                    .cloned()
                    .unwrap_or_else(|| {
                        Arc::new(MonitorSession::new(
                            self.connector.clone(),
                            request.clone(),
                            SessionLifetime::Persistent,
                        ))
                    }),
                SessionLifetime::OneShot => Arc::new(MonitorSession::new(
                    self.connector.clone(),
                    request.clone(),
                    SessionLifetime::OneShot,
                )),
            };
            if lifetime == SessionLifetime::Persistent {
                inner.sessions.insert(target.id.clone(), session.clone());
            }
            let owned = inner.owned_sessions.entry(target.id.clone()).or_default();
            if !owned
                .iter()
                .any(|existing| Arc::ptr_eq(existing, &session))
            {
                owned.push(session.clone());
            }
            inner.active += 1;
            let handle = Arc::new(InFlight::new());
            inner.inflight.insert(target.id.clone(), handle.clone());
            (handle, session, generation, self.clock.now_ms())
        };

        self.publish();

        let runtime = self.clone();
        let worker_handle = handle.clone();
        let worker_session = session.clone();
        std::thread::spawn(move || {
            let report = runtime.collect_and_store(
                &session,
                lifetime,
                mode,
                expected_interval_seconds,
                collected_at_ms,
            );
            runtime.finish_collection(
                &target.id,
                generation,
                worker_handle,
                report,
                &worker_session,
            );
        });
        Ok(handle)
    }

    fn collect_and_store(
        self: &Arc<Self>,
        session: &Arc<MonitorSession>,
        lifetime: SessionLifetime,
        mode: ServerMonitorScheduleMode,
        expected_interval_seconds: Option<u64>,
        collected_at_ms: i64,
    ) -> CollectionReport {
        if self.shutting_down.load(Ordering::SeqCst) {
            return CollectionReport::cancelled(collected_at_ms, mode);
        }
        let outcome = session.collect(None, collected_at_ms);
        let report = match outcome {
            Ok(CollectionOutcome::Success(snapshot)) => CollectionReport {
                snapshot: Some(snapshot.clone()),
                outcome: ServerMonitorOutcome::Success(snapshot),
                failure_kind: None,
                error: None,
                persistence_error: None,
                cancelled: false,
                collected_at_ms,
                source_mode: mode,
                expected_interval_seconds,
            },
            Ok(CollectionOutcome::Partial(snapshot)) => CollectionReport {
                snapshot: Some(snapshot.clone()),
                outcome: ServerMonitorOutcome::Partial(snapshot),
                failure_kind: None,
                error: None,
                persistence_error: None,
                cancelled: false,
                collected_at_ms,
                source_mode: mode,
                expected_interval_seconds,
            },
            Ok(CollectionOutcome::Failed(failure)) => CollectionReport {
                snapshot: None,
                outcome: ServerMonitorOutcome::Failed,
                failure_kind: Some(failure.kind),
                error: Some(failure.message),
                persistence_error: None,
                cancelled: false,
                collected_at_ms,
                source_mode: mode,
                expected_interval_seconds,
            },
            Err(CollectionError::Cancelled) => {
                CollectionReport::cancelled(collected_at_ms, mode)
            }
            Err(CollectionError::Transport(error)) => {
                let kind = transport_failure_kind(error.kind);
                CollectionReport {
                    snapshot: None,
                    outcome: ServerMonitorOutcome::Failed,
                    failure_kind: Some(kind),
                    error: Some(error.message),
                    persistence_error: None,
                    cancelled: false,
                    collected_at_ms,
                    source_mode: mode,
                    expected_interval_seconds,
                }
            }
        };
        if lifetime == SessionLifetime::OneShot {
            session.close();
        }
        report
    }

    fn finish_collection(
        self: &Arc<Self>,
        target_id: &str,
        generation: u64,
        handle: Arc<InFlight>,
        mut report: CollectionReport,
        session: &Arc<MonitorSession>,
    ) {
        let now = self.clock.now_ms();
        let mut prune: Option<(u32, i64, String)> = None;
        let mut failure_notification: Option<(String, String, String)> = None;
        let mut alert_notification: Option<(String, String, Vec<String>)> = None;

        if let Ok(mut inner) = self.inner.lock() {
            inner.active = inner.active.saturating_sub(1);
            inner.inflight.remove(target_id);
            let mut owned_empty = false;
            if let Some(sessions) = inner.owned_sessions.get_mut(target_id) {
                sessions.retain(|owned| !Arc::ptr_eq(owned, session));
                owned_empty = sessions.is_empty();
            }
            if owned_empty {
                inner.owned_sessions.remove(target_id);
            }
            let current_generation = inner.generation.get(target_id).copied().unwrap_or(0);
            let accepted = !report.cancelled
                && !self.shutting_down.load(Ordering::SeqCst)
                && current_generation == generation;
            if accepted {
                inner.error = None;
                let sample = report_sample(target_id, &report);
                if let Err(error) = self.store.append_sample(&sample) {
                    inner.error = Some(error.clone());
                    report.persistence_error = Some(error);
                }
                let mut config = inner.config.clone();
                if let Some(target) = config
                    .targets
                    .iter_mut()
                    .find(|target| target.id == target_id)
                {
                    if let Some(kind) = report.failure_kind {
                        target.blocked = kind.is_blocking();
                        target.last_error_kind = Some(kind);
                        target.last_error = report.error.clone();
                        let error_msg = report
                            .error
                            .clone()
                            .unwrap_or_else(|| "Unknown collection failure".to_string());
                        failure_notification =
                            Some((target.id.clone(), target.name.clone(), error_msg));
                    } else if report.snapshot.is_some() {
                        target.blocked = false;
                        target.last_error_kind = None;
                        target.last_error = None;
                        target.last_success_at_ms = Some(report.collected_at_ms);

                        if let Some(ref snapshot) = report.snapshot {
                            let (has_alert, reasons) = evaluate_alert(snapshot, &inner.config.settings);
                            if has_alert {
                                target.has_alert = true;
                                target.alert_reasons = reasons.clone();

                                let cooldown_ms = inner.config.settings.alert_cooldown_minutes as i64 * 60_000;
                                let should_notify = match target.last_alert_at_ms {
                                    Some(last_at) => now.saturating_sub(last_at) >= cooldown_ms,
                                    None => true,
                                };
                                if should_notify {
                                    target.last_alert_at_ms = Some(now);
                                    alert_notification = Some((target.id.clone(), target.name.clone(), reasons));
                                }
                            } else {
                                target.has_alert = false;
                                target.alert_reasons.clear();
                            }
                        } else {
                            target.has_alert = false;
                            target.alert_reasons.clear();
                        }
                    }
                    target.updated_at_ms = now;
                }
                match self.store.save_config(&config) {
                    Ok(()) => inner.config = config,
                    Err(error) => {
                        log::error!("Server monitor status could not be persisted: {error}");
                        inner.error = Some(error.clone());
                        report.persistence_error = Some(error);
                    }
                }
                let mode = inner.config.settings.schedule_mode;
                let auto_enabled = inner.config.settings.auto_collect_enabled;
                let blocked = inner
                    .config
                    .targets
                    .iter()
                    .find(|target| target.id == target_id)
                    .map(|target| target.blocked)
                    .unwrap_or(false);
                if mode == ServerMonitorScheduleMode::Interval && auto_enabled && !blocked {
                    let interval_ms = i64::from(inner.config.settings.interval_minutes) * 60_000;
                    inner
                        .interval_due
                        .insert(target_id.to_string(), now + interval_ms);
                } else {
                    inner.interval_due.remove(target_id);
                }
                let retention = inner.config.settings.retention_days;
                prune = Some((retention, now, today_string(now)));
            } else {
                // A stale generation, deleted target or shutdown result must not
                // publish, persist or let a manual refresh observe success.
                inner.sessions.remove(target_id);
                report.snapshot = None;
                report.outcome = ServerMonitorOutcome::Failed;
                report.failure_kind = None;
                report.error = Some(CANCELLED_MESSAGE.to_string());
                report.cancelled = true;
            }
        }
        if let Some((target_id, target_name, error_msg)) = failure_notification {
            self.events.notify_failure(&target_id, &target_name, &error_msg);
        }
        if let Some((target_id, target_name, reasons)) = alert_notification {
            self.events.notify_alert(&target_id, &target_name, &reasons);
        }
        handle.complete(report);
        if let Some((retention, now_ms, today)) = prune {
            let _ = self.store.prune(retention, now_ms, &today);
        }
        self.publish();
    }

    pub(crate) fn refresh_target(
        self: &Arc<Self>,
        target_id: &str,
    ) -> Result<ServerMonitorState, String> {
        let now = self.clock.now_ms();
        let (target, lifetime, mode, expected, daily_slot) = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            let target = inner
                .config
                .targets
                .iter()
                .find(|target| target.id == target_id)
                .cloned()
                .ok_or_else(|| "Server monitor target not found".to_string())?;
            let settings = inner.config.settings.clone();
            let persistent = settings.schedule_mode == ServerMonitorScheduleMode::Interval
                && settings.auto_collect_enabled
                && target.enabled;
            let daily_slot = if settings.schedule_mode == ServerMonitorScheduleMode::Daily
                && settings.auto_collect_enabled
                && target.enabled
            {
                let date = today_string(now);
                let armed_after = inner
                    .daily_armed_after_ms
                    .get(&target.id)
                    .copied();
                settings.daily_times.iter().find_map(|planned_time| {
                    let slot_ms = local_slot_ms(&date, planned_time)?;
                    if now < slot_ms || now >= slot_ms + super::scheduler::DAILY_SLOT_WINDOW_MS {
                        return None;
                    }
                    if !daily_slot_eligible(slot_ms, armed_after) {
                        return None;
                    }
                    Some((date.clone(), planned_time.clone()))
                })
            } else {
                None
            };
            (
                target,
                if persistent {
                    SessionLifetime::Persistent
                } else {
                    SessionLifetime::OneShot
                },
                settings.schedule_mode,
                match settings.schedule_mode {
                    ServerMonitorScheduleMode::Interval => {
                        Some(u64::from(settings.interval_minutes * 60))
                    }
                    ServerMonitorScheduleMode::Daily => None,
                },
                daily_slot,
            )
        };
        if let Some((date, planned_time)) = daily_slot {
            // A manual collection colliding with a due daily slot claims that
            // slot and its result is the one attempt for the target/date/time.
            self.claim_daily_slot(target_id, &date, &planned_time, now)?;
        }
        let handle = self.begin_collection(target, lifetime, mode, expected)?;
        let report = handle
            .wait()
            .ok_or_else(|| "Server monitor collection did not complete".to_string())?;
        if let Some(error) = report.persistence_error {
            return Err(error);
        }
        if report.cancelled {
            return Err("Server monitor collection was cancelled".to_string());
        }
        self.clear_error();
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn test_connection(
        &self,
        draft: ServerMonitorTargetDraft,
    ) -> Result<ServerMonitorTestResult, String> {
        let now = self.clock.now_ms();
        let existing = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            draft
                .id
                .as_ref()
                .filter(|id| !id.trim().is_empty())
                .and_then(|id| {
                    inner
                        .config
                        .targets
                        .iter()
                        .find(|target| &target.id == id)
                        .cloned()
                })
        };
        // A draft test resolves the stored password for an existing target and
        // validates the submitted connection fields without persisting anything.
        let record = draft_record(&draft, existing.as_ref(), now)?;
        validate_target(&record)?;
        let request = connect_request(&record)?;
        let session = MonitorSession::new(self.connector.clone(), request, SessionLifetime::OneShot);
        let outcome = session.collect(None, now);
        session.close();
        Ok(match outcome {
            Ok(CollectionOutcome::Success(snapshot)) => ServerMonitorTestResult {
                succeeded: true,
                partial: false,
                error_kind: None,
                error: None,
                snapshot: Some(snapshot),
            },
            Ok(CollectionOutcome::Partial(snapshot)) => ServerMonitorTestResult {
                succeeded: true,
                partial: true,
                error_kind: None,
                error: None,
                snapshot: Some(snapshot),
            },
            Ok(CollectionOutcome::Failed(failure)) => ServerMonitorTestResult {
                succeeded: false,
                partial: false,
                error_kind: Some(failure.kind),
                error: Some(failure.message),
                snapshot: None,
            },
            Err(CollectionError::Transport(error)) => ServerMonitorTestResult {
                succeeded: false,
                partial: false,
                error_kind: Some(transport_failure_kind(error.kind)),
                error: Some(error.message),
                snapshot: None,
            },
            Err(CollectionError::Cancelled) => ServerMonitorTestResult {
                succeeded: false,
                partial: false,
                error_kind: None,
                error: Some("Connection test was cancelled".to_string()),
                snapshot: None,
            },
        })
    }
}

pub(super) fn build_custom_record(
    draft: &ServerMonitorCustomDraft,
    existing: Option<&ServerMonitorTargetRecord>,
) -> ServerMonitorCustomRecord {
    let preserved_password = existing
        .and_then(|record| record.custom.as_ref())
        .and_then(|custom| custom.password.clone())
        .filter(|password| !password.is_empty());
    let password = if draft.auth_kind == crate::server_monitor::types::ServerMonitorAuthKind::Password
    {
        draft
            .password
            .clone()
            .filter(|password| !password.is_empty())
            .or(preserved_password)
    } else {
        None
    };
    ServerMonitorCustomRecord {
        host: draft.host.trim().to_string(),
        port: draft.port,
        user: draft.user.trim().to_string(),
        auth_kind: draft.auth_kind,
        key_path: draft
            .key_path
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        password,
    }
}

pub(super) fn build_tunnel_record(
    draft: &ServerMonitorTunnelDraft,
    existing: Option<&ServerMonitorTargetRecord>,
) -> ServerMonitorTunnelRecord {
    let preserved_password = existing
        .and_then(|record| record.tunnel.as_ref())
        .and_then(|tunnel| tunnel.password.clone())
        .filter(|password| !password.is_empty());
    let password = if draft.auth_kind == Some(crate::server_monitor::types::ServerMonitorAuthKind::Password) {
        draft
            .password
            .clone()
            .filter(|password| !password.is_empty())
            .or(preserved_password)
    } else {
        None
    };
    ServerMonitorTunnelRecord {
        enabled: draft.enabled,
        source_kind: draft.source_kind,
        saved_host_name: draft
            .saved_host_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        host: draft
            .host
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        port: draft.port,
        user: draft
            .user
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        auth_kind: draft.auth_kind,
        key_path: draft
            .key_path
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        password,
    }
}

fn draft_record(
    draft: &ServerMonitorTargetDraft,
    existing: Option<&ServerMonitorTargetRecord>,
    now: i64,
) -> Result<ServerMonitorTargetRecord, String> {
    Ok(ServerMonitorTargetRecord {
        id: draft
            .id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: draft.name.trim().to_string(),
        enabled: draft.enabled,
        source_kind: draft.source_kind,
        saved_host_name: draft
            .saved_host_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        custom: draft
            .custom
            .as_ref()
            .map(|custom| build_custom_record(custom, existing)),
        tunnel: draft
            .tunnel
            .as_ref()
            .map(|tunnel| build_tunnel_record(tunnel, existing)),
        created_at_ms: now,
        updated_at_ms: now,
        blocked: false,
        last_error_kind: None,
        last_error: None,
        last_success_at_ms: existing.and_then(|record| record.last_success_at_ms),
        has_alert: existing.map(|record| record.has_alert).unwrap_or(false),
        alert_reasons: existing
            .map(|record| record.alert_reasons.clone())
            .unwrap_or_default(),
        last_alert_at_ms: existing.and_then(|record| record.last_alert_at_ms),
    })
}

pub(super) fn validate_target(record: &ServerMonitorTargetRecord) -> Result<(), String> {
    let name = record.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("Server name must be 1 to 80 characters".to_string());
    }
    if record.id.trim().is_empty() {
        return Err("Server id is required".to_string());
    }
    match record.source_kind {
        ServerMonitorSourceKind::SavedHost => {
            if record
                .saved_host_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err("Select an existing SSH host".to_string());
            }
        }
        ServerMonitorSourceKind::Custom => {
            let custom = record
                .custom
                .as_ref()
                .ok_or_else(|| "Custom connection details are required".to_string())?;
            if custom.host.trim().is_empty() {
                return Err("SSH host is required".to_string());
            }
            if custom.user.trim().is_empty() {
                return Err("SSH user is required".to_string());
            }
            if custom.port == 0 {
                return Err("SSH port must be from 1 to 65535".to_string());
            }
            match custom.auth_kind {
                crate::server_monitor::types::ServerMonitorAuthKind::Key => {
                    if custom
                        .key_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        return Err("Key authentication requires a key path".to_string());
                    }
                }
                crate::server_monitor::types::ServerMonitorAuthKind::Password => {
                    if custom
                        .password
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        return Err("Password authentication requires a password".to_string());
                    }
                }
                crate::server_monitor::types::ServerMonitorAuthKind::Agent => {
                    return Err("Agent authentication requires a saved SSH host".to_string());
                }
            }
        }
    }
    if let Some(tunnel) = &record.tunnel {
        if tunnel.enabled {
            match tunnel.source_kind {
                ServerMonitorSourceKind::SavedHost => {
                    if tunnel
                        .saved_host_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        return Err("Select an existing SSH host for the tunnel".to_string());
                    }
                }
                ServerMonitorSourceKind::Custom => {
                    if tunnel.host.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_none() {
                        return Err("Tunnel SSH host is required".to_string());
                    }
                    if tunnel.user.as_deref().map(str::trim).filter(|v| !v.is_empty()).is_none() {
                        return Err("Tunnel SSH user is required".to_string());
                    }
                    if tunnel.port.unwrap_or(0) == 0 {
                        return Err("Tunnel SSH port must be from 1 to 65535".to_string());
                    }
                    match tunnel.auth_kind {
                        Some(crate::server_monitor::types::ServerMonitorAuthKind::Key) => {
                            if tunnel
                                .key_path
                                .as_deref()
                                .map(str::trim)
                                .filter(|v| !v.is_empty())
                                .is_none()
                            {
                                return Err("Tunnel key authentication requires a key path".to_string());
                            }
                        }
                        Some(crate::server_monitor::types::ServerMonitorAuthKind::Password) => {
                            if tunnel
                                .password
                                .as_deref()
                                .map(str::trim)
                                .filter(|v| !v.is_empty())
                                .is_none()
                            {
                                return Err("Tunnel password authentication requires a password".to_string());
                            }
                        }
                        Some(crate::server_monitor::types::ServerMonitorAuthKind::Agent) => {
                            return Err("Tunnel agent authentication requires a saved SSH host".to_string());
                        }
                        None => {
                            return Err("Tunnel authentication method is required".to_string());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn same_source(
    left: &ServerMonitorTargetRecord,
    right: &ServerMonitorTargetRecord,
) -> bool {
    if left.source_kind != right.source_kind || left.saved_host_name != right.saved_host_name {
        return false;
    }
    if left.tunnel != right.tunnel {
        return false;
    }
    match (&left.custom, &right.custom) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.host == right.host
                && left.port == right.port
                && left.user == right.user
                && left.auth_kind == right.auth_kind
                && left.key_path == right.key_path
                && left.password == right.password
        }
        _ => false,
    }
}

fn connect_request(target: &ServerMonitorTargetRecord) -> Result<MonitorConnectRequest, String> {
    let source = match target.source_kind {
        ServerMonitorSourceKind::SavedHost => MonitorSource::SavedHost {
            alias: target
                .saved_host_name
                .clone()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Saved SSH host alias is missing".to_string())?,
        },
        ServerMonitorSourceKind::Custom => {
            let custom = target
                .custom
                .as_ref()
                .ok_or_else(|| "Custom connection details are missing".to_string())?;
            let auth = match custom.auth_kind {
                crate::server_monitor::types::ServerMonitorAuthKind::Password => {
                    MonitorAuth::Password
                }
                crate::server_monitor::types::ServerMonitorAuthKind::Key => MonitorAuth::Key {
                    path: custom.key_path.clone().unwrap_or_default(),
                },
                crate::server_monitor::types::ServerMonitorAuthKind::Agent => MonitorAuth::Agent,
            };
            MonitorSource::Custom {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
            }
        }
    };
    Ok(MonitorConnectRequest {
        target_id: target.id.clone(),
        source,
        password: target
            .custom
            .as_ref()
            .and_then(|custom| custom.password.clone()),
        tunnel: target.tunnel.clone(),
    })
}

fn transport_failure_kind(kind: MonitorTransportErrorKind) -> CollectionFailureKind {
    match kind {
        MonitorTransportErrorKind::Transport => CollectionFailureKind::Transport,
        MonitorTransportErrorKind::Auth => CollectionFailureKind::Auth,
        MonitorTransportErrorKind::HostKey => CollectionFailureKind::HostKey,
        MonitorTransportErrorKind::Config => CollectionFailureKind::Config,
        MonitorTransportErrorKind::Timeout => CollectionFailureKind::Timeout,
    }
}
