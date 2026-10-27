//! Pool-only Chat Completions <-> Responses request/response conversion and the
//! portable request contract.
//!
//! The ordinary gateway forwards an OpenAI request to the candidate's own
//! endpoint and converts only OpenAI -> native Anthropic Messages. A model pool
//! may select a candidate whose stored endpoint differs from the client's, so
//! this module supplies the missing OpenAI <-> OpenAI translation together with
//! the portable-field contract that must be enforced *before* any candidate is
//! selected.
//!
//! This module never touches ordinary routing, key policy, health settlement or
//! Protocol Router; it only shapes pool request and response bodies. The
//! session-affinity layer, scheduler and streaming lifecycle stay in
//! `runtime_http.rs`.

mod helpers;
mod protocol;
mod request;
mod response;
mod validation;

pub(in crate::ai_gateway) use protocol::{
    client_protocol, endpoint_path, include_usage_requested, is_openai_path,
};
pub(in crate::ai_gateway) use request::convert_request;
pub(in crate::ai_gateway) use response::convert_response;
pub(in crate::ai_gateway) use validation::validate_pool_request;
