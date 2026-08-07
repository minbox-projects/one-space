use super::{
    error::GatewayErrorCategory,
    security::{
        decrypt_credential, encrypt_credential, initialize_security,
        initialize_security_with_migration, EncryptedCredential, LegacyRootKeyStore,
        LocalRootKeyStore, RootKey, RootKeyStore, SecurityLockReason, SecurityState,
    },
};
use crate::shared_sqlite;
use base64::Engine as _;
use rusqlite::params;
use std::{cell::RefCell, path::PathBuf, sync::Arc};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

#[derive(Default)]
struct FakeLegacyKeyStore {
    stored: RefCell<Option<Vec<u8>>>,
    fail_load: bool,
    load_calls: RefCell<usize>,
    delete_calls: RefCell<usize>,
}

impl LegacyRootKeyStore for FakeLegacyKeyStore {
    fn load(&self) -> Result<Option<Vec<u8>>, super::error::GatewayError> {
        *self.load_calls.borrow_mut() += 1;
        if self.fail_load {
            return Err(super::error::GatewayError::new(
                GatewayErrorCategory::CredentialStoreUnavailable,
                None,
            ));
        }
        Ok(self.stored.borrow().clone())
    }

    fn delete(&self) -> Result<(), super::error::GatewayError> {
        *self.delete_calls.borrow_mut() += 1;
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

fn key_file(name: &str) -> (PathBuf, LocalRootKeyStore) {
    let directory = std::env::temp_dir().join(format!(
        "onespace-ai-gateway-root-key-{name}-{}",
        uuid::Uuid::new_v4()
    ));
    let path = directory.join("ai-routing-gateway-root-key-v1");
    (directory, LocalRootKeyStore::at(path))
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
fn local_key_store_creates_raw_key_with_private_permissions() {
    let (directory, store) = key_file("permissions");
    let (database_path, connection) = database("local-permissions");
    assert!(matches!(
        initialize_security(&connection, &store),
        SecurityState::Ready(_)
    ));
    let key_path = directory.join("ai-routing-gateway-root-key-v1");
    assert_eq!(
        std::fs::read(&key_path).expect("read raw root key").len(),
        32
    );
    #[cfg(unix)]
    {
        assert_eq!(
            std::fs::metadata(&directory)
                .expect("read directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&key_path)
                .expect("read key metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert!(std::fs::read_dir(&directory)
        .expect("list key directory")
        .all(|entry| !entry
            .expect("read directory entry")
            .file_name()
            .to_string_lossy()
            .contains(".tmp-")));
    drop(connection);
    remove_database(&database_path);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn concurrent_local_initialization_reloads_one_persistent_key() {
    const CONCURRENT_INITIALIZERS: usize = 8;
    let (directory, store) = key_file("concurrent");
    let store = Arc::new(store);
    let mut threads = Vec::with_capacity(CONCURRENT_INITIALIZERS);

    for index in 0..CONCURRENT_INITIALIZERS {
        let store = Arc::clone(&store);
        threads.push(std::thread::spawn(move || {
            let (path, connection) = database(&format!("concurrent-create-{index}"));
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

    let (reload_path, reload_connection) = database("concurrent-reload");
    let reloaded_key = match initialize_security(&reload_connection, store.as_ref()) {
        SecurityState::Ready(key) => key,
        SecurityState::Locked(reason) => panic!("security reload locked: {reason:?}"),
    };
    for (record_id, encrypted) in encrypted_credentials {
        assert_eq!(
            decrypt_credential(&reloaded_key, "oauth_token", &record_id, &encrypted)
                .expect("decrypt with reloaded key"),
            b"SAFE_FIXTURE_CONCURRENT"
        );
    }
    drop(reload_connection);
    remove_database(&reload_path);
    assert_eq!(
        std::fs::read(directory.join("ai-routing-gateway-root-key-v1"))
            .expect("read persistent root key")
            .len(),
        32
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn migration_validates_credentials_and_gateway_keys_before_persisting() {
    let (directory, store) = key_file("migration-success");
    let (database_path, connection) = database("migration-success");
    let candidate = root_key(41);
    connection
        .execute(
            "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-migrate', 'api_key', 'Migrate', 'default')",
            [],
        )
        .expect("insert migration account");
    let credential = encrypt_credential(
        &candidate,
        "third_party_api_key",
        "account-migrate",
        b"SAFE_FIXTURE_ACCOUNT_KEY",
    )
    .expect("encrypt account credential");
    connection
        .execute(
            "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES ('account-migrate', 'third_party_api_key', ?1, ?2, ?3)",
            params![credential.ciphertext, credential.nonce.as_slice(), credential.cipher_version],
        )
        .expect("insert account credential");
    let gateway_key = encrypt_credential(
        &candidate,
        "gateway_api_key",
        "key-migrate",
        b"SAFE_FIXTURE_GATEWAY_KEY",
    )
    .expect("encrypt gateway key");
    connection
        .execute(
            "INSERT INTO ai_gateway_api_keys (id, name, key_prefix, key_hash, hash_salt, ciphertext, nonce, cipher_version) VALUES ('key-migrate', 'Migrate', 'osk_migrate', X'01', X'02', ?1, ?2, ?3)",
            params![gateway_key.ciphertext, gateway_key.nonce.as_slice(), gateway_key.cipher_version],
        )
        .expect("insert gateway key");
    let legacy = FakeLegacyKeyStore {
        stored: RefCell::new(Some(vec![41; 32])),
        ..FakeLegacyKeyStore::default()
    };

    assert!(matches!(
        initialize_security_with_migration(&connection, &store, Some(&legacy)),
        SecurityState::Ready(_)
    ));
    assert_eq!(*legacy.load_calls.borrow(), 1);
    assert_eq!(*legacy.delete_calls.borrow(), 1);
    assert_eq!(
        std::fs::read(directory.join("ai-routing-gateway-root-key-v1")).expect("read migrated key"),
        vec![41; 32]
    );
    assert!(matches!(
        initialize_security_with_migration(&connection, &store, Some(&legacy)),
        SecurityState::Ready(_)
    ));
    assert_eq!(*legacy.load_calls.borrow(), 1);
    drop(connection);
    remove_database(&database_path);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn failed_migration_preserves_ciphertext_and_does_not_write_or_delete() {
    let (directory, store) = key_file("migration-failure");
    let (database_path, connection) = database("migration-failure");
    connection
        .execute(
            "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-locked', 'oauth', 'Locked', 'default')",
            [],
        )
        .expect("insert locked account");
    let encrypted = encrypt_credential(
        &root_key(51),
        "oauth_token",
        "account-locked",
        b"SAFE_FIXTURE_TOKEN",
    )
    .expect("encrypt locked credential");
    let original_ciphertext = encrypted.ciphertext.clone();
    connection
        .execute(
            "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES ('account-locked', 'oauth_token', ?1, ?2, ?3)",
            params![encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
        )
        .expect("insert locked credential");
    let wrong_legacy = FakeLegacyKeyStore {
        stored: RefCell::new(Some(vec![52; 32])),
        ..FakeLegacyKeyStore::default()
    };

    assert!(matches!(
        initialize_security_with_migration(&connection, &store, Some(&wrong_legacy)),
        SecurityState::Locked(SecurityLockReason::MigrationValidationFailed)
    ));
    assert!(!directory.join("ai-routing-gateway-root-key-v1").exists());
    assert_eq!(*wrong_legacy.delete_calls.borrow(), 0);
    let preserved: Vec<u8> = connection
        .query_row(
            "SELECT ciphertext FROM ai_gateway_credentials WHERE account_id = 'account-locked'",
            [],
            |row| row.get(0),
        )
        .expect("read preserved migration ciphertext");
    assert_eq!(preserved, original_ciphertext);
    drop(connection);
    remove_database(&database_path);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn migration_persistence_failure_keeps_legacy_key_and_ciphertext() {
    let (database_path, connection) = database("migration-store-failure");
    connection
        .execute(
            "INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-store-failure', 'oauth', 'Locked', 'default')",
            [],
        )
        .expect("insert account");
    let encrypted = encrypt_credential(
        &root_key(53),
        "oauth_token",
        "account-store-failure",
        b"SAFE_FIXTURE_TOKEN",
    )
    .expect("encrypt credential");
    let original_ciphertext = encrypted.ciphertext.clone();
    connection
        .execute(
            "INSERT INTO ai_gateway_credentials (account_id, record_type, ciphertext, nonce, cipher_version) VALUES ('account-store-failure', 'oauth_token', ?1, ?2, ?3)",
            params![encrypted.ciphertext, encrypted.nonce.as_slice(), encrypted.cipher_version],
        )
        .expect("insert credential");
    let local = FakeKeyStore {
        fail_store: true,
        ..FakeKeyStore::default()
    };
    let legacy = FakeLegacyKeyStore {
        stored: RefCell::new(Some(vec![53; 32])),
        ..FakeLegacyKeyStore::default()
    };

    assert!(matches!(
        initialize_security_with_migration(&connection, &local, Some(&legacy)),
        SecurityState::Locked(SecurityLockReason::CredentialStoreUnavailable)
    ));
    assert_eq!(*local.store_calls.borrow(), 1);
    assert_eq!(*legacy.delete_calls.borrow(), 0);
    let preserved: Vec<u8> = connection
        .query_row(
            "SELECT ciphertext FROM ai_gateway_credentials WHERE account_id = 'account-store-failure'",
            [],
            |row| row.get(0),
        )
        .expect("read preserved ciphertext");
    assert_eq!(preserved, original_ciphertext);
    drop(connection);
    remove_database(&database_path);
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
