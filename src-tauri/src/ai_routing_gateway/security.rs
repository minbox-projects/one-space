use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use rusqlite::Connection;
use std::sync::{Mutex, OnceLock};

use super::error::{GatewayError, GatewayErrorCategory};

const CIPHER_VERSION: i64 = 1;
const ROOT_KEY_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;
const KEYCHAIN_SERVICE: &str = "com.onespace.ai-routing-gateway";
const KEYCHAIN_ACCOUNT: &str = "root-data-key-v1";

pub(crate) struct RootKey([u8; ROOT_KEY_LENGTH]);

impl std::fmt::Debug for RootKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RootKey([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EncryptedCredential {
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) nonce: [u8; NONCE_LENGTH],
    pub(crate) cipher_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecurityLockReason {
    StorageUnavailable,
    RootKeyMissing,
    CredentialStoreUnavailable,
    RootKeyInvalid,
}

#[derive(Debug)]
pub(crate) enum SecurityState {
    Ready(RootKey),
    Locked(SecurityLockReason),
}

fn root_key_creation_lock() -> &'static Mutex<()> {
    static ROOT_KEY_CREATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ROOT_KEY_CREATION_LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) trait RootKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>, GatewayError>;
    fn store(&self, key: &[u8]) -> Result<(), GatewayError>;
}

#[cfg(target_os = "macos")]
pub(crate) struct MacOsKeychainStore;

#[cfg(target_os = "macos")]
impl RootKeyStore for MacOsKeychainStore {
    fn load(&self) -> Result<Option<Vec<u8>>, GatewayError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|_| {
            GatewayError::new(GatewayErrorCategory::CredentialStoreUnavailable, None)
        })?;
        match entry.get_password() {
            Ok(encoded) => STANDARD
                .decode(encoded)
                .map(Some)
                .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, None)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(GatewayError::new(
                GatewayErrorCategory::CredentialStoreUnavailable,
                None,
            )),
        }
    }

    fn store(&self, key: &[u8]) -> Result<(), GatewayError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|_| {
            GatewayError::new(GatewayErrorCategory::CredentialStoreUnavailable, None)
        })?;
        entry
            .set_password(&STANDARD.encode(key))
            .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialStoreUnavailable, None))
    }
}

pub(crate) fn initialize_security(
    connection: &Connection,
    key_store: &dyn RootKeyStore,
) -> SecurityState {
    let encrypted_records = match connection.query_row(
        "SELECT (SELECT COUNT(*) FROM ai_gateway_credentials) + (SELECT COUNT(*) FROM ai_gateway_api_keys WHERE ciphertext IS NOT NULL)",
        [],
        |row| row.get::<_, i64>(0),
    ) {
            Ok(count) => count,
            Err(_) => return SecurityState::Locked(SecurityLockReason::StorageUnavailable),
        };

    match key_store.load() {
        Ok(Some(bytes)) => match RootKey::try_from(bytes) {
            Ok(key) => SecurityState::Ready(key),
            Err(_) => SecurityState::Locked(SecurityLockReason::RootKeyInvalid),
        },
        Ok(None) if encrypted_records > 0 => {
            SecurityState::Locked(SecurityLockReason::RootKeyMissing)
        }
        Ok(None) => {
            // 在生成和保存前重新读取，确保并发调用共享同一个已保存根密钥。
            let _creation_guard = root_key_creation_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match key_store.load() {
                Ok(Some(bytes)) => match RootKey::try_from(bytes) {
                    Ok(key) => SecurityState::Ready(key),
                    Err(_) => SecurityState::Locked(SecurityLockReason::RootKeyInvalid),
                },
                Ok(None) => {
                    let mut bytes = [0u8; ROOT_KEY_LENGTH];
                    OsRng.fill_bytes(&mut bytes);
                    if key_store.store(&bytes).is_err() {
                        return SecurityState::Locked(
                            SecurityLockReason::CredentialStoreUnavailable,
                        );
                    }
                    SecurityState::Ready(RootKey(bytes))
                }
                Err(_) => SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable),
            }
        }
        Err(_) => SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable),
    }
}

impl TryFrom<Vec<u8>> for RootKey {
    type Error = GatewayError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let bytes: [u8; ROOT_KEY_LENGTH] = value
            .try_into()
            .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, None))?;
        Ok(Self(bytes))
    }
}

pub(crate) fn encrypt_credential(
    root_key: &RootKey,
    record_type: &str,
    record_id: &str,
    plaintext: &[u8],
) -> Result<EncryptedCredential, GatewayError> {
    validate_identity(record_type, record_id)?;
    let cipher = Aes256Gcm::new_from_slice(&root_key.0)
        .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, Some(record_id)))?;
    let mut nonce = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce);
    let aad = credential_aad(record_type, record_id);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| {
            GatewayError::new(
                GatewayErrorCategory::CredentialAuthenticationFailed,
                Some(record_id),
            )
        })?;
    Ok(EncryptedCredential {
        ciphertext,
        nonce,
        cipher_version: CIPHER_VERSION,
    })
}

pub(crate) fn decrypt_credential(
    root_key: &RootKey,
    record_type: &str,
    record_id: &str,
    credential: &EncryptedCredential,
) -> Result<Vec<u8>, GatewayError> {
    validate_identity(record_type, record_id)?;
    if credential.cipher_version != CIPHER_VERSION {
        return Err(GatewayError::new(
            GatewayErrorCategory::CredentialVersionUnsupported,
            Some(record_id),
        ));
    }
    let cipher = Aes256Gcm::new_from_slice(&root_key.0)
        .map_err(|_| GatewayError::new(GatewayErrorCategory::CredentialInvalid, Some(record_id)))?;
    let aad = credential_aad(record_type, record_id);
    cipher
        .decrypt(
            Nonce::from_slice(&credential.nonce),
            Payload {
                msg: &credential.ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| {
            GatewayError::new(
                GatewayErrorCategory::CredentialAuthenticationFailed,
                Some(record_id),
            )
        })
}

fn validate_identity(record_type: &str, record_id: &str) -> Result<(), GatewayError> {
    if record_type.is_empty() || record_id.is_empty() {
        return Err(GatewayError::new(
            GatewayErrorCategory::CredentialInvalid,
            (!record_id.is_empty()).then_some(record_id),
        ));
    }
    Ok(())
}

fn credential_aad(record_type: &str, record_id: &str) -> Vec<u8> {
    format!("onespace.ai-routing-gateway.v1\0{record_type}\0{record_id}").into_bytes()
}
