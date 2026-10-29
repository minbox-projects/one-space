use std::collections::HashMap;
use std::sync::Mutex;

use instant_acme::{
    Account, AccountCredentials, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    OrderStatus, RetryPolicy,
};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use uuid::Uuid;

#[path = "acme_http.rs"]
mod acme_http;

use acme_http::AcmeHttpClient;

use super::dns_verifier::{cloudflare_add_txt_record, cloudflare_delete_txt_record};
use super::storage::save_issued_certificate;
use super::types::{SslChallengeItem, SslIssuedCertificate, SslOrderRequest, SslOrderResponse};

static ACTIVE_ORDERS: Mutex<Option<HashMap<String, ActiveOrderState>>> = Mutex::new(None);

#[cfg(test)]
static TEST_DIRECTORY_URL: Mutex<Option<String>> = Mutex::new(None);

#[cfg(test)]
pub(crate) struct TestDirectoryUrlGuard(Option<String>);

#[cfg(test)]
impl Drop for TestDirectoryUrlGuard {
    fn drop(&mut self) {
        *TEST_DIRECTORY_URL
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = self.0.take();
    }
}

#[cfg(test)]
pub(crate) fn set_test_directory_url(url: impl Into<String>) -> TestDirectoryUrlGuard {
    let mut slot = TEST_DIRECTORY_URL
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = slot.replace(url.into());
    TestDirectoryUrlGuard(previous)
}

#[cfg(test)]
fn test_directory_url() -> Option<String> {
    TEST_DIRECTORY_URL
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[cfg(test)]
fn test_default_directory_url(use_staging: bool) -> String {
    if use_staging {
        LetsEncrypt::Staging.url().to_string()
    } else {
        LetsEncrypt::Production.url().to_string()
    }
}

#[allow(dead_code)]
struct ActiveOrderState {
    order_id: String,
    primary_domain: String,
    san_domains: Vec<String>,
    key_type: String,
    private_key_pem: String,
    csr_der: Vec<u8>,
    account_creds_json: String,
    order_url: String,
    cf_handles: Vec<String>,
    cf_token: Option<String>,
}

fn with_orders<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, ActiveOrderState>) -> R,
{
    let mut guard = ACTIVE_ORDERS.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    f(guard.as_mut().unwrap())
}

pub async fn create_acme_order(req: SslOrderRequest) -> Result<SslOrderResponse, String> {
    let clean_domain = req.domain.trim().to_lowercase();
    if clean_domain.is_empty() {
        return Err("Primary domain cannot be empty".to_string());
    }

    let mut all_domains = vec![clean_domain.clone()];
    for s in req.san_domains.clone() {
        let clean_san = s.trim().to_lowercase();
        if !clean_san.is_empty() && !all_domains.contains(&clean_san) {
            all_domains.push(clean_san);
        }
    }

    // 1. Choose directory (Production vs Staging)
    #[cfg(test)]
    let directory_url =
        test_directory_url().unwrap_or_else(|| test_default_directory_url(req.use_staging));
    #[cfg(not(test))]
    let directory_url = if req.use_staging {
        LetsEncrypt::Staging.url().to_string()
    } else {
        LetsEncrypt::Production.url().to_string()
    };

    // 2. Generate key pair and CSR
    let key_pair = if req.key_type == "rsa2048" {
        KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256).map_err(|e| e.to_string())?
    } else {
        KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(|e| e.to_string())?
    };

    let private_key_pem = key_pair.serialize_pem();

    let mut params = CertificateParams::new(all_domains.clone()).map_err(|e| e.to_string())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, clean_domain.clone());
    params.distinguished_name = dn;

    let csr = params.serialize_request(&key_pair).map_err(|e| e.to_string())?;
    let csr_der = csr.der().to_vec();

    // 3. Create ACME Account
    let contact: Vec<String> = if let Some(ref em) = req.email {
        if !em.trim().is_empty() {
            vec![format!("mailto:{}", em.trim())]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let contact_refs: Vec<&str> = contact.iter().map(|s| s.as_str()).collect();

    let (account, creds) = Account::builder_with_http(Box::new(AcmeHttpClient::new()))
        .create(
            &NewAccount {
                contact: &contact_refs,
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url,
            None,
        )
        .await
        .map_err(|e| format!("Failed to create ACME account: {}", e))?;

    let account_creds_json = serde_json::to_string(&creds)
        .map_err(|e| format!("Failed to serialize credentials: {}", e))?;

    // 4. Create New Order
    let identifiers: Vec<Identifier> = all_domains
        .iter()
        .map(|d| Identifier::Dns(d.clone()))
        .collect();

    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .map_err(|e| format!("Failed to create ACME order: {}", e))?;

    // 5. Extract DNS-01 Challenges
    let mut challenge_items = Vec::new();
    let mut authorizations = order.authorizations();
    while let Some(res) = authorizations.next().await {
        let mut authz = res.map_err(|e| format!("Failed to fetch authorization: {}", e))?;
        let domain_str = authz.identifier().to_string();

        if let Some(ch) = authz.challenge(ChallengeType::Dns01) {
            let txt_value = ch.key_authorization().dns_value();
            let record_name = format!("_acme-challenge.{}", domain_str.trim_start_matches("*."));

            challenge_items.push(SslChallengeItem {
                domain: domain_str.clone(),
                record_name: record_name.clone(),
                record_type: "TXT".to_string(),
                record_value: txt_value,
            });
        }
    }

    // 6. If Cloudflare API Token is provided, automatically add records
    let mut cf_handles = Vec::new();
    if let Some(ref cf_token) = req.cloudflare_api_token {
        if !cf_token.trim().is_empty() {
            for item in &challenge_items {
                match cloudflare_add_txt_record(
                    cf_token,
                    &item.domain,
                    &item.record_name,
                    &item.record_value,
                )
                .await
                {
                    Ok(handle) => cf_handles.push(handle),
                    Err(e) => {
                        log::warn!("Cloudflare auto DNS error for {}: {}", item.domain, e);
                    }
                }
            }
        }
    }

    let order_id = Uuid::new_v4().to_string();
    let order_url = order.url().to_string();

    let state = ActiveOrderState {
        order_id: order_id.clone(),
        primary_domain: clean_domain,
        san_domains: req.san_domains,
        key_type: req.key_type,
        private_key_pem,
        csr_der,
        account_creds_json,
        order_url,
        cf_handles,
        cf_token: req.cloudflare_api_token,
    };

    with_orders(|orders| {
        orders.insert(order_id.clone(), state);
    });

    Ok(SslOrderResponse {
        order_id,
        challenges: challenge_items,
        status: "pending_challenge".to_string(),
    })
}

pub async fn verify_and_finalize_order(order_id: &str) -> Result<SslIssuedCertificate, String> {
    let state = with_orders(|orders| orders.remove(order_id))
        .ok_or_else(|| format!("Order not found or already completed: {}", order_id))?;

    let creds: AccountCredentials = serde_json::from_str(&state.account_creds_json)
        .map_err(|e| format!("Invalid credentials JSON: {}", e))?;

    let account = Account::builder_with_http(Box::new(AcmeHttpClient::new()))
        .from_credentials(creds)
        .await
        .map_err(|e| format!("Failed to restore ACME account: {}", e))?;

    let mut order = account
        .order(state.order_url.clone())
        .await
        .map_err(|e| format!("Failed to fetch order: {}", e))?;

    // 1. Notify ACME server that each challenge is ready
    let mut authorizations = order.authorizations();
    while let Some(res) = authorizations.next().await {
        let mut authz = res.map_err(|e| format!("Authorization error: {}", e))?;
        if let Some(mut ch) = authz.challenge(ChallengeType::Dns01) {
            ch.set_ready()
                .await
                .map_err(|e| format!("Failed to set challenge ready: {}", e))?;
        }
    }

    // 2. Poll order status until Ready or Valid
    let status = order
        .poll_ready(&RetryPolicy::default())
        .await
        .map_err(|e| format!("Order poll ready error: {}", e))?;

    if status != OrderStatus::Ready && status != OrderStatus::Valid {
        cleanup_cf(&state).await;
        return Err(format!("Unexpected order status: {:?}", status));
    }

    // 3. Finalize order with CSR
    order
        .finalize_csr(&state.csr_der)
        .await
        .map_err(|e| format!("Failed to finalize CSR: {}", e))?;

    // 4. Download certificate
    let fullchain_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await
        .map_err(|e| format!("Failed to download certificate: {}", e))?;

    // Clean up temporary Cloudflare records
    cleanup_cf(&state).await;

    // 5. Split fullchain.pem into cert.pem and chain.pem
    let (cert_pem, chain_pem) = split_fullchain(&fullchain_pem);

    // 6. Parse certificate to extract timestamps and issuer
    let (issued_at, expires_at, issuer) = parse_cert_metadata(&cert_pem);

    let cert_id = Uuid::new_v4().to_string();

    save_issued_certificate(
        cert_id,
        state.primary_domain,
        state.san_domains,
        issuer,
        issued_at,
        expires_at,
        state.key_type,
        cert_pem,
        chain_pem,
        fullchain_pem,
        &state.private_key_pem,
    )
}

async fn cleanup_cf(state: &ActiveOrderState) {
    if let Some(ref token) = state.cf_token {
        for handle in &state.cf_handles {
            let _ = cloudflare_delete_txt_record(token, handle).await;
        }
    }
}

fn split_fullchain(fullchain: &str) -> (String, String) {
    let mut certs = Vec::new();
    let mut current = String::new();
    for line in fullchain.lines() {
        current.push_str(line);
        current.push('\n');
        if line.contains("END CERTIFICATE") {
            certs.push(current.clone());
            current.clear();
        }
    }

    if certs.is_empty() {
        return (fullchain.to_string(), String::new());
    }

    let leaf = certs[0].clone();
    let chain = certs[1..].join("");
    (leaf, chain)
}

fn parse_cert_metadata(cert_pem: &str) -> (i64, i64, String) {
    for pem in x509_parser::pem::Pem::iter_from_buffer(cert_pem.as_bytes()) {
        if let Ok(pem) = pem {
            if let Ok((_, cert)) = x509_parser::parse_x509_certificate(&pem.contents) {
                let not_before = cert.validity().not_before.timestamp();
                let not_after = cert.validity().not_after.timestamp();
                let issuer = cert
                    .issuer()
                    .iter_common_name()
                    .next()
                    .and_then(|a| a.as_str().ok())
                    .unwrap_or("Let's Encrypt")
                    .to_string();
                return (not_before, not_after, issuer);
            }
        }
    }
    (0, 0, "Let's Encrypt".to_string())
}
