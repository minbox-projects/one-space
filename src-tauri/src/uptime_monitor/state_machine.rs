use std::collections::HashMap;
use std::sync::Mutex;

use super::types::{HeartbeatRecord, MonitorStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateTransition {
    BecameDown {
        monitor_id: String,
        cause: String,
        timestamp: i64,
    },
    BecameUp {
        monitor_id: String,
        timestamp: i64,
    },
}

#[derive(Debug, Clone)]
pub struct MonitorRuntimeState {
    pub current_status: MonitorStatus,
    pub consecutive_failures: u8,
    pub consecutive_successes: u8,
    pub last_checked_at: Option<i64>,
    pub last_response_time_ms: Option<u32>,
    pub last_error_message: Option<String>,
    /// 是否正处于已被告警通知的故障周期中（防止故障持续期间任何重复报警）
    pub is_incident_alerted: bool,
    /// 上次触发告警的时间戳
    pub last_alerted_at: Option<i64>,
}

impl Default for MonitorRuntimeState {
    fn default() -> Self {
        Self {
            current_status: MonitorStatus::Unknown,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_checked_at: None,
            last_response_time_ms: None,
            last_error_message: None,
            is_incident_alerted: false,
            last_alerted_at: None,
        }
    }
}

pub struct StateMachineRegistry {
    states: Mutex<HashMap<String, MonitorRuntimeState>>,
}

impl StateMachineRegistry {
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_state(&self, monitor_id: &str) -> MonitorRuntimeState {
        let states = self.states.lock().unwrap();
        states.get(monitor_id).cloned().unwrap_or_default()
    }

    pub fn reset_monitor(&self, monitor_id: &str) {
        let mut states = self.states.lock().unwrap();
        states.remove(monitor_id);
    }

    /// 应用启动或恢复时，从已存在的未解决故障中预防水合状态，避免重复告警
    pub fn hydrate_active_incident(&self, monitor_id: &str, started_at: i64, cause: &str) {
        let mut states = self.states.lock().unwrap();
        let state = states.entry(monitor_id.to_string()).or_default();
        state.current_status = MonitorStatus::Down;
        state.is_incident_alerted = true;
        state.last_alerted_at = Some(started_at);
        state.last_error_message = Some(cause.to_string());
    }

    pub fn process_heartbeat(
        &self,
        record: &HeartbeatRecord,
        retry_count: u8,
        recover_count: u8,
    ) -> Option<StateTransition> {
        let mut states = self.states.lock().unwrap();
        let state = states.entry(record.monitor_id.clone()).or_default();

        state.last_checked_at = Some(record.timestamp);
        state.last_response_time_ms = Some(record.response_time_ms);
        state.last_error_message = record.error_message.clone();

        let is_failed = record.status == MonitorStatus::Down;
        let effective_retry = retry_count.max(1);
        let effective_recover = recover_count.max(1);

        if is_failed {
            state.consecutive_failures += 1;
            state.consecutive_successes = 0;

            // 核心保障：只有在当前未处于已被告警的持续故障中，且连续失败次数达到阈值时，才触发一次 BecameDown
            if state.current_status != MonitorStatus::Down
                && !state.is_incident_alerted
                && state.consecutive_failures >= effective_retry
            {
                state.current_status = MonitorStatus::Down;
                state.is_incident_alerted = true;
                state.last_alerted_at = Some(record.timestamp);
                return Some(StateTransition::BecameDown {
                    monitor_id: record.monitor_id.clone(),
                    cause: record
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown failure".to_string()),
                    timestamp: record.timestamp,
                });
            }
        } else {
            state.consecutive_successes += 1;
            state.consecutive_failures = 0;

            // 只有此前发生过故障（状态为 Down 或存在未解决 incident），且连续成功达到恢复阈值时，才触发一次 BecameUp
            if (state.current_status == MonitorStatus::Down || state.is_incident_alerted)
                && state.consecutive_successes >= effective_recover
            {
                state.current_status = record.status;
                state.is_incident_alerted = false;
                state.last_alerted_at = Some(record.timestamp);
                return Some(StateTransition::BecameUp {
                    monitor_id: record.monitor_id.clone(),
                    timestamp: record.timestamp,
                });
            } else if state.current_status != MonitorStatus::Down && !state.is_incident_alerted {
                state.current_status = record.status;
            }
        }

        None
    }
}
