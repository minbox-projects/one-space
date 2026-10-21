use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::nginx_template::generate_nginx_config;
use super::types::SslIssuedCertificate;

static CERT_STORAGE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCertificate {
    id: String,
    primary_domain: String,
    san_domains: Vec<String>,
    issuer: String,
    issued_at: i64,
    expires_at: i64,
    key_type: String,
    cert_pem: String,
    chain_pem: String,
    fullchain_pem: String,
    encrypted_privkey: String,
    created_at: i64,
}

fn get_certs_file_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    Ok(dir.join("ssl_certificates.json"))
}

fn load_stored_certificates() -> Result<Vec<StoredCertificate>, String> {
    let path = get_certs_file_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read certificates json: {}", e))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse certificates json: {}", e))
}

fn save_stored_certificates(certs: &[StoredCertificate]) -> Result<(), String> {
    let path = get_certs_file_path()?;
    let json_bytes = serde_json::to_vec_pretty(certs)
        .map_err(|e| format!("Failed to serialize certificates: {}", e))?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, json_bytes)
        .map_err(|e| format!("Failed to write tmp certificates file: {}", e))?;
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename certificates file: {}", e))?;
    Ok(())
}

pub fn save_issued_certificate(
    id: String,
    primary_domain: String,
    san_domains: Vec<String>,
    issuer: String,
    issued_at: i64,
    expires_at: i64,
    key_type: String,
    cert_pem: String,
    chain_pem: String,
    fullchain_pem: String,
    privkey_pem: &str,
) -> Result<SslIssuedCertificate, String> {
    let _guard = CERT_STORAGE_LOCK.lock().map_err(|e| e.to_string())?;

    let password = crate::crypto::get_or_init_master_password()?;
    let encrypted_privkey = crate::crypto::encrypt(privkey_pem, &password)?;

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let stored = StoredCertificate {
        id: id.clone(),
        primary_domain: primary_domain.clone(),
        san_domains: san_domains.clone(),
        issuer: issuer.clone(),
        issued_at,
        expires_at,
        key_type: key_type.clone(),
        cert_pem: cert_pem.clone(),
        chain_pem: chain_pem.clone(),
        fullchain_pem: fullchain_pem.clone(),
        encrypted_privkey,
        created_at: now_ts,
    };

    let mut certs = load_stored_certificates()?;
    certs.retain(|c| c.id != id);
    certs.push(stored);
    save_stored_certificates(&certs)?;

    let nginx_config = generate_nginx_config(&primary_domain, &san_domains, None);
    let days_remaining = (expires_at - now_ts) / 86400;

    Ok(SslIssuedCertificate {
        id,
        primary_domain,
        san_domains,
        issuer,
        issued_at,
        expires_at,
        days_remaining,
        key_type,
        cert_pem,
        chain_pem,
        fullchain_pem,
        privkey_pem: Some(privkey_pem.to_string()),
        nginx_config,
        created_at: now_ts,
    })
}

pub fn list_issued_certificates() -> Result<Vec<SslIssuedCertificate>, String> {
    let _guard = CERT_STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let certs = load_stored_certificates()?;
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut result = Vec::new();
    for c in certs {
        let days_remaining = (c.expires_at - now_ts) / 86400;
        let nginx_config = generate_nginx_config(&c.primary_domain, &c.san_domains, None);

        // Try decrypting privkey if password exists
        let privkey_pem = match crate::crypto::get_or_init_master_password() {
            Ok(pwd) => crate::crypto::decrypt(&c.encrypted_privkey, &pwd).ok(),
            Err(_) => None,
        };

        result.push(SslIssuedCertificate {
            id: c.id,
            primary_domain: c.primary_domain,
            san_domains: c.san_domains,
            issuer: c.issuer,
            issued_at: c.issued_at,
            expires_at: c.expires_at,
            days_remaining,
            key_type: c.key_type,
            cert_pem: c.cert_pem,
            chain_pem: c.chain_pem,
            fullchain_pem: c.fullchain_pem,
            privkey_pem,
            nginx_config,
            created_at: c.created_at,
        });
    }

    Ok(result)
}

pub fn delete_issued_certificate(id: &str) -> Result<(), String> {
    let _guard = CERT_STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let mut certs = load_stored_certificates()?;
    certs.retain(|c| c.id != id);
    save_stored_certificates(&certs)
}

pub fn export_certificate_files(id: &str, target_dir: &str) -> Result<String, String> {
    let _guard = CERT_STORAGE_LOCK.lock().map_err(|e| e.to_string())?;
    let certs = load_stored_certificates()?;
    let cert = certs
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("Certificate not found: {}", id))?;

    let pwd = crate::crypto::get_or_init_master_password()?;
    let privkey_pem = crate::crypto::decrypt(&cert.encrypted_privkey, &pwd)?;

    let target_path = Path::new(target_dir);
    if !target_path.exists() {
        return Err(format!("Target directory does not exist: {}", target_dir));
    }

    let folder_name = format!("{}_ssl", cert.primary_domain.replace('*', "wildcard"));
    let out_dir = target_path.join(folder_name);
    fs::create_dir_all(&out_dir).map_err(|e| format!("Failed to create export dir: {}", e))?;

    let fullchain_path = out_dir.join("fullchain.pem");
    let privkey_path = out_dir.join("privkey.pem");
    let cert_path = out_dir.join("cert.pem");
    let chain_path = out_dir.join("chain.pem");
    let nginx_path = out_dir.join("nginx.conf");
    let readme_path = out_dir.join("README.md");

    fs::write(&fullchain_path, &cert.fullchain_pem)
        .map_err(|e| format!("Failed to write fullchain.pem: {}", e))?;
    fs::write(&privkey_path, &privkey_pem)
        .map_err(|e| format!("Failed to write privkey.pem: {}", e))?;
    fs::write(&cert_path, &cert.cert_pem)
        .map_err(|e| format!("Failed to write cert.pem: {}", e))?;
    fs::write(&chain_path, &cert.chain_pem)
        .map_err(|e| format!("Failed to write chain.pem: {}", e))?;

    let nginx_content = generate_nginx_config(
        &cert.primary_domain,
        &cert.san_domains,
        Some(&out_dir.to_string_lossy()),
    );
    fs::write(&nginx_path, &nginx_content)
        .map_err(|e| format!("Failed to write nginx.conf: {}", e))?;

    let readme_content = format!(
r#"# SSL Certificate Deployment Guide ({domain})

Files in this directory:
- `fullchain.pem`: Complete certificate chain (Server Cert + Intermediate CA Cert). Use this for Nginx `ssl_certificate`.
- `privkey.pem`: Private key. Use this for Nginx `ssl_certificate_key`.
- `cert.pem`: Server certificate only.
- `chain.pem`: Intermediate CA certificate only.
- `nginx.conf`: Ready-to-use Nginx configuration template.

Deployment instructions for Nginx:
1. Copy `fullchain.pem` and `privkey.pem` to your server (e.g. `/etc/nginx/ssl/{domain}/`).
2. Update your Nginx server block or include `nginx.conf`.
3. Test your configuration: `nginx -t`
4. Reload Nginx: `nginx -s reload`
"#,
        domain = cert.primary_domain
    );
    fs::write(&readme_path, readme_content)
        .map_err(|e| format!("Failed to write README.md: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&privkey_path, fs::Permissions::from_mode(0o600));
    }

    Ok(out_dir.to_string_lossy().to_string())
}
