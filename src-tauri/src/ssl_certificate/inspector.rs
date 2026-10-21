use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::parse_x509_certificate;

use super::types::{CertificateNode, SslCheckResult};

#[derive(Debug, Default)]
struct CapturedTlsData {
    certificates_der: Vec<Vec<u8>>,
    validation_error: Option<String>,
}

#[derive(Debug, Clone)]
struct CapturingVerifier {
    captured: Arc<Mutex<CapturedTlsData>>,
    default_verifier: Arc<dyn ServerCertVerifier>,
}

impl CapturingVerifier {
    fn new(captured: Arc<Mutex<CapturedTlsData>>, provider: Arc<rustls::crypto::CryptoProvider>) -> Self {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let default_verifier =
            rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(root_store), provider.clone())
                .build()
                .unwrap_or_else(|_| {
                    // Fallback to empty if builder fails
                    let empty_store = rustls::RootCertStore::empty();
                    rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(empty_store), provider)
                        .build()
                        .expect("fallback verifier")
                });

        Self {
            captured,
            default_verifier,
        }
    }
}

impl ServerCertVerifier for CapturingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let mut data = self.captured.lock().unwrap();
        data.certificates_der.clear();
        data.certificates_der.push(end_entity.as_ref().to_vec());
        for inter in intermediates {
            data.certificates_der.push(inter.as_ref().to_vec());
        }

        // Try standard verification
        match self.default_verifier.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Ok(verified) => Ok(verified),
            Err(err) => {
                data.validation_error = Some(err.to_string());
                // Return assertion so that handshake completes and we can inspect the certs
                Ok(ServerCertVerified::assertion())
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.default_verifier
            .verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.default_verifier
            .verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.default_verifier.supported_verify_schemes()
    }
}

pub async fn inspect_ssl(domain: &str, port: u16) -> Result<SslCheckResult, String> {
    let clean_domain = domain
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(domain)
        .trim();

    if clean_domain.is_empty() {
        return Err("Domain cannot be empty".to_string());
    }

    // 1. Resolve socket address
    let host_port = format!("{}:{}", clean_domain, port);
    let resolved_addrs = tokio::net::lookup_host(&host_port)
        .await
        .map_err(|e| format!("Failed to resolve host {}: {}", clean_domain, e))?
        .collect::<Vec<SocketAddr>>();

    let target_addr = resolved_addrs
        .first()
        .ok_or_else(|| format!("Could not find any IP address for {}", clean_domain))?;

    let ip_str = target_addr.ip().to_string();

    // 2. TCP connect with timeout
    let tcp_stream = tokio::time::timeout(
        Duration::from_secs(10),
        TcpStream::connect(*target_addr),
    )
    .await
    .map_err(|_| format!("Connection timeout to {}:{}", clean_domain, port))?
    .map_err(|e| format!("TCP connection failed to {}:{}: {}", clean_domain, port, e))?;

    // 3. Build TLS client config with explicit ring provider and CapturingVerifier
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let captured = Arc::new(Mutex::new(CapturedTlsData::default()));
    let verifier = Arc::new(CapturingVerifier::new(captured.clone(), provider.clone()));

    let client_config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS protocol configuration error: {}", e))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from(clean_domain.to_string())
        .map_err(|e| format!("Invalid server name {}: {}", clean_domain, e))?;

    // 4. Perform TLS handshake
    let tls_stream = tokio::time::timeout(
        Duration::from_secs(10),
        connector.connect(server_name, tcp_stream),
    )
    .await
    .map_err(|_| format!("TLS handshake timeout with {}", clean_domain))?
    .map_err(|e| format!("TLS handshake failed with {}: {}", clean_domain, e))?;

    let (_io, session) = tls_stream.get_ref();
    let tls_version = session.protocol_version().map(|v| match v {
        rustls::ProtocolVersion::TLSv1_2 => "TLSv1.2".to_string(),
        rustls::ProtocolVersion::TLSv1_3 => "TLSv1.3".to_string(),
        other => format!("{:?}", other),
    });

    let cipher_suite = session
        .negotiated_cipher_suite()
        .map(|cs| format!("{:?}", cs.suite()));

    let captured_guard = captured.lock().unwrap();
    let certs_der = captured_guard.certificates_der.clone();
    let val_error = captured_guard.validation_error.clone();
    drop(captured_guard);

    if certs_der.is_empty() {
        return Err("No certificates returned by server during handshake".to_string());
    }

    // 5. Parse certificates with x509-parser
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut cert_nodes = Vec::new();
    let mut domain_matches = false;
    let mut primary_days_remaining = 0i64;
    let mut primary_is_expired = false;

    for (idx, der_bytes) in certs_der.iter().enumerate() {
        match parse_x509_certificate(der_bytes) {
            Ok((_, parsed_cert)) => {
                let node = convert_cert_node(&parsed_cert, der_bytes, now_ts);
                if idx == 0 {
                    primary_days_remaining = node.days_remaining;
                    primary_is_expired = node.days_remaining <= 0;
                    domain_matches = check_domain_match(clean_domain, &node.sans, node.subject_cn.as_deref());
                }
                cert_nodes.push(node);
            }
            Err(e) => {
                log::warn!("Failed to parse certificate index {}: {:?}", idx, e);
            }
        }
    }

    let is_valid = val_error.is_none() && !primary_is_expired && domain_matches;
    let is_expiring_soon = primary_days_remaining > 0 && primary_days_remaining <= 30;

    Ok(SslCheckResult {
        domain: clean_domain.to_string(),
        port,
        ip_address: Some(ip_str),
        handshake_success: true,
        tls_version,
        cipher_suite,
        is_valid,
        validation_error: val_error,
        days_remaining: primary_days_remaining,
        is_expired: primary_is_expired,
        is_expiring_soon,
        domain_matches,
        certificates: cert_nodes,
    })
}

fn convert_cert_node(
    cert: &x509_parser::certificate::X509Certificate<'_>,
    raw_der: &[u8],
    now_ts: i64,
) -> CertificateNode {
    let subject = cert.subject().to_string();
    let subject_cn = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .map(|s| s.to_string());

    let issuer = cert.issuer().to_string();
    let issuer_cn = cert
        .issuer()
        .iter_common_name()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .map(|s| s.to_string());

    let issuer_org = cert
        .issuer()
        .iter_organization()
        .next()
        .and_then(|attr| attr.as_str().ok())
        .map(|s| s.to_string());

    let not_before = cert.validity().not_before.timestamp();
    let not_after = cert.validity().not_after.timestamp();
    let days_remaining = (not_after - now_ts) / 86400;

    let mut sans = Vec::new();
    for ext in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san_ext) = ext.parsed_extension() {
            for general_name in &san_ext.general_names {
                match general_name {
                    GeneralName::DNSName(name) => sans.push(name.to_string()),
                    GeneralName::IPAddress(ip) => {
                        if ip.len() == 4 {
                            sans.push(format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let serial_number = format_serial_number(&cert.serial.to_bytes_be());
    let signature_algorithm = cert.signature_algorithm.algorithm.to_string();
    let public_key_algorithm = cert.public_key().algorithm.algorithm.to_string();

    let mut hasher = Sha256::new();
    hasher.update(raw_der);
    let sha256_fingerprint = format_hex_colon(&hasher.finalize());

    let is_root = subject == issuer;
    let is_ca = cert.is_ca();

    CertificateNode {
        subject,
        subject_cn,
        issuer,
        issuer_cn,
        issuer_org,
        not_before,
        not_after,
        days_remaining,
        sans,
        serial_number,
        signature_algorithm,
        public_key_algorithm,
        sha256_fingerprint,
        is_root,
        is_ca,
    }
}

pub fn check_domain_match(domain: &str, sans: &[String], cn: Option<&str>) -> bool {
    let lower_domain = domain.to_lowercase();
    for san in sans {
        if matches_pattern(&lower_domain, &san.to_lowercase()) {
            return true;
        }
    }
    if let Some(c) = cn {
        if matches_pattern(&lower_domain, &c.to_lowercase()) {
            return true;
        }
    }
    false
}

fn matches_pattern(domain: &str, pattern: &str) -> bool {
    if pattern == domain {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        if let Some(dot_idx) = domain.find('.') {
            let domain_suffix = &domain[dot_idx + 1..];
            return domain_suffix == suffix;
        }
    }
    false
}

fn format_serial_number(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":")
}

fn format_hex_colon(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":")
}
