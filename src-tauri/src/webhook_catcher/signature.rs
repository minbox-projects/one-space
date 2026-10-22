use std::collections::HashMap;
use super::types::DetectedSignature;

pub fn detect_signature(headers: &HashMap<String, String>) -> Option<DetectedSignature> {
    for (k, v) in headers {
        let key_lower = k.to_lowercase();
        if key_lower == "x-hub-signature-256" || key_lower == "x-hub-signature" {
            return Some(DetectedSignature {
                provider: "GitHub".to_string(),
                header_name: k.clone(),
                header_value: v.clone(),
            });
        }
        if key_lower == "stripe-signature" {
            return Some(DetectedSignature {
                provider: "Stripe".to_string(),
                header_name: k.clone(),
                header_value: v.clone(),
            });
        }
        if key_lower == "x-lark-signature" {
            return Some(DetectedSignature {
                provider: "Feishu / Lark".to_string(),
                header_name: k.clone(),
                header_value: v.clone(),
            });
        }
        if key_lower == "wechatpay-signature" {
            return Some(DetectedSignature {
                provider: "WeChat Pay".to_string(),
                header_name: k.clone(),
                header_value: v.clone(),
            });
        }
        if key_lower == "x-shopify-hmac-sha256" {
            return Some(DetectedSignature {
                provider: "Shopify".to_string(),
                header_name: k.clone(),
                header_value: v.clone(),
            });
        }
        if key_lower == "x-slack-signature" {
            return Some(DetectedSignature {
                provider: "Slack".to_string(),
                header_name: k.clone(),
                header_value: v.clone(),
            });
        }
    }
    None
}

pub fn detect_body_format(content_type: &str, body: &str) -> String {
    let ct = content_type.to_lowercase();
    if ct.contains("application/json") || ct.contains("+json") {
        return "json".to_string();
    }
    if ct.contains("application/x-www-form-urlencoded") {
        return "form".to_string();
    }
    if ct.contains("text/xml") || ct.contains("application/xml") {
        return "xml".to_string();
    }
    let trimmed = body.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return "json".to_string();
    }
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        return "xml".to_string();
    }
    "raw".to_string()
}
