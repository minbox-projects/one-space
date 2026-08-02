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
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier, Mutex,
    },
};

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

struct ConcurrentFakeKeyStore {
    stored: Mutex<Option<Vec<u8>>>,
    store_calls: AtomicUsize,
    initial_load_barrier: Arc<Barrier>,
    initial_load_count: usize,
    initial_loads: AtomicUsize,
}

impl ConcurrentFakeKeyStore {
    fn new(initial_load_count: usize, initial_load_barrier: Arc<Barrier>) -> Self {
        Self {
            stored: Mutex::new(None),
            store_calls: AtomicUsize::new(0),
            initial_load_barrier,
            initial_load_count,
            initial_loads: AtomicUsize::new(0),
        }
    }
}

impl RootKeyStore for ConcurrentFakeKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>, super::error::GatewayError> {
        let load_number = self.initial_loads.fetch_add(1, Ordering::SeqCst);
        let stored = self
            .stored
            .lock()
            .expect("lock concurrent key store")
            .clone();
        if load_number < self.initial_load_count {
            self.initial_load_barrier.wait();
        }
        Ok(stored)
    }

    fn store(&self, key: &[u8]) -> Result<(), super::error::GatewayError> {
        self.store_calls.fetch_add(1, Ordering::SeqCst);
        *self.stored.lock().expect("lock concurrent key store") = Some(key.to_vec());
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
        encrypt_credential(&key, "api_key", "account-7", b"SAFE_FIXTURE_CREDENTIAL")
            .expect("encrypt credential");
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
    let plaintext = "SAFE_FIXTURE_TOKEN";
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
fn concurrent_key_store_creation_reloads_one_key_for_decryption() {
    const CONCURRENT_INITIALIZERS: usize = 8;
    let barrier = Arc::new(Barrier::new(CONCURRENT_INITIALIZERS));
    let store = Arc::new(ConcurrentFakeKeyStore::new(
        CONCURRENT_INITIALIZERS,
        Arc::clone(&barrier),
    ));
    let mut threads = Vec::with_capacity(CONCURRENT_INITIALIZERS);

    for index in 0..CONCURRENT_INITIALIZERS {
        let barrier = Arc::clone(&barrier);
        let store = Arc::clone(&store);
        threads.push(std::thread::spawn(move || {
            let (path, connection) = database(&format!("concurrent-create-{index}"));
            barrier.wait();
            let key = match initialize_security(&connection, store.as_ref()) {
                SecurityState::Ready(key) => key,
                SecurityState::Locked(reason) => {
                    panic!("security initialization locked: {reason:?}")
                }
            };
            let record_id = format!("account-{index}");
            let encrypted =
                encrypt_credential(&key, "oauth_token", &record_id, b"SAFE_FIXTURE_CONCURRENT")
                    .expect("encrypt concurrent credential");
            assert_eq!(
                decrypt_credential(&key, "oauth_token", &record_id, &encrypted)
                    .expect("decrypt concurrent credential"),
                b"SAFE_FIXTURE_CONCURRENT"
            );
            drop(connection);
            remove_database(&path);
            (record_id, encrypted)
        }));
    }

    let encrypted_credentials: Vec<(String, EncryptedCredential)> = threads
        .into_iter()
        .map(|thread| {
            thread
                .join()
                .expect("join concurrent security initialization")
        })
        .collect();
    assert_eq!(store.store_calls.load(Ordering::SeqCst), 1);

    let (reload_path, reload_connection) = database("concurrent-reload");
    let reloaded_key = match initialize_security(&reload_connection, store.as_ref()) {
        SecurityState::Ready(key) => key,
        SecurityState::Locked(reason) => panic!("security reload locked: {reason:?}"),
    };
    assert_eq!(store.store_calls.load(Ordering::SeqCst), 1);
    for (record_id, encrypted) in encrypted_credentials {
        assert_eq!(
            decrypt_credential(&reloaded_key, "oauth_token", &record_id, &encrypted)
                .expect("decrypt with reloaded key"),
            b"SAFE_FIXTURE_CONCURRENT"
        );
    }
    drop(reload_connection);
    remove_database(&reload_path);
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
