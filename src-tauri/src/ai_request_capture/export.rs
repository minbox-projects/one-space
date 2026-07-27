use super::{
    enrichment, AiRequestCaptureCurlResult, AiRequestCaptureDetail, AiRequestCaptureExportInput,
    AiRequestCaptureExportResult, AiRequestCaptureHeader, CaptureState, CaptureStore,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub(crate) fn export_har(
    store: &CaptureStore,
    input: AiRequestCaptureExportInput,
) -> Result<AiRequestCaptureExportResult, String> {
    let records = store.finished_for_export(input.query)?;
    let document = har_document(&records)?;
    let path = Path::new(&input.output_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(AiRequestCaptureExportResult {
        output_path: input.output_path,
        exported: records.len() as u64,
    })
}

pub(crate) fn har_document(records: &[AiRequestCaptureDetail]) -> Result<Value, String> {
    Ok(json!({
        "log": {
            "version": "1.2",
            "creator": { "name": "OneSpace AI Request Capture", "version": env!("CARGO_PKG_VERSION") },
            "entries": records.iter().map(har_entry).collect::<Vec<_>>(),
        }
    }))
}

pub(crate) fn curl_command(record: &AiRequestCaptureDetail) -> AiRequestCaptureCurlResult {
    let complete = record.state == CaptureState::Completed
        && record.error.is_none()
        && !record.request_body.truncated
        && !record.response_body.truncated;
    let warning = (!complete).then(|| {
        "Captured request is incomplete because the transfer failed or a body sample was truncated; this cURL may not replay the original request.".to_string()
    });
    let mut curl = format!("curl -X {}", quote(&record.method));
    for (name, value) in curl_headers(&record.request_headers) {
        curl.push_str(" -H ");
        curl.push_str(&quote(&format!("{name}: {value}")));
    }

    let request_body =
        enrichment::body_representation(&record.request_headers, &record.request_body);
    if record.request_body.data.is_empty() {
        curl.push(' ');
        curl.push_str(&quote(&record.upstream_url));
    } else if request_body.encoding.is_some() {
        let bytes = record
            .request_body
            .data
            .iter()
            .map(|byte| format!("\\{byte:03o}"))
            .collect::<String>();
        curl = format!(
            "printf '%b' {} | {curl} --data-binary @- {}",
            quote(&bytes),
            quote(&record.upstream_url)
        );
    } else {
        curl.push_str(" --data-binary ");
        curl.push_str(&quote(&request_body.data));
        curl.push(' ');
        curl.push_str(&quote(&record.upstream_url));
    }
    let command = warning
        .as_deref()
        .map(|warning| format!("# WARNING: {warning}\n{curl}"))
        .unwrap_or(curl);
    AiRequestCaptureCurlResult {
        command,
        complete,
        warning,
    }
}

fn har_entry(record: &AiRequestCaptureDetail) -> Value {
    let request_body =
        enrichment::body_representation(&record.request_headers, &record.request_body);
    let response_body =
        enrichment::body_representation(&record.response_headers, &record.response_body);
    let request_content_type = content_type(&record.request_headers);
    let response_content_type = content_type(&record.response_headers);
    let mut request = json!({
        "method": record.method,
        "url": record.upstream_url,
        "httpVersion": record.http_version,
        "headers": har_headers(&record.request_headers),
        "queryString": har_query(&record.upstream_url),
        "cookies": [],
        "headersSize": -1,
        "bodySize": record.request_body.total_bytes,
    });
    if !record.request_body.data.is_empty() {
        request["postData"] = json!({
            "mimeType": request_content_type,
            "text": request_body.data,
            "encoding": request_body.encoding,
        });
    }
    let mut response = json!({
        "status": record.response_status.unwrap_or(0),
        "statusText": "",
        "httpVersion": record.http_version,
        "headers": har_headers(&record.response_headers),
        "cookies": [],
        "content": {
            "size": record.response_body.total_bytes,
            "mimeType": response_content_type,
            "text": response_body.data,
            "encoding": response_body.encoding,
        },
        "redirectURL": "",
        "headersSize": -1,
        "bodySize": record.response_body.total_bytes,
    });
    let comment = record_comment(record);
    if !comment.is_empty() {
        request["comment"] = Value::String(comment.clone());
        response["comment"] = Value::String(comment.clone());
    }
    json!({
        "startedDateTime": chrono::DateTime::from_timestamp_millis(record.started_at)
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| record.started_at.to_string()),
        "time": record.duration_ms.unwrap_or_default(),
        "request": request,
        "response": response,
        "cache": {},
        "timings": { "blocked": -1, "dns": -1, "connect": -1, "send": -1, "wait": record.duration_ms.unwrap_or_default(), "receive": -1, "ssl": -1 },
        "comment": comment,
        "_onespace": {
            "state": record.state,
            "error": record.error,
            "request": body_metadata(&record.request_body),
            "response": body_metadata(&record.response_body),
        },
    })
}

fn body_metadata(body: &super::CapturedBody) -> Value {
    json!({
        "capturedBytes": body.captured_bytes,
        "totalBytes": body.total_bytes,
        "truncated": body.truncated,
    })
}

fn har_headers(headers: &[AiRequestCaptureHeader]) -> Vec<Value> {
    headers
        .iter()
        .flat_map(|header| {
            header
                .values
                .iter()
                .map(|value| json!({ "name": header.name, "value": value }))
        })
        .collect()
}

fn har_query(upstream_url: &str) -> Vec<Value> {
    url::Url::parse(upstream_url)
        .ok()
        .map(|url| {
            url.query_pairs()
                .map(|(name, value)| json!({ "name": name, "value": value }))
                .collect()
        })
        .unwrap_or_default()
}

fn content_type(headers: &[AiRequestCaptureHeader]) -> String {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .and_then(|header| header.values.first())
        .cloned()
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

fn curl_headers(headers: &[AiRequestCaptureHeader]) -> Vec<(String, String)> {
    let mut excluded = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "host",
        "content-length",
        "accept-encoding",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<HashSet<_>>();
    for header in headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case("connection"))
    {
        for value in &header.values {
            excluded.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(|name| name.to_ascii_lowercase()),
            );
        }
    }
    let mut result = headers
        .iter()
        .filter(|header| !excluded.contains(&header.name.to_ascii_lowercase()))
        .flat_map(|header| {
            header
                .values
                .iter()
                .map(|value| (header.name.clone(), value.clone()))
        })
        .collect::<Vec<_>>();
    result.push(("accept-encoding".to_string(), "identity".to_string()));
    result
}

fn record_comment(record: &AiRequestCaptureDetail) -> String {
    let mut notes = Vec::new();
    if record.request_body.truncated {
        notes.push("request body capture truncated".to_string());
    }
    if record.response_body.truncated {
        notes.push("response body capture truncated".to_string());
    }
    if record.state != CaptureState::Completed {
        notes.push(format!("capture state: {}", record.state.as_str()));
    }
    if let Some(error) = &record.error {
        notes.push(format!("transfer error: {error}"));
    }
    notes.join("; ")
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
