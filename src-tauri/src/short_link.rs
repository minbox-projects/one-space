use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

const TINYURL_CREATE_URL: &str = "https://api.tinyurl.com/create";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortLinkErrorCode {
    NotConfigured,
    InvalidUrl,
    AuthenticationFailed,
    RateLimited,
    RequestRejected,
    ServiceUnavailable,
    NetworkError,
    InvalidResponse,
    StorageError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortLinkError {
    code: ShortLinkErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl ShortLinkError {
    fn new(code: ShortLinkErrorCode, message: &'static str) -> Self {
        Self {
            code,
            message: Some(message.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortLinkConfigStatus {
    configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortLinkResult {
    long_url: String,
    short_url: String,
}

#[derive(Serialize)]
struct TinyUrlCreateRequest<'a> {
    url: &'a str,
}

#[derive(Deserialize)]
struct TinyUrlCreateResponse {
    data: Option<TinyUrlCreateData>,
}

#[derive(Deserialize)]
struct TinyUrlCreateData {
    tiny_url: Option<String>,
}

fn invalid_url_error() -> ShortLinkError {
    ShortLinkError::new(
        ShortLinkErrorCode::InvalidUrl,
        "The URL must be an absolute HTTP or HTTPS URL with a host.",
    )
}

fn validate_http_url(value: &str) -> Result<(), ShortLinkError> {
    let parsed = Url::parse(value).map_err(|_| invalid_url_error())?;
    if !matches!(parsed.scheme(), "http" | "https") || !parsed.has_host() {
        return Err(invalid_url_error());
    }
    Ok(())
}

fn storage_error() -> ShortLinkError {
    ShortLinkError::new(
        ShortLinkErrorCode::StorageError,
        "The encrypted credential store is unavailable.",
    )
}

fn status_with_reader<F>(read_token: F) -> Result<ShortLinkConfigStatus, ShortLinkError>
where
    F: FnOnce() -> Result<Option<String>, String>,
{
    let token = read_token().map_err(|_| storage_error())?;
    Ok(ShortLinkConfigStatus {
        configured: token.is_some_and(|value| !value.trim().is_empty()),
    })
}

fn save_with_writer<F>(
    token: String,
    write_token: F,
) -> Result<ShortLinkConfigStatus, ShortLinkError>
where
    F: FnOnce(String) -> Result<(), String>,
{
    let token = token.trim();
    if token.is_empty() {
        return Err(ShortLinkError::new(
            ShortLinkErrorCode::NotConfigured,
            "The TinyURL API token cannot be empty.",
        ));
    }
    write_token(token.to_string()).map_err(|_| storage_error())?;
    Ok(ShortLinkConfigStatus { configured: true })
}

fn delete_with_writer<F>(delete_token: F) -> Result<ShortLinkConfigStatus, ShortLinkError>
where
    F: FnOnce() -> Result<(), String>,
{
    delete_token().map_err(|_| storage_error())?;
    Ok(ShortLinkConfigStatus { configured: false })
}

fn map_http_status(status: StatusCode) -> ShortLinkError {
    let (code, message) = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => (
            ShortLinkErrorCode::AuthenticationFailed,
            "TinyURL rejected the configured credentials.",
        ),
        StatusCode::TOO_MANY_REQUESTS => (
            ShortLinkErrorCode::RateLimited,
            "TinyURL rate limited the request.",
        ),
        status if status.is_client_error() => (
            ShortLinkErrorCode::RequestRejected,
            "TinyURL rejected the request.",
        ),
        _ => (
            ShortLinkErrorCode::ServiceUnavailable,
            "TinyURL is temporarily unavailable.",
        ),
    };
    ShortLinkError::new(code, message)
}

async fn create_with_dependencies<F>(
    url: String,
    read_token: F,
    client: &Client,
    endpoint: &str,
) -> Result<ShortLinkResult, ShortLinkError>
where
    F: FnOnce() -> Result<Option<String>, String>,
{
    validate_http_url(&url)?;

    let token = read_token()
        .map_err(|_| storage_error())?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ShortLinkError::new(
                ShortLinkErrorCode::NotConfigured,
                "A TinyURL API token has not been configured.",
            )
        })?;

    let response = client
        .post(endpoint)
        .bearer_auth(token.trim())
        .json(&TinyUrlCreateRequest { url: &url })
        .send()
        .await
        .map_err(|_| {
            ShortLinkError::new(
                ShortLinkErrorCode::NetworkError,
                "The TinyURL request could not be completed.",
            )
        })?;

    if !response.status().is_success() {
        return Err(map_http_status(response.status()));
    }

    let payload = response
        .json::<TinyUrlCreateResponse>()
        .await
        .map_err(|_| {
            ShortLinkError::new(
                ShortLinkErrorCode::InvalidResponse,
                "TinyURL returned an invalid response.",
            )
        })?;
    let short_url = payload.data.and_then(|data| data.tiny_url).ok_or_else(|| {
        ShortLinkError::new(
            ShortLinkErrorCode::InvalidResponse,
            "TinyURL returned an invalid response.",
        )
    })?;
    validate_http_url(&short_url).map_err(|_| {
        ShortLinkError::new(
            ShortLinkErrorCode::InvalidResponse,
            "TinyURL returned an invalid response.",
        )
    })?;

    Ok(ShortLinkResult {
        long_url: url,
        short_url,
    })
}

#[tauri::command]
pub fn short_link_config_status() -> Result<ShortLinkConfigStatus, ShortLinkError> {
    status_with_reader(crate::secrets::get_tinyurl_api_token)
}

#[tauri::command]
pub fn short_link_save_token(token: String) -> Result<ShortLinkConfigStatus, ShortLinkError> {
    save_with_writer(token, crate::secrets::save_tinyurl_api_token)
}

#[tauri::command]
pub fn short_link_delete_token() -> Result<ShortLinkConfigStatus, ShortLinkError> {
    delete_with_writer(crate::secrets::delete_tinyurl_api_token)
}

#[tauri::command]
pub async fn short_link_create(url: String) -> Result<ShortLinkResult, ShortLinkError> {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| {
            ShortLinkError::new(
                ShortLinkErrorCode::NetworkError,
                "The TinyURL request could not be completed.",
            )
        })?;
    create_with_dependencies(
        url,
        crate::secrets::get_tinyurl_api_token,
        &client,
        TINYURL_CREATE_URL,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http_body_util::{BodyExt, Full};
    use hyper::body::Incoming;
    use hyper::service::service_fn;
    use hyper::{Method, Request, Response};
    use hyper_util::rt::TokioIo;
    use serde_json::{json, Value};
    use std::convert::Infallible;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    const TEST_TOKEN: &str = "local-mock-sensitive-token";
    const TEST_LONG_URL: &str = "https://example.test/private/path?secret=query-value";

    struct CapturedRequest {
        method: Method,
        path: String,
        authorization: Option<String>,
        body: Vec<u8>,
    }

    async fn spawn_mock(
        status: StatusCode,
        body: String,
        delay: Duration,
    ) -> (String, oneshot::Receiver<CapturedRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local mock server");
        let address = listener.local_addr().expect("read local mock address");
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));

        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let service = service_fn(move |request: Request<Incoming>| {
                let sender = Arc::clone(&sender);
                let body = body.clone();
                async move {
                    let method = request.method().clone();
                    let path = request.uri().path().to_string();
                    let authorization = request
                        .headers()
                        .get(reqwest::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);
                    let request_body = request
                        .into_body()
                        .collect()
                        .await
                        .expect("collect local request body")
                        .to_bytes()
                        .to_vec();
                    if let Some(sender) = sender.lock().expect("lock request sender").take() {
                        let _ = sender.send(CapturedRequest {
                            method,
                            path,
                            authorization,
                            body: request_body,
                        });
                    }
                    tokio::time::sleep(delay).await;
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(status)
                            .header("content-type", "application/json")
                            .body(Full::new(Bytes::from(body)))
                            .expect("build local response"),
                    )
                }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });

        (format!("http://{address}/create"), receiver)
    }

    fn test_client(timeout: Duration) -> Client {
        Client::builder()
            .timeout(timeout)
            .build()
            .expect("build local test client")
    }

    fn expect_create_error(
        result: Result<ShortLinkResult, ShortLinkError>,
        context: &'static str,
    ) -> ShortLinkError {
        match result {
            Ok(_) => panic!("{context}"),
            Err(error) => error,
        }
    }

    #[test]
    fn short_link_url_validation_accepts_only_absolute_http_urls_with_hosts() {
        for valid in [
            "https://example.com",
            "http://localhost:8080/path",
            "https://127.0.0.1/resource",
        ] {
            assert!(validate_http_url(valid).is_ok());
        }

        for invalid in [
            "",
            "   ",
            "/relative/path",
            "https://",
            "javascript:alert(1)",
            "data:text/plain,hello",
            "file:///tmp/item",
        ] {
            let error = validate_http_url(invalid).expect_err("reject invalid URL");
            assert_eq!(error.code, ShortLinkErrorCode::InvalidUrl);
        }
    }

    #[tokio::test]
    async fn short_link_invalid_url_fails_before_credentials_or_http_are_used() {
        let client = test_client(Duration::from_millis(100));
        for invalid in ["", "/relative", "https://", "javascript:alert(1)"] {
            let error = expect_create_error(
                create_with_dependencies(
                    invalid.to_string(),
                    || panic!("credential reader must not run for invalid URLs"),
                    &client,
                    "http://127.0.0.1:1/create",
                )
                .await,
                "invalid URL unexpectedly created a short link",
            );
            assert_eq!(error.code, ShortLinkErrorCode::InvalidUrl);
        }
    }

    #[tokio::test]
    async fn short_link_create_sends_minimal_authenticated_post_and_returns_minimal_result() {
        let response_body = json!({
            "data": { "tiny_url": "https://tinyurl.com/local-result" },
            "extra": TEST_TOKEN
        })
        .to_string();
        let (endpoint, captured) = spawn_mock(StatusCode::OK, response_body, Duration::ZERO).await;
        let result = create_with_dependencies(
            TEST_LONG_URL.to_string(),
            || Ok(Some(TEST_TOKEN.to_string())),
            &test_client(Duration::from_secs(1)),
            &endpoint,
        )
        .await
        .expect("create short link through local mock");
        let request = captured.await.expect("capture local request");
        let expected_authorization = format!("Bearer {TEST_TOKEN}");
        let body: Value = serde_json::from_slice(&request.body).expect("parse request body");
        let has_only_url = body.as_object().is_some_and(|object| {
            object.len() == 1 && object.get("url").and_then(Value::as_str) == Some(TEST_LONG_URL)
        });

        assert_eq!(request.method, Method::POST);
        assert_eq!(request.path, "/create");
        assert!(request.authorization == Some(expected_authorization));
        assert!(has_only_url);
        assert!(result.long_url == TEST_LONG_URL);
        assert_eq!(result.short_url, "https://tinyurl.com/local-result");
        let serialized = serde_json::to_string(&result).expect("serialize minimal result");
        let serialized_value: Value =
            serde_json::from_str(&serialized).expect("parse minimal result");
        let has_minimal_result_shape = serialized_value.as_object().is_some_and(|object| {
            object.len() == 2
                && object.get("longUrl").and_then(Value::as_str) == Some(TEST_LONG_URL)
                && object.get("shortUrl").and_then(Value::as_str)
                    == Some("https://tinyurl.com/local-result")
        });
        assert!(has_minimal_result_shape);
        assert!(!serialized.contains(TEST_TOKEN));
    }

    #[tokio::test]
    async fn short_link_http_statuses_map_to_stable_codes_without_reading_response_details() {
        let cases = [
            (
                StatusCode::UNAUTHORIZED,
                ShortLinkErrorCode::AuthenticationFailed,
            ),
            (
                StatusCode::FORBIDDEN,
                ShortLinkErrorCode::AuthenticationFailed,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                ShortLinkErrorCode::RateLimited,
            ),
            (StatusCode::BAD_REQUEST, ShortLinkErrorCode::RequestRejected),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ShortLinkErrorCode::ServiceUnavailable,
            ),
        ];

        for (status, expected_code) in cases {
            let sensitive_body = json!({ "token": TEST_TOKEN, "url": TEST_LONG_URL }).to_string();
            let (endpoint, _) = spawn_mock(status, sensitive_body, Duration::ZERO).await;
            let error = expect_create_error(
                create_with_dependencies(
                    TEST_LONG_URL.to_string(),
                    || Ok(Some(TEST_TOKEN.to_string())),
                    &test_client(Duration::from_secs(1)),
                    &endpoint,
                )
                .await,
                "non-success HTTP response unexpectedly created a short link",
            );
            assert_eq!(error.code, expected_code);
            let serialized = serde_json::to_string(&error).expect("serialize HTTP error");
            assert!(!serialized.contains(TEST_TOKEN));
            assert!(!serialized.contains(TEST_LONG_URL));
            assert!(!serialized.to_ascii_lowercase().contains("authorization"));
        }
    }

    #[tokio::test]
    async fn short_link_malformed_success_responses_map_to_invalid_response() {
        for body in [
            "not-json".to_string(),
            json!({ "data": {} }).to_string(),
            json!({ "data": { "tiny_url": "file:///tmp/not-http" } }).to_string(),
            json!({ "data": { "tiny_url": "https://" } }).to_string(),
        ] {
            let (endpoint, _) = spawn_mock(StatusCode::OK, body, Duration::ZERO).await;
            let error = expect_create_error(
                create_with_dependencies(
                    TEST_LONG_URL.to_string(),
                    || Ok(Some(TEST_TOKEN.to_string())),
                    &test_client(Duration::from_secs(1)),
                    &endpoint,
                )
                .await,
                "malformed response unexpectedly created a short link",
            );
            assert_eq!(error.code, ShortLinkErrorCode::InvalidResponse);
        }
    }

    #[tokio::test]
    async fn short_link_timeout_and_connection_failure_map_to_network_error() {
        let valid_response = json!({
            "data": { "tiny_url": "https://tinyurl.com/late-result" }
        })
        .to_string();
        let (endpoint, _) =
            spawn_mock(StatusCode::OK, valid_response, Duration::from_millis(250)).await;
        let timeout_error = expect_create_error(
            create_with_dependencies(
                TEST_LONG_URL.to_string(),
                || Ok(Some(TEST_TOKEN.to_string())),
                &test_client(Duration::from_millis(25)),
                &endpoint,
            )
            .await,
            "timed-out response unexpectedly created a short link",
        );
        assert_eq!(timeout_error.code, ShortLinkErrorCode::NetworkError);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("reserve local address");
        let unused_address = listener.local_addr().expect("read unused local address");
        drop(listener);
        let connection_error = expect_create_error(
            create_with_dependencies(
                TEST_LONG_URL.to_string(),
                || Ok(Some(TEST_TOKEN.to_string())),
                &test_client(Duration::from_millis(100)),
                &format!("http://{unused_address}/create"),
            )
            .await,
            "connection failure unexpectedly created a short link",
        );
        assert_eq!(connection_error.code, ShortLinkErrorCode::NetworkError);
    }

    #[test]
    fn short_link_credential_commands_trim_hide_and_delete_only_the_token() {
        let status = status_with_reader(|| Ok(Some(TEST_TOKEN.to_string())))
            .expect("read configured status");
        let serialized = serde_json::to_string(&status).expect("serialize configured status");
        assert_eq!(serialized, r#"{"configured":true}"#);
        assert!(!serialized.contains(TEST_TOKEN));

        let written = Arc::new(Mutex::new(None));
        let written_for_save = Arc::clone(&written);
        let save_status = save_with_writer(format!("  {TEST_TOKEN}  "), move |value| {
            *written_for_save.lock().expect("lock saved token") = Some(value);
            Ok(())
        })
        .expect("save trimmed token");
        assert!(save_status.configured);
        assert!(written.lock().expect("read saved token").as_deref() == Some(TEST_TOKEN));

        let untouched_history = vec!["local-history-entry"];
        let deleted = Arc::new(Mutex::new(false));
        let deleted_for_call = Arc::clone(&deleted);
        let delete_status = delete_with_writer(move || {
            *deleted_for_call.lock().expect("lock delete flag") = true;
            Ok(())
        })
        .expect("delete token");
        assert!(!delete_status.configured);
        assert!(*deleted.lock().expect("read delete flag"));
        assert_eq!(untouched_history, vec!["local-history-entry"]);
    }

    #[test]
    fn short_link_blank_token_is_rejected_without_writing() {
        let error = save_with_writer(" \n\t ".to_string(), |_| {
            panic!("blank token must not reach storage")
        })
        .expect_err("reject blank token");
        assert_eq!(error.code, ShortLinkErrorCode::NotConfigured);
    }

    #[tokio::test]
    async fn short_link_missing_credentials_and_storage_failures_use_stable_codes() {
        let client = test_client(Duration::from_millis(100));
        let missing = expect_create_error(
            create_with_dependencies(
                TEST_LONG_URL.to_string(),
                || Ok(None),
                &client,
                "http://127.0.0.1:1/create",
            )
            .await,
            "missing credentials unexpectedly created a short link",
        );
        assert_eq!(missing.code, ShortLinkErrorCode::NotConfigured);

        let read_failure = expect_create_error(
            create_with_dependencies(
                TEST_LONG_URL.to_string(),
                || Err("sensitive storage detail".to_string()),
                &client,
                "http://127.0.0.1:1/create",
            )
            .await,
            "credential read failure unexpectedly created a short link",
        );
        assert_eq!(read_failure.code, ShortLinkErrorCode::StorageError);

        let status_failure = status_with_reader(|| Err("status detail".to_string()))
            .expect_err("map status failure");
        let save_failure =
            save_with_writer(TEST_TOKEN.to_string(), |_| Err("save detail".to_string()))
                .expect_err("map save failure");
        let delete_failure = delete_with_writer(|| Err("delete detail".to_string()))
            .expect_err("map delete failure");
        for error in [status_failure, save_failure, delete_failure] {
            assert_eq!(error.code, ShortLinkErrorCode::StorageError);
        }
    }

    #[test]
    fn short_link_error_serialization_contains_no_sensitive_request_data() {
        for (code, expected_code) in [
            (ShortLinkErrorCode::NotConfigured, "not_configured"),
            (ShortLinkErrorCode::InvalidUrl, "invalid_url"),
            (
                ShortLinkErrorCode::AuthenticationFailed,
                "authentication_failed",
            ),
            (ShortLinkErrorCode::RateLimited, "rate_limited"),
            (ShortLinkErrorCode::RequestRejected, "request_rejected"),
            (
                ShortLinkErrorCode::ServiceUnavailable,
                "service_unavailable",
            ),
            (ShortLinkErrorCode::NetworkError, "network_error"),
            (ShortLinkErrorCode::InvalidResponse, "invalid_response"),
            (ShortLinkErrorCode::StorageError, "storage_error"),
        ] {
            let error = ShortLinkError::new(code, "Safe diagnostic summary.");
            let serialized = serde_json::to_string(&error).expect("serialize safe error");
            let serialized_value: Value =
                serde_json::from_str(&serialized).expect("parse safe error");
            let has_expected_code =
                serialized_value.get("code").and_then(Value::as_str) == Some(expected_code);
            assert!(has_expected_code);
            let debug = format!("{error:?}");
            for output in [serialized, debug] {
                assert!(!output.contains(TEST_TOKEN));
                assert!(!output.contains(TEST_LONG_URL));
                assert!(!output.to_ascii_lowercase().contains("authorization"));
            }
        }
    }
}
