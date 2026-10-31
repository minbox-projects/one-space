//! Loopback relay primitives for the OpenCode Zen sidecar.
//!
//! The sidecar validates a per-process token, restricts the upstream base to
//! the Zen allowlist, and forwards request bytes with a BoringSSL client. This
//! module holds the pure helpers shared with the binary and its tests.

/// Maximum accepted request body size.
pub const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Header carrying the per-process sidecar token.
pub const SIDECAR_TOKEN_HEADER: &str = "x-onespace-sidecar-token";

/// Header carrying the upstream base URL.
pub const UPSTREAM_BASE_HEADER: &str = "x-onespace-upstream-base";

/// Default upstream read/connect timeout in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// BoringSSL cipher list accepted by the Zen free tier.
pub const TLS_CIPHER_LIST: &str = "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES128-SHA:ECDHE-ECDSA-AES256-SHA:ECDHE-RSA-AES256-SHA:AES128-GCM-SHA256:AES256-GCM-SHA384:AES128-SHA:AES256-SHA";

/// TLS curves used by the upstream client.
pub const TLS_CURVES: &str = "X25519:P-256:P-384";

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-connection",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// Join a validated upstream base with an incoming path and query.
///
/// A duplicated version segment is removed when the base already ends with
/// `/v1`, so `/v1/chat/completions` is not appended twice.
pub fn join_upstream_url(base: &str, path_and_query: &str) -> Result<String, String> {
    let base = base.trim_end_matches('/');
    let path_and_query = path_and_query.trim();
    if path_and_query.is_empty() {
        return Err("request path is empty".to_string());
    }
    if !path_and_query.starts_with('/') {
        return Err("request path must start with '/'".to_string());
    }

    let path = if base.ends_with("/v1") && (path_and_query == "/v1" || path_and_query.starts_with("/v1/") || path_and_query.starts_with("/v1?")) {
        &path_and_query[3..]
    } else {
        path_and_query
    };

    if path.is_empty() {
        Ok(base.to_string())
    } else {
        Ok(format!("{base}{path}"))
    }
}

/// Validate that an upstream base is an HTTPS OpenCode endpoint.
///
/// Only the exact `opencode.ai` host with a `/zen` or `/inference/openai` path
/// is accepted. Ports, user information, other hosts, plain HTTP, queries,
/// fragments and unrelated paths are rejected.
pub fn validate_upstream_base(base: &str) -> Result<(), String> {
    let base = base.trim();
    if base.is_empty() {
        return Err("upstream base is empty".to_string());
    }
    if base.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("upstream base contains whitespace or control characters".to_string());
    }
    let rest = base
        .strip_prefix("https://")
        .ok_or_else(|| "upstream base must use https".to_string())?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    if authority != "opencode.ai" {
        return Err("upstream host is not allowed".to_string());
    }
    if path.is_empty() || path.contains('?') || path.contains('#') {
        return Err("upstream base path is invalid".to_string());
    }
    if path == "/zen"
        || path.starts_with("/zen/")
        || path == "/inference/openai"
        || path.starts_with("/inference/openai/")
    {
        Ok(())
    } else {
        Err("upstream base path is not allowed".to_string())
    }
}

/// Whether a client request header may be forwarded upstream.
pub fn is_forwardable_request_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if HOP_BY_HOP_HEADERS.contains(&lower.as_str()) {
        return false;
    }
    !matches!(
        lower.as_str(),
        "host"
            | "content-length"
            | "accept-encoding"
            | "x-onespace-sidecar-token"
            | "x-onespace-upstream-base"
    )
}

/// Whether an upstream response header may be relayed to the client.
///
/// `content-encoding` is intentionally preserved because the body is relayed
/// without decompression.
pub fn is_forwardable_response_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !HOP_BY_HOP_HEADERS.contains(&lower.as_str()) && lower != "content-length"
}

/// Compare the expected token with an optional provided value.
pub fn token_matches(expected: &str, provided: Option<&str>) -> bool {
    match provided {
        Some(value) => !expected.is_empty() && expected.len() == value.len() && expected.as_bytes() == value.as_bytes(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_paths_without_duplicating_the_version_segment() {
        assert_eq!(
            join_upstream_url("https://opencode.ai/zen/v1", "/v1/chat/completions?x=1").unwrap(),
            "https://opencode.ai/zen/v1/chat/completions?x=1"
        );
        assert_eq!(
            join_upstream_url("https://opencode.ai/zen/v1/", "/v1?x=1").unwrap(),
            "https://opencode.ai/zen/v1?x=1"
        );
        assert_eq!(
            join_upstream_url("https://opencode.ai/zen", "/v1/chat/completions").unwrap(),
            "https://opencode.ai/zen/v1/chat/completions"
        );
        assert!(join_upstream_url("https://opencode.ai/zen/v1", "").is_err());
    }

    #[test]
    fn inference_openai_upstream_base_accepted() {
        assert!(validate_upstream_base("https://opencode.ai/inference/openai/v1").is_ok());
        assert_eq!(
            join_upstream_url("https://opencode.ai/inference/openai/v1", "/v1/chat/completions").unwrap(),
            "https://opencode.ai/inference/openai/v1/chat/completions"
        );
        // legacy /zen still accepted
        assert!(validate_upstream_base("https://opencode.ai/zen/v1").is_ok());
        // non-opencode hosts and unrelated inference/anthropic paths rejected
        assert!(validate_upstream_base("https://evil.example.com/inference/openai/v1").is_err());
        assert!(validate_upstream_base("https://opencode.ai/inference/anthropic/v1").is_err());
    }
}
