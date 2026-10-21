use super::inspector::check_domain_match;
use super::nginx_template::generate_nginx_config;
use super::types::SslWatchItemInput;
use super::watchlist::{add_watch_item, load_watchlist, remove_watch_item};

#[test]
fn test_domain_matching_rules() {
    let sans = vec![
        "example.com".to_string(),
        "*.example.com".to_string(),
        "admin.mycorp.org".to_string(),
    ];

    // Exact matches
    assert!(check_domain_match("example.com", &sans, None));
    assert!(check_domain_match("admin.mycorp.org", &sans, None));

    // Wildcard matches
    assert!(check_domain_match("api.example.com", &sans, None));
    assert!(check_domain_match("web.example.com", &sans, None));

    // Mismatches
    assert!(!check_domain_match("other.com", &sans, None));
    assert!(!check_domain_match("nested.api.example.com", &sans, None));
    assert!(!check_domain_match("mycorp.org", &sans, None));

    // Fallback to CN
    assert!(check_domain_match(
        "cn-only.net",
        &[],
        Some("cn-only.net")
    ));
}

#[test]
fn test_nginx_template_generation() {
    let config = generate_nginx_config(
        "example.com",
        &["www.example.com".to_string(), "*.example.com".to_string()],
        Some("/var/ssl"),
    );

    assert!(config.contains("server_name example.com www.example.com *.example.com;"));
    assert!(config.contains("ssl_certificate /var/ssl/example.com/fullchain.pem;"));
    assert!(config.contains("ssl_certificate_key /var/ssl/example.com/privkey.pem;"));
    assert!(config.contains("return 301 https://$host$request_uri;"));
    assert!(config.contains("listen 443 ssl http2;"));
}

#[test]
fn test_watchlist_operations() {
    // Add item
    let input = SslWatchItemInput {
        domain: "test.domain.local".to_string(),
        port: 443,
        alias: Some("Local Test".to_string()),
        alert_days_threshold: 15,
    };

    let item = add_watch_item(input).expect("add watch item");
    assert_eq!(item.domain, "test.domain.local");
    assert_eq!(item.alert_days_threshold, 15);

    // List and check existence
    let list = load_watchlist().expect("load watchlist");
    assert!(list.iter().any(|i| i.id == item.id));

    // Remove
    remove_watch_item(&item.id).expect("remove watch item");
    let after_list = load_watchlist().expect("load watchlist after remove");
    assert!(!after_list.iter().any(|i| i.id == item.id));
}

#[tokio::test]
async fn test_inspect_huanweiyun() {
    let res = super::inspector::inspect_ssl("huanweiyun.com", 443).await;
    match res {
        Ok(check) => {
            assert_eq!(check.domain, "huanweiyun.com");
            assert!(check.handshake_success);
            assert_eq!(check.tls_version.as_deref(), Some("TLSv1.3"));
        }
        Err(e) => {
            println!("Network check skipped or offline: {}", e);
        }
    }
}

#[test]
fn test_ssl_config_normalization() {
    use super::config::{is_valid_hh_mm, normalize_config, SslConfig};

    assert!(is_valid_hh_mm("00:00"));
    assert!(is_valid_hh_mm("09:30"));
    assert!(is_valid_hh_mm("23:59"));
    assert!(!is_valid_hh_mm("24:00"));
    assert!(!is_valid_hh_mm("12:60"));
    assert!(!is_valid_hh_mm("9:30"));
    assert!(!is_valid_hh_mm("invalid"));

    let config = SslConfig {
        auto_check_enabled: true,
        check_times: vec![
            "18:00".to_string(),
            "09:00".to_string(),
            "18:00".to_string(),
            "invalid".to_string(),
        ],
        alert_days_threshold: 0,
    };
    let normalized = normalize_config(config);
    assert_eq!(
        normalized.check_times,
        vec!["09:00".to_string(), "18:00".to_string()]
    );
    assert_eq!(normalized.alert_days_threshold, 30);
}
