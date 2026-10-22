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

            if state.current_status != MonitorStatus::Down
                && state.consecutive_failures >= effective_retry
            {
                state.current_status = MonitorStatus::Down;
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

            if state.current_status == MonitorStatus::Down {
                if state.consecutive_successes >= effective_recover {
                    state.current_status = record.status;
                    return Some(StateTransition::BecameUp {
                        monitor_id: record.monitor_id.clone(),
                        timestamp: record.timestamp,
                    });
                }
            } else {
                state.current_status = record.status;
            }
        }

        None
    }
}
