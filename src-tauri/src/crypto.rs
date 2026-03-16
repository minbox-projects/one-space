use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use pbkdf2::pbkdf2_hmac;
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const LEGACY_SALT: &[u8] = b"onespace-salt-fixed";
const LEGACY_NONCE: [u8; 12] = [0u8; 12];
const V2_PREFIX: &str = "v2:";
const V2_SALT_LEN: usize = 16;
const V2_NONCE_LEN: usize = 12;
const PBKDF2_ITERATIONS: u32 = 100_000;

pub fn get_local_key_path() -> Result<PathBuf, String> {
    let app_dir = crate::config::get_app_dir()?;
    Ok(app_dir.join(".local_key"))
}

pub fn get_or_init_master_password() -> Result<String, String> {
    let path = get_local_key_path()?;
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| e.to_string())
    } else {
        let new_pass = Uuid::new_v4().to_string();
        fs::write(&path, &new_pass).map_err(|e| e.to_string())?;
        Ok(new_pass)
    }
}

pub fn set_master_password(new_pass: &str) -> Result<(), String> {
    let path = get_local_key_path()?;
    fs::write(&path, new_pass).map_err(|e| e.to_string())?;
    Ok(())
}

fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
    key
}

fn derive_key_legacy(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, 1000, &mut key);
    key
}

pub fn encrypt(data: &str, password: &str) -> Result<String, String> {
    let mut salt = [0u8; V2_SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let mut nonce_bytes = [0u8; V2_NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);

    let key_bytes = derive_key(password, &salt);
    let key = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = key
        .encrypt(nonce, data.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut packed = Vec::with_capacity(V2_SALT_LEN + V2_NONCE_LEN + ciphertext.len());
    packed.extend_from_slice(&salt);
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ciphertext);
    Ok(format!(
        "{}{}",
        V2_PREFIX,
        general_purpose::STANDARD.encode(packed)
    ))
}

pub fn decrypt(encrypted_data: &str, password: &str) -> Result<String, String> {
    if let Some(encoded) = encrypted_data.strip_prefix(V2_PREFIX) {
        let packed = general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| e.to_string())?;
        if packed.len() <= V2_SALT_LEN + V2_NONCE_LEN {
            return Err("Invalid encrypted payload".to_string());
        }

        let salt = &packed[..V2_SALT_LEN];
        let nonce_bytes = &packed[V2_SALT_LEN..V2_SALT_LEN + V2_NONCE_LEN];
        let ciphertext = &packed[V2_SALT_LEN + V2_NONCE_LEN..];

        let key_bytes = derive_key(password, salt);
        let key = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = key.decrypt(nonce, ciphertext).map_err(|e| e.to_string())?;
        return String::from_utf8(plaintext).map_err(|e| e.to_string());
    }

    // Backward compatible decryption for legacy fixed-salt/fixed-nonce payloads.
    let key_bytes = derive_key_legacy(password, LEGACY_SALT);
    let key = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;
    let ciphertext = general_purpose::STANDARD
        .decode(encrypted_data)
        .map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(&LEGACY_NONCE);
    let plaintext = key
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| e.to_string())?;
    String::from_utf8(plaintext).map_err(|e| e.to_string())
}
