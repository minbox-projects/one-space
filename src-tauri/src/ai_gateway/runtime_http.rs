mod attempts;
mod errors;
mod events;
mod http_io;
mod non_streaming;
mod routing;
mod server;
mod streaming;
mod usage;

pub(super) use super::anthropic::{
    convert_response, error_payload as anthropic_error_payload, is_conversion_path,
    is_messages_path, stream_error_event, validate_conversion_request,
};
pub(super) use super::anthropic_sse::{AnthropicSseConverter, AnthropicSseObserver};
pub(super) use super::attempt_policy::{
    all_unavailable_message, clear_ttl_expired_key_runtime_state, excluded_key_summary,
    key_failure_kind, persist_key_runtime_state, record_provider_failure,
    record_provider_failure_if_absent, sanitize_provider_error_text, select_attempt_key,
    settle_failure, settle_key_failure, settle_key_probe_failure, settle_key_probe_success,
    settle_success, sync_key_runtime_marks, AttemptResult, KeyAuthFailedTransition,
    MappingAutoDisabledTransition, RequestHealth, RetryCandidate,
};
pub(super) use super::forwarding::{
    forward_non_streaming_with_protocol, open_streaming_response_with_protocol,
};
pub(super) use super::model_pools::{
    cool_pool_candidate, expand_fallback_model_in, failure_label, plan_pool_request,
    pool_candidate_scope_failure, PoolFallbackStage, PoolPlan,
};
pub(super) use super::openai_conversion::{self, include_usage_requested};
pub(super) use super::openai_sse::OpenAiSseConverter;
pub(super) use super::output_timing::OutputTimingObserver;
pub(super) use super::pool_context_hold;
pub(super) use super::reasoning::{
    parse_pool_request_effort, resolved_effort_token, strip_pool_entry_controls, PoolOutputCap,
    PoolReasoningRequest,
};
pub(super) use super::selection::{
    candidate_providers, classify_failure_with_message, clear_key_runtime_state,
    default_retry_delay, default_retry_delay_with_base,
    find_probe_candidate_with_settings, is_quota_exceeded_message,
    is_retryable_with_message, mark_key_failure, rearm_key_probe, resolve_model_for_protocol,
    resolve_session_id_with_custom, resolve_upstream_protocol, retry_header_delay, select_usable_key,
    session_affinity, try_acquire_probe_guard, weighted_candidates, FailureClass, ModelResolution,
    ProbeCandidate, ProbeGuard, SessionOrder,
};
pub(super) use super::storage::{local_base_url, read_config_shared};
pub(super) use super::usage_log::{
    compute_cost_at_time_with_breakdown, extract_upstream_error_text, match_price_for_provider,
    normalize_retention_days, now_millis, parse_usage_from_response, CanonicalUsage,
    NativeReasoningSnapshot, SseUsageAccumulator, UsageAccounting, UsageLogEntry, UsageLogRecord,
    UsageLogStore, UsageResult, UsageTokens,
};
pub(super) use super::{
    now_ts, GatewayConfig, GatewayKey, GatewaySettings, GatewayStatus, GatewayUpstreamProvider, ModelPool,
    PoolCandidateIdentity, PoolReasoningEffort, UpstreamProtocol,
};
pub(super) use futures_util::StreamExt;
pub(super) use serde_json::{json, Value};
pub(super) use std::collections::{HashMap, HashSet, VecDeque};
pub(super) use std::io::ErrorKind;
pub(super) use std::sync::atomic::{AtomicU64, Ordering};
pub(super) use std::sync::OnceLock;
pub(super) use std::time::{Duration, SystemTime};
pub(super) use tauri::Emitter;
pub(super) use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub(super) use tokio::net::{TcpListener, TcpStream};
pub(super) use tokio::sync::{oneshot, Mutex};
pub(super) use tokio::time::Instant;
pub(super) use super::{
    AI_GATEWAY_CONFIG_UPDATED_EVENT, AI_GATEWAY_KEY_AUTH_FAILED_EVENT,
    GatewayKeyAuthFailedPayload,
};
pub(super) use super::reasoning;

pub(in crate::ai_gateway) use attempts::{AttemptLog, ForwardCapture};
pub(in crate::ai_gateway) use errors::format_network_error_reason;
pub(in crate::ai_gateway) use events::{
    emit_config_updated, emit_key_auth_failed, emit_mapping_auto_disabled,
};
#[cfg(test)]
pub(in crate::ai_gateway) use events::{
    CONFIG_UPDATE_EVENTS, KEY_AUTH_FAILED_EVENTS, KEY_AUTH_FAILED_MESSAGE_INPUTS,
    KEY_AUTH_FAILED_PAYLOADS, MAPPING_AUTO_DISABLED_MESSAGE_INPUTS,
};
#[allow(unused_imports)]
pub(in crate::ai_gateway) use http_io::{
    find_header_end, http_response_bytes, json_response, read_http_request, reason_for_status,
    HttpRequest, HttpResponse,
};
#[allow(unused_imports)]
pub(in crate::ai_gateway) use routing::{
    handle_connection, is_authorized, local_model_names, models_payload,
};
#[allow(unused_imports)]
pub(in crate::ai_gateway) use server::{
    autostart, run_server, server_status, start_server, state_lock, status_from_config,
    stop_server, stop_server_now, RunningServer, RUNNING_SERVER,
};
#[cfg(test)]
#[allow(unused_imports)]
pub(in crate::ai_gateway) use server::{force_next_accept_failure, retire_listener_epoch};
#[cfg(test)]
pub(in crate::ai_gateway) use non_streaming::attempt_non_streaming;
#[cfg(test)]
pub(in crate::ai_gateway) use streaming::attempt_streaming;
