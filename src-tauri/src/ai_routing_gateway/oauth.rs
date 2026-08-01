use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use super::error::{GatewayError, GatewayErrorCategory};

const SESSION_TTL: Duration = Duration::from_secs(10 * 60);
const MIN_DEVICE_INTERVAL: Duration = Duration::from_secs(1);
const SLOW_DOWN_INCREMENT: Duration = Duration::from_secs(5);

// OpenAI 尚未发布第三方桌面应用可用的 Codex OAuth client 注册和授权契约。
// 该门禁禁止复制官方客户端的内部 client ID、scope 或端点。
pub(crate) const OAUTH_RELEASE_BLOCK_REASON: &str =
    "official_third_party_codex_oauth_contract_unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OfficialOAuthAvailability {
    ReleaseBlocked,
}

pub(crate) fn official_oauth_availability() -> OfficialOAuthAvailability {
    OfficialOAuthAvailability::ReleaseBlocked
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorizationStart {
    pub(crate) session_id: String,
    pub(crate) authorization_url: String,
    pub(crate) callback_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorizationCode {
    pub(crate) code: String,
    pub(crate) pkce_verifier: String,
}

#[derive(Debug)]
struct AuthorizationSession {
    state: String,
    pkce_verifier: String,
    callback_url: String,
    expires_at: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct OAuthSessionStore {
    authorization: Mutex<HashMap<String, AuthorizationSession>>,
    devices: Mutex<HashMap<String, DeviceSession>>,
}

impl OAuthSessionStore {
    pub(crate) fn begin_loopback(
        &self,
        _callback_port: u16,
    ) -> Result<AuthorizationStart, GatewayError> {
        Err(oauth_error(GatewayErrorCategory::OAuthReleaseBlocked, None))
    }

    pub(crate) fn begin_device_code(&self) -> Result<DeviceCodeStart, GatewayError> {
        Err(oauth_error(GatewayErrorCategory::OAuthReleaseBlocked, None))
    }

    pub(crate) fn complete_callback(
        &self,
        session_id: &str,
        full_callback_url: &str,
    ) -> Result<AuthorizationCode, GatewayError> {
        let mut sessions = self
            .authorization
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = sessions.remove(session_id) else {
            return Err(oauth_error(
                GatewayErrorCategory::OAuthSessionInvalid,
                Some(session_id),
            ));
        };
        if session.expires_at <= Instant::now() {
            return Err(oauth_error(
                GatewayErrorCategory::OAuthExpired,
                Some(session_id),
            ));
        }
        let callback = url::Url::parse(full_callback_url).map_err(|_| {
            oauth_error(GatewayErrorCategory::OAuthSessionInvalid, Some(session_id))
        })?;
        let expected = url::Url::parse(&session.callback_url).map_err(|_| {
            oauth_error(GatewayErrorCategory::OAuthSessionInvalid, Some(session_id))
        })?;
        if callback.scheme() != "http"
            || !is_loopback_host(callback.host_str())
            || callback.origin() != expected.origin()
            || callback.path() != expected.path()
        {
            return Err(oauth_error(
                GatewayErrorCategory::OAuthSessionInvalid,
                Some(session_id),
            ));
        }
        let query: HashMap<_, _> = callback.query_pairs().into_owned().collect();
        if query.get("state") != Some(&session.state) {
            return Err(oauth_error(
                GatewayErrorCategory::OAuthStateMismatch,
                Some(session_id),
            ));
        }
        if query.contains_key("error") {
            return Err(oauth_error(
                GatewayErrorCategory::OAuthSessionInvalid,
                Some(session_id),
            ));
        }
        let code = query
            .get("code")
            .filter(|value| !value.is_empty())
            .cloned()
            .ok_or_else(|| {
                oauth_error(GatewayErrorCategory::OAuthSessionInvalid, Some(session_id))
            })?;
        Ok(AuthorizationCode {
            code,
            pkce_verifier: session.pkce_verifier,
        })
    }

    pub(crate) fn cancel(&self, session_id: &str) {
        self.authorization
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(session_id);
        self.devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(session_id);
    }

    pub(crate) fn note_loopback_listener_failure(
        &self,
        session_id: &str,
    ) -> Result<(), GatewayError> {
        let sessions = self
            .authorization
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if sessions
            .get(session_id)
            .is_some_and(|session| session.expires_at > Instant::now())
        {
            Ok(())
        } else {
            Err(oauth_error(
                GatewayErrorCategory::OAuthSessionInvalid,
                Some(session_id),
            ))
        }
    }

    pub(crate) fn apply_device_response(
        &self,
        session_id: &str,
        response: DevicePollResponse,
        now: Instant,
    ) -> Result<DevicePollOutcome, GatewayError> {
        let mut devices = self
            .devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = devices.get_mut(session_id) else {
            return Err(oauth_error(
                GatewayErrorCategory::OAuthSessionInvalid,
                Some(session_id),
            ));
        };
        if now >= session.expires_at {
            devices.remove(session_id);
            return Ok(DevicePollOutcome::Expired);
        }
        if now < session.next_poll_at {
            return Ok(DevicePollOutcome::Wait(session.next_poll_at - now));
        }
        match response {
            DevicePollResponse::AuthorizationPending => {
                session.next_poll_at = now + session.interval;
                Ok(DevicePollOutcome::Pending(session.interval))
            }
            DevicePollResponse::SlowDown => {
                session.interval += SLOW_DOWN_INCREMENT;
                session.next_poll_at = now + session.interval;
                Ok(DevicePollOutcome::Pending(session.interval))
            }
            DevicePollResponse::Authorized { code } => {
                devices.remove(session_id);
                Ok(DevicePollOutcome::Authorized(code))
            }
            DevicePollResponse::AccessDenied => {
                devices.remove(session_id);
                Ok(DevicePollOutcome::Denied)
            }
            DevicePollResponse::ExpiredToken => {
                devices.remove(session_id);
                Ok(DevicePollOutcome::Expired)
            }
        }
    }

    #[cfg(test)]
    fn begin_loopback_fixture(
        &self,
        callback_port: u16,
        authorization_endpoint: &str,
        client_id: &str,
        fixed_scope: &str,
    ) -> AuthorizationStart {
        assert!(callback_port > 0);
        let session_id = random_url_token(16);
        let state = random_url_token(32);
        let pkce_verifier = random_url_token(64);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce_verifier.as_bytes()));
        let callback_url = format!("http://127.0.0.1:{callback_port}/oauth/callback");
        let mut url = url::Url::parse(authorization_endpoint).expect("fixture authorization URL");
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &callback_url)
            .append_pair("scope", fixed_scope)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        self.authorization
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                session_id.clone(),
                AuthorizationSession {
                    state,
                    pkce_verifier,
                    callback_url: callback_url.clone(),
                    expires_at: Instant::now() + SESSION_TTL,
                },
            );
        AuthorizationStart {
            session_id,
            authorization_url: url.into(),
            callback_url,
        }
    }

    #[cfg(test)]
    fn begin_device_fixture(&self, interval: Duration, expires_in: Duration) -> DeviceCodeStart {
        let session_id = random_url_token(16);
        let now = Instant::now();
        let interval = interval.max(MIN_DEVICE_INTERVAL);
        self.devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                session_id.clone(),
                DeviceSession {
                    interval,
                    next_poll_at: now,
                    expires_at: now + expires_in,
                },
            );
        DeviceCodeStart {
            session_id,
            user_code: "TEST-CODE".to_owned(),
            verification_url: "http://127.0.0.1/device".to_owned(),
            interval,
            expires_in,
        }
    }
}

#[derive(Debug)]
struct DeviceSession {
    interval: Duration,
    next_poll_at: Instant,
    expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceCodeStart {
    pub(crate) session_id: String,
    pub(crate) user_code: String,
    pub(crate) verification_url: String,
    pub(crate) interval: Duration,
    pub(crate) expires_in: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DevicePollResponse {
    AuthorizationPending,
    SlowDown,
    Authorized { code: String },
    AccessDenied,
    ExpiredToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DevicePollOutcome {
    Wait(Duration),
    Pending(Duration),
    Authorized(String),
    Denied,
    Expired,
}

fn random_url_token(length: usize) -> String {
    let mut bytes = vec![0u8; length];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("127.0.0.1") | Some("localhost"))
}

fn oauth_error(category: GatewayErrorCategory, entity_id: Option<&str>) -> GatewayError {
    GatewayError::new(category, entity_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_ENDPOINT: &str = "http://127.0.0.1:18443/authorize";
    const FIXTURE_CLIENT_ID: &str = "local-test-client";
    const FIXED_FIXTURE_SCOPE: &str = "openid profile offline_access";

    fn callback(start: &AuthorizationStart) -> (String, String) {
        let authorization = url::Url::parse(&start.authorization_url).unwrap();
        let state = authorization
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        (
            format!("{}?code=local-code&state={state}", start.callback_url),
            state,
        )
    }

    #[test]
    fn production_oauth_is_release_blocked_without_public_contract() {
        let store = OAuthSessionStore::default();
        assert_eq!(
            official_oauth_availability(),
            OfficialOAuthAvailability::ReleaseBlocked
        );
        assert_eq!(
            store.begin_loopback(18222).unwrap_err().category(),
            GatewayErrorCategory::OAuthReleaseBlocked
        );
        assert_eq!(
            store.begin_device_code().unwrap_err().category(),
            GatewayErrorCategory::OAuthReleaseBlocked
        );
        assert!(!OAUTH_RELEASE_BLOCK_REASON.is_empty());
    }

    #[test]
    fn pkce_loopback_and_manual_full_callback_share_strict_memory_session() {
        let store = OAuthSessionStore::default();
        let start = store.begin_loopback_fixture(
            18222,
            FIXTURE_ENDPOINT,
            FIXTURE_CLIENT_ID,
            FIXED_FIXTURE_SCOPE,
        );
        let authorization = url::Url::parse(&start.authorization_url).unwrap();
        let query: HashMap<_, _> = authorization.query_pairs().into_owned().collect();
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some(FIXED_FIXTURE_SCOPE)
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        let (manual_callback, _) = callback(&start);
        let code = store
            .complete_callback(&start.session_id, &manual_callback)
            .expect("complete manual callback");
        assert_eq!(code.code, "local-code");
        assert!(code.pkce_verifier.len() >= 43);
        assert_eq!(
            store
                .complete_callback(&start.session_id, &manual_callback)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::OAuthSessionInvalid
        );
    }

    #[test]
    fn callback_rejects_state_error_and_non_loopback_then_cleans_terminal_session() {
        let store = OAuthSessionStore::default();
        let start = store.begin_loopback_fixture(
            18223,
            FIXTURE_ENDPOINT,
            FIXTURE_CLIENT_ID,
            FIXED_FIXTURE_SCOPE,
        );
        let bad_state = format!("{}?code=x&state=wrong", start.callback_url);
        assert_eq!(
            store
                .complete_callback(&start.session_id, &bad_state)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::OAuthStateMismatch
        );
        assert_eq!(
            store
                .complete_callback(&start.session_id, &bad_state)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::OAuthSessionInvalid
        );

        let start = store.begin_loopback_fixture(
            18224,
            FIXTURE_ENDPOINT,
            FIXTURE_CLIENT_ID,
            FIXED_FIXTURE_SCOPE,
        );
        let (_, state) = callback(&start);
        let hostile = format!("http://example.com:18224/oauth/callback?code=x&state={state}");
        assert_eq!(
            store
                .complete_callback(&start.session_id, &hostile)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::OAuthSessionInvalid
        );

        let start = store.begin_loopback_fixture(
            18225,
            FIXTURE_ENDPOINT,
            FIXTURE_CLIENT_ID,
            FIXED_FIXTURE_SCOPE,
        );
        let (_, state) = callback(&start);
        let denied = format!("{}?error=access_denied&state={state}", start.callback_url);
        assert_eq!(
            store
                .complete_callback(&start.session_id, &denied)
                .unwrap_err()
                .category(),
            GatewayErrorCategory::OAuthSessionInvalid
        );
    }

    #[test]
    fn loopback_listener_failure_preserves_manual_callback_fallback() {
        let store = OAuthSessionStore::default();
        let start = store.begin_loopback_fixture(
            18226,
            FIXTURE_ENDPOINT,
            FIXTURE_CLIENT_ID,
            FIXED_FIXTURE_SCOPE,
        );
        store
            .note_loopback_listener_failure(&start.session_id)
            .expect("listener failure must preserve session");
        let (manual_callback, _) = callback(&start);
        assert_eq!(
            store
                .complete_callback(&start.session_id, &manual_callback)
                .unwrap()
                .code,
            "local-code"
        );
    }

    #[test]
    fn device_code_honors_interval_slow_down_and_all_terminal_states() {
        let store = OAuthSessionStore::default();
        let start = store.begin_device_fixture(Duration::from_secs(2), Duration::from_secs(60));
        let now = Instant::now();
        assert_eq!(
            store
                .apply_device_response(
                    &start.session_id,
                    DevicePollResponse::AuthorizationPending,
                    now
                )
                .unwrap(),
            DevicePollOutcome::Pending(Duration::from_secs(2))
        );
        assert!(matches!(
            store
                .apply_device_response(&start.session_id, DevicePollResponse::SlowDown, now)
                .unwrap(),
            DevicePollOutcome::Wait(_)
        ));
        let later = now + Duration::from_secs(2);
        assert_eq!(
            store
                .apply_device_response(&start.session_id, DevicePollResponse::SlowDown, later)
                .unwrap(),
            DevicePollOutcome::Pending(Duration::from_secs(7))
        );
        assert_eq!(
            store
                .apply_device_response(
                    &start.session_id,
                    DevicePollResponse::Authorized {
                        code: "device-code".into()
                    },
                    later + Duration::from_secs(7),
                )
                .unwrap(),
            DevicePollOutcome::Authorized("device-code".into())
        );

        for (response, expected) in [
            (DevicePollResponse::AccessDenied, DevicePollOutcome::Denied),
            (DevicePollResponse::ExpiredToken, DevicePollOutcome::Expired),
        ] {
            let start = store.begin_device_fixture(Duration::ZERO, Duration::from_secs(60));
            assert_eq!(
                store
                    .apply_device_response(&start.session_id, response, Instant::now())
                    .unwrap(),
                expected
            );
            assert_eq!(
                store
                    .apply_device_response(
                        &start.session_id,
                        DevicePollResponse::AuthorizationPending,
                        Instant::now(),
                    )
                    .unwrap_err()
                    .category(),
                GatewayErrorCategory::OAuthSessionInvalid
            );
        }

        let start = store.begin_device_fixture(Duration::ZERO, Duration::ZERO);
        assert_eq!(
            store
                .apply_device_response(
                    &start.session_id,
                    DevicePollResponse::AuthorizationPending,
                    Instant::now(),
                )
                .unwrap(),
            DevicePollOutcome::Expired
        );
        let start = store.begin_device_fixture(Duration::ZERO, Duration::from_secs(60));
        store.cancel(&start.session_id);
        assert_eq!(
            store
                .apply_device_response(
                    &start.session_id,
                    DevicePollResponse::AuthorizationPending,
                    Instant::now(),
                )
                .unwrap_err()
                .category(),
            GatewayErrorCategory::OAuthSessionInvalid
        );
    }
}
