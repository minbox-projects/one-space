use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

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
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, 1000, &mut key);
    key
}

pub fn encrypt(data: &str, password: &str) -> Result<String, String> {
    let salt = b"onespace-salt-fixed"; // In a real app, use a per-file salt stored in JSON
    let key_bytes = derive_key(password, salt);
    let key = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;

    let nonce_bytes = [0u8; 12]; // Fixed nonce for simplicity in this context, or random + prepend
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = key
        .encrypt(nonce, data.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(general_purpose::STANDARD.encode(ciphertext))
}

pub fn decrypt(encrypted_data: &str, password: &str) -> Result<String, String> {
    let salt = b"onespace-salt-fixed";
    let key_bytes = derive_key(password, salt);
    let key = Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| e.to_string())?;

    let ciphertext = general_purpose::STANDARD
        .decode(encrypted_data)
        .map_err(|e| e.to_string())?;
    let nonce_bytes = [0u8; 12];
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = key
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| e.to_string())?;
    String::from_utf8(plaintext).map_err(|e| e.to_string())
}
