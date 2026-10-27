//! Shared field validators and error formatting for the portable pool contract.

use serde_json::{Map, Value};

pub(super) fn field_error(field: impl AsRef<str>, message: impl Into<String>) -> String {
    format!("field '{}': {}", field.as_ref(), message.into())
}

pub(super) fn nonempty(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(_) => true,
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

pub(super) fn present_nonnull(object: &Map<String, Value>, field: &str) -> bool {
    object.get(field).is_some_and(|value| !value.is_null())
}

/// A required, non-blank string field.
pub(super) fn required_str<'a>(value: Option<&'a Value>, field: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| field_error(field, "is required and must be a non-empty string"))
}

/// Validate a boolean-valued field, rejecting any non-boolean supplied value.
pub(super) fn validate_bool(object: &Map<String, Value>, field: &str) -> Result<(), String> {
    if let Some(value) = object.get(field).filter(|value| !value.is_null()) {
        if !value.is_boolean() {
            return Err(field_error(field, "must be a boolean"));
        }
    }
    Ok(())
}
