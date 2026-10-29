//! Managed HTTP transport for the ACME SDK.
//!
//! Every ACME directory, account, order, authorization and certificate
//! request builds a fresh `reqwest::Client` from the current global proxy
//! snapshot, so a proxy change is observed by the next SDK operation while the
//! account and order identity stays intact.

use std::future::Future;
use std::pin::Pin;

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::{BodyExt, Full};
use instant_acme::{BodyWrapper, BytesResponse, Error, HttpClient};

/// `instant_acme::HttpClient` implementation backed by the application's
/// managed `reqwest` transport.
#[derive(Clone, Copy, Debug, Default)]
pub struct AcmeHttpClient;

impl AcmeHttpClient {
    pub fn new() -> Self {
        Self
    }
}

impl HttpClient for AcmeHttpClient {
    fn request(
        &self,
        request: Request<BodyWrapper<Bytes>>,
    ) -> Pin<Box<dyn Future<Output = Result<BytesResponse, Error>> + Send>> {
        Box::pin(send(request))
    }
}

async fn send(request: Request<BodyWrapper<Bytes>>) -> Result<BytesResponse, Error> {
    let (parts, body) = request.into_parts();
    let body_bytes = body
        .collect()
        .await
        .map_err(|_| request_body_error())?
        .to_bytes();

    let client = crate::proxy::apply_current_proxy_to_builder(reqwest::Client::builder())
        .map_err(|_| transport_error())?
        // The SDK's default hyper client does not follow redirects.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| transport_error())?;

    let response = client
        .request(parts.method, parts.uri.to_string())
        .headers(parts.headers)
        .body(body_bytes)
        .send()
        .await
        .map_err(|error| Error::Other(Box::new(error)))?;

    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| Error::Other(Box::new(error)))?;

    let mut managed = Response::new(Full::new(bytes));
    *managed.status_mut() = status;
    *managed.headers_mut() = headers;
    Ok(BytesResponse::from(managed))
}

fn transport_error() -> Error {
    Error::Other(Box::new(std::io::Error::other(
        "Failed to prepare ACME HTTP transport",
    )))
}

fn request_body_error() -> Error {
    Error::Other(Box::new(std::io::Error::other(
        "Failed to read ACME HTTP request body",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssl_certificate::tests::{
        http_proxy_config, publish_direct, publish_proxy, FakeAcmeProxy,
    };
    use http::{Method, StatusCode};

    fn request(
        method: Method,
        uri: &str,
        body: &[u8],
        header: (&str, &str),
    ) -> Request<BodyWrapper<Bytes>> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("X-Fixture-Request", header.1)
            .body(BodyWrapper::from(body.to_vec()))
            .unwrap()
    }

    async fn response_body(mut response: BytesResponse) -> (StatusCode, http::HeaderMap, Vec<u8>) {
        let status = response.parts.status;
        let headers = response.parts.headers.clone();
        let body = response
            .body
            .into_bytes()
            .await
            .expect("collect the ACME HTTP response body");
        (status, headers, body.to_vec())
    }

    fn assert_ok_directory_response(status: StatusCode, headers: &http::HeaderMap, body: &[u8]) {
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers
                .get("replay-nonce")
                .and_then(|value| value.to_str().ok()),
            Some("SAFE_FIXTURE_replay_nonce")
        );
        assert!(String::from_utf8_lossy(body).contains("\"newNonce\""));
    }

    fn assert_ok_submit_response(status: StatusCode, headers: &http::HeaderMap, body: &[u8]) {
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers
                .get("x-fixture-response")
                .and_then(|value| value.to_str().ok()),
            Some("SAFE_FIXTURE_response")
        );
        assert_eq!(body, b"SAFE_FIXTURE_ok");
    }

    #[tokio::test]
    async fn acme_http_client_routes_each_request_through_current_proxy() {
        let _serial = crate::proxy::test_support::SERIAL_GUARD
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let proxy_a = FakeAcmeProxy::new(String::new()).await;
        let proxy_b = FakeAcmeProxy::new(String::new()).await;
        let _home =
            crate::proxy::test_support::with_global_proxy(Some(http_proxy_config(&proxy_a)));
        let client = AcmeHttpClient::new();

        let (status, headers, body) = response_body(
            client
                .request(request(
                    Method::GET,
                    "http://acme.example.test/directory",
                    b"",
                    ("X-Fixture-Request", "SAFE_FIXTURE_header"),
                ))
                .await
                .expect("GET through proxy A"),
        )
        .await;
        assert_ok_directory_response(status, &headers, &body);

        let (status, headers, body) = response_body(
            client
                .request(request(
                    Method::POST,
                    "http://acme.example.test/submit",
                    b"SAFE_FIXTURE_body",
                    ("X-Fixture-Request", "SAFE_FIXTURE_header"),
                ))
                .await
                .expect("POST through proxy A"),
        )
        .await;
        assert_ok_submit_response(status, &headers, &body);

        publish_proxy(&proxy_b);

        let (status, headers, body) = response_body(
            client
                .request(request(
                    Method::GET,
                    "http://acme.example.test/directory",
                    b"",
                    ("X-Fixture-Request", "SAFE_FIXTURE_header"),
                ))
                .await
                .expect("GET through proxy B"),
        )
        .await;
        assert_ok_directory_response(status, &headers, &body);

        let (status, headers, body) = response_body(
            client
                .request(request(
                    Method::POST,
                    "http://acme.example.test/submit",
                    b"SAFE_FIXTURE_body",
                    ("X-Fixture-Request", "SAFE_FIXTURE_header"),
                ))
                .await
                .expect("POST through proxy B"),
        )
        .await;
        assert_ok_submit_response(status, &headers, &body);

        let (status, headers, body) = response_body(
            client
                .request(request(
                    Method::GET,
                    "http://acme.example.test/missing",
                    b"",
                    ("X-Fixture-Request", "SAFE_FIXTURE_header"),
                ))
                .await
                .expect("non-2xx through proxy B is a normal response"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            headers
                .get("x-fixture-response")
                .and_then(|value| value.to_str().ok()),
            Some("SAFE_FIXTURE_missing")
        );
        assert_eq!(body, b"SAFE_FIXTURE_missing");

        let a_requests = proxy_a.requests();
        assert_eq!(a_requests.len(), 2);
        assert_eq!(a_requests[0].method, "GET");
        assert_eq!(a_requests[0].target, "http://acme.example.test/directory");
        assert_eq!(
            a_requests[0].header("x-fixture-request"),
            Some("SAFE_FIXTURE_header")
        );
        assert_eq!(a_requests[1].method, "POST");
        assert_eq!(a_requests[1].target, "http://acme.example.test/submit");
        assert_eq!(
            a_requests[1].header("x-fixture-request"),
            Some("SAFE_FIXTURE_header")
        );
        assert_eq!(a_requests[1].body, b"SAFE_FIXTURE_body");

        let b_requests = proxy_b.requests();
        assert_eq!(b_requests.len(), 3);
        assert_eq!(b_requests[0].method, "GET");
        assert_eq!(b_requests[0].target, "http://acme.example.test/directory");
        assert_eq!(
            b_requests[0].header("x-fixture-request"),
            Some("SAFE_FIXTURE_header")
        );
        assert_eq!(b_requests[1].method, "POST");
        assert_eq!(b_requests[1].target, "http://acme.example.test/submit");
        assert_eq!(
            b_requests[1].header("x-fixture-request"),
            Some("SAFE_FIXTURE_header")
        );
        assert_eq!(b_requests[1].body, b"SAFE_FIXTURE_body");
        assert_eq!(b_requests[2].method, "GET");
        assert_eq!(b_requests[2].target, "http://acme.example.test/missing");
        assert!(proxy_a.hits() == 2);

        publish_direct();
    }
}
