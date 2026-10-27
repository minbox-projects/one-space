//! Extraction, redaction and bounding of upstream error text.

/// Maximum stored length of one sanitized upstream error message.
const MAX_ERROR_MESSAGE_CHARS: usize = 4096;
/// Marker appended when an error message had to be truncated.
const ERROR_TRUNCATION_MARKER: char = '…';
/// Credential-shaped runs shorter than this are left readable.
const MIN_MASKED_CREDENTIAL_CHARS: usize = 8;

/// Extract the upstream's own error information from a failure body.
///
/// A standard JSON envelope yields its non-empty `error.message`; any other body
/// yields a readable summary (lossy decode, markup removed, consecutive
/// whitespace collapsed, trimmed). Bodies without readable text yield `None`.
pub(in crate::ai_gateway) fn extract_upstream_error_text(body: &[u8]) -> Option<String> {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
        let message = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|message| !message.is_empty());
        if let Some(message) = message {
            return Some(message.to_string());
        }
    }
    let decoded = String::from_utf8_lossy(body);
    let summary = strip_html_markup(&decoded)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if summary.is_empty() {
        None
    } else {
        Some(summary)
    }
}

/// Drop every `<...>` markup span so only the readable text remains. An
/// unmatched `<` is kept literally instead of swallowing the rest of the body.
fn strip_html_markup(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        result.push_str(&rest[..start]);
        match rest[start..].find('>') {
            Some(end) => rest = &rest[start + end + 1..],
            None => {
                result.push_str(&rest[start..]);
                return result;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Redact, bound and normalize one extracted upstream error message.
///
/// The provider key is replaced verbatim with `[redacted]`, `Bearer` and `sk-`
/// credential shapes are masked, and text longer than
/// [`MAX_ERROR_MESSAGE_CHARS`] is truncated on a Unicode scalar boundary with a
/// trailing `…`. Text that stays empty is stored as no message (`None`).
pub(in crate::ai_gateway) fn sanitize_error_text(text: &str, api_key: &str) -> Option<String> {
    let redacted = if api_key.is_empty() {
        text.to_string()
    } else {
        text.replace(api_key, "[redacted]")
    };
    let masked = mask_credential_shapes(&redacted);
    let trimmed = masked.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= MAX_ERROR_MESSAGE_CHARS {
        return Some(trimmed.to_string());
    }
    let mut bounded: String = trimmed.chars().take(MAX_ERROR_MESSAGE_CHARS - 1).collect();
    bounded.push(ERROR_TRUNCATION_MARKER);
    Some(bounded)
}

/// Mask `sk-` prefixed tokens and `Bearer <token>` credentials so no complete
/// token can reach the log. Only ASCII credential characters join a token, so
/// multi-byte text can never be split by the masking.
fn mask_credential_shapes(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut masked = String::with_capacity(text.len());
    let mut copied = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let mut replacement_start = cursor;
        let mut replacement_end = cursor;
        if bytes[cursor..].starts_with(b"sk-") {
            replacement_end = credential_run_end(bytes, cursor + 3).unwrap_or(cursor);
        } else if starts_with_ignore_ascii_case(bytes, cursor, b"bearer") {
            let mut token_start = cursor + b"bearer".len();
            while token_start < bytes.len() && bytes[token_start].is_ascii_whitespace() {
                token_start += 1;
            }
            if token_start > cursor + b"bearer".len() {
                if let Some(end) = credential_run_end(bytes, token_start) {
                    replacement_start = token_start;
                    replacement_end = end;
                }
            }
        }
        if replacement_end > replacement_start {
            masked.push_str(&text[copied..replacement_start]);
            masked.push_str("[redacted]");
            copied = replacement_end;
            cursor = replacement_end;
        } else {
            cursor += 1;
        }
    }
    masked.push_str(&text[copied..]);
    masked
}

/// End of the credential-character run starting at `start`, when it is long
/// enough to be a token rather than an ordinary word.
fn credential_run_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start;
    while end < bytes.len() && is_credential_byte(bytes[end]) {
        end += 1;
    }
    if end - start >= MIN_MASKED_CREDENTIAL_CHARS {
        Some(end)
    } else {
        None
    }
}

fn is_credential_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

fn starts_with_ignore_ascii_case(bytes: &[u8], start: usize, word: &[u8]) -> bool {
    bytes.len() - start >= word.len()
        && bytes[start..start + word.len()]
            .iter()
            .zip(word)
            .all(|(byte, expected)| byte.eq_ignore_ascii_case(expected))
}
