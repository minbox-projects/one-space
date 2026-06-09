use super::MCPServer;

/// 加密敏感数据
pub fn encrypt_sensitive_data(server: &mut MCPServer) -> Result<(), String> {
    let password = crate::crypto::get_or_init_master_password()?;

    // 加密 env 中的敏感值
    if let Some(ref mut env) = server.env {
        for (_key, value) in env.iter_mut() {
            if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                *value = crate::crypto::encrypt(value, &password)?;
            }
        }
    }

    // 加密 headers 中的敏感值
    if let Some(ref mut headers) = server.headers {
        for (key, value) in headers.iter_mut() {
            if key.to_lowercase().contains("auth")
                || key.to_lowercase().contains("key")
                || key.to_lowercase().contains("token")
                || key.to_lowercase().contains("secret")
            {
                if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                    *value = crate::crypto::encrypt(value, &password)?;
                }
            }
        }
    }

    Ok(())
}

/// 解密敏感数据
pub fn decrypt_sensitive_data(server: &mut MCPServer) -> Result<(), String> {
    let password = crate::crypto::get_or_init_master_password()?;

    if let Some(ref mut env) = server.env {
        for (_, value) in env.iter_mut() {
            if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                if let Ok(decrypted) = crate::crypto::decrypt(value, &password) {
                    *value = decrypted;
                }
            }
        }
    }

    if let Some(ref mut headers) = server.headers {
        for (key, value) in headers.iter_mut() {
            if key.to_lowercase().contains("auth")
                || key.to_lowercase().contains("key")
                || key.to_lowercase().contains("token")
                || key.to_lowercase().contains("secret")
            {
                if !value.is_empty() && !value.starts_with('$') && !value.starts_with("${") {
                    if let Ok(decrypted) = crate::crypto::decrypt(value, &password) {
                        *value = decrypted;
                    }
                }
            }
        }
    }

    Ok(())
}
