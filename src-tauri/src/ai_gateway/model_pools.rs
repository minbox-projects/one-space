//! Operator-defined model pools (Step 1 of 20261013-free-model-pool).
//!
//! A model pool is a virtual provider that exposes one public model id (`model`)
//! while routing each request, through the existing gateway attempt pipeline, to
//! exactly one of its explicitly selected source mappings. This module owns the
//! pool-specific decisions the ordinary pipeline does not:
//!
//! - pool alias resolution, including the fail-closed current/retired boundary;
//! - exact source-candidate resolution against the latest configuration;
//! - pool-scoped smooth weighted round robin isolated from the ordinary
//!   scheduler and from session affinity;
//! - a process-memory candidate cooldown keyed by pool plus full candidate
//!   identity, so a model-scoped limit never marks a shared provider key;
//! - nonsecret readiness status for the UI;
//! - atomic create/edit validation and the alias reservation bookkeeping.
//!
//! Transport, streaming, first-byte and cancellation semantics stay in
//! `runtime_http.rs`; key selection/retry/probe decisions stay in
//! `attempt_policy.rs`. This module only decides which real sources a pool may
//! use and how a candidate-scoped failure cools one of them.

mod candidate;
mod health;
mod planning;
mod scheduler;
mod status;
mod validation;

pub(in crate::ai_gateway) use candidate::*;
pub(in crate::ai_gateway) use health::*;
pub(in crate::ai_gateway) use planning::*;
#[cfg(test)]
pub(in crate::ai_gateway) use scheduler::{pool_provider_copy_count, reset_pool_provider_copy_count};
pub(in crate::ai_gateway) use validation::*;

#[allow(unused_imports)]
pub use status::{
    ModelPoolCandidateStatus, ModelPoolFallbackModelStatus, ModelPoolFallbackSourceStatus,
    ModelPoolReasoningCoverage, ModelPoolStatus,
};
pub(in crate::ai_gateway) use status::pool_status;
