use super::{
    error::GatewayErrorCategory,
    security::{
        decrypt_credential, encrypt_credential, initialize_security, EncryptedCredential, RootKey,
        RootKeyStore, SecurityLockReason, SecurityState,
    },
};
use crate::shared_sqlite;
use base64::Engine as _;
use rusqlite::params;
use std::{cell::RefCell, path::PathBuf};

#[derive(Default)]
struct FakeKeyStore {
    stored: RefCell<Option<Vec<u8>>>,
    fail_load: bool,
    fail_store: bool,
    store_calls: RefCell<usize>,
}

impl RootKeyStore for FakeKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>, super::error::GatewayError> {
        if self.fail_load {
            return Err(super::error::GatewayError::new(
                GatewayErrorCategory::CredentialStoreUnavailable,
                None,
            ));
        }
        Ok(self.stored.borrow().clone())
    }

    fn store(&self, key: &[u8]) -> Result<(), super::error::GatewayError> {
        *self.store_calls.borrow_mut() += 1;
        if self.fail_store {
            return Err(super::error::GatewayError::new(
                GatewayErrorCategory::CredentialStoreUnavailable,
                None,
            ));
        }
        *self.stored.borrow_mut() = Some(key.to_vec());
        Ok(())
    }
}

fn database(name: &str) -> (PathBuf, rusqlite::Connection) {
    let path = std::env::temp_dir().join(format!(
        "onespace-ai-gateway-security-{name}-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let connection = shared_sqlite::open_at(&path).expect("open gateway database");
    (path, connection)
}

fn remove_database(path: &PathBuf) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
    }
}

fn root_key(byte: u8) -> RootKey {
    RootKey::try_from(vec![byte; 32]).expect("construct root key")
}

#[test]
fn encryption_uses_random_nonce_and_authenticates_identity() {
    let key = root_key(7);
    let first = encrypt_credential(&key, "oauth_token", "account-1", b"sensitive-token")
        .expect("encrypt first credential");
    let second = encrypt_credential(&key, "oauth_token", "account-1", b"sensitive-token")
        .expect("encrypt second credential");
    assert_ne!(first.nonce, second.nonce);
    assert_ne!(first.ciphertext, second.ciphertext);
    assert_eq!(
        decrypt_credential(&key, "oauth_token", "account-1", &first).expect("decrypt credential"),
        b"sensitive-token"
    );
    let aad_error = decrypt_credential(&key, "oauth_token", "account-2", &first)
        .expect_err("stable record ID must be authenticated");
    assert_eq!(
        aad_error.category(),
        GatewayErrorCategory::CredentialAuthenticationFailed
    );
    assert_eq!(aad_error.entity_id(), Some("account-2"));
}

#[test]
fn tampering_and_unknown_cipher_versions_make_credentials_unavailable() {
    let key = root_key(11);
    let mut encrypted =
        encrypt_credential(&key, "api_key", "account-7", b"secret").expect("encrypt credential");
    encrypted.ciphertext[0] ^= 1;
    assert_eq!(
        decrypt_credential(&key, "api_key", "account-7", &encrypted)
            .expect_err("reject tampered ciphertext")
            .category(),
        GatewayErrorCategory::CredentialAuthenticationFailed
    );
    let unsupported = EncryptedCredential {
        cipher_version: 99,
        ..encrypted
    };
    assert_eq!(
        decrypt_credential(&key, "api_key", "account-7", &unsupported)
            .expect_err("reject unknown cipher version")
            .category(),
        GatewayErrorCategory::CredentialVersionUnsupported
    );
}

#[test]
fn security_errors_and_debug_output_are_redacted() {
    let plaintext = "top-secret-token";
    let key = root_key(19);
    let encrypted = encrypt_credential(&key, "oauth_token", "account-safe", plaintext.as_bytes())
        .expect("encrypt credential");
    let wrong_key = root_key(20);
    let error = decrypt_credential(&wrong_key, "oauth_token", "account-safe", &encrypted)
        .expect_err("reject wrong key");
    let displayed = error.to_string();
    let debugged = format!("{error:?} {key:?} {encrypted:?}");
    assert!(displayed.contains("credential_authentication_failed"));
    assert!(displayed.contains("account-safe"));
    assert!(!displayed.contains(plaintext));
    assert!(!debugged.contains(plaintext));
    assert!(!debugged.contains(&base64::engine::general_purpose::STANDARD.encode([19u8; 32])));
}

#[test]
fn key_store_creates_once_only_when_no_ciphertext_exists() {
    let (path, connection) = database("create-once");
    let store = FakeKeyStore::default();
    assert!(matches!(
        initialize_security(&connection, &store),
        SecurityState::Ready(_)
    ));
    assert_eq!(*store.store_calls.borrow(), 1);
    assert!(matches!(
        initialize_security(&connection, &store),
        SecurityState::Ready(_)
    ));
    assert_eq!(*store.store_calls.borrow(), 1);
    drop(connection);
    remove_database(&path);
}

#[test]
fn missing_or_unavailable_root_key_locks_without_overwriting_ciphertext() {
    let (path, connection) = database("locked");
    connection
        .execute(
            "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-locked', 'oauth', 'Locked Account', 'default')",
            [],
        )
        .expect("insert account");
    let ciphertext = vec![1u8, 2, 3, 4];
    connection
        .execute(
            "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES (?1, ?2, ?3, ?4, 1)",
            params!["account-locked", "oauth_token", ciphertext, vec![0u8; 12]],
        )
        .expect("insert encrypted credential");

    let missing = FakeKeyStore::default();
    assert!(matches!(
        initialize_security(&connection, &missing),
        SecurityState::Locked(SecurityLockReason::RootKeyMissing)
    ));
    assert_eq!(*missing.store_calls.borrow(), 0);

    let unavailable = FakeKeyStore {
        fail_load: true,
        ..FakeKeyStore::default()
    };
    assert!(matches!(
        initialize_security(&connection, &unavailable),
        SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable)
    ));
    assert_eq!(*unavailable.store_calls.borrow(), 0);
    let preserved: Vec<u8> = connection
        .query_row(
            "SELECT ciphertext FROM ai_gateway_credentials WHERE account_id = 'account-locked'",
            [],
            |row| row.get(0),
        )
        .expect("read preserved ciphertext");
    assert_eq!(preserved, vec![1u8, 2, 3, 4]);
    drop(connection);
    remove_database(&path);
}

#[test]
fn invalid_key_and_store_failure_produce_stable_locked_states() {
    let (path, connection) = database("invalid-key");
    let invalid = FakeKeyStore {
        stored: RefCell::new(Some(vec![1, 2, 3])),
        ..FakeKeyStore::default()
    };
    assert!(matches!(
        initialize_security(&connection, &invalid),
        SecurityState::Locked(SecurityLockReason::RootKeyInvalid)
    ));
    let failing_store = FakeKeyStore {
        fail_store: true,
        ..FakeKeyStore::default()
    };
    assert!(matches!(
        initialize_security(&connection, &failing_store),
        SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable)
    ));
    drop(connection);
    remove_database(&path);
}
