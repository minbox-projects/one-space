use super::config::*;
use super::environment::*;
use super::types::*;

#[test]
fn test_validate_tunnel_config_valid_cloudflare() {
    let cfg = TunnelConfig {
        id: "cf-test".to_string(),
        name: "Test CF".to_string(),
        mode: TunnelMode::Cloudflare,
        local_port: 3000,
        protocol: TunnelProtocol::Http,
        remote_port: None,
        frp_server: None,
        frp_token: None,
        custom_domain: None,
        auto_start: false,
        created_at: 0,
        updated_at: 0,
    };
    assert!(validate_tunnel_config(&cfg).is_ok());
}

#[test]
fn test_validate_tunnel_config_invalid_port() {
    let mut cfg = TunnelConfig {
        id: "cf-test".to_string(),
        name: "Test CF".to_string(),
        mode: TunnelMode::Cloudflare,
        local_port: 0,
        protocol: TunnelProtocol::Http,
        remote_port: None,
        frp_server: None,
        frp_token: None,
        custom_domain: None,
        auto_start: false,
        created_at: 0,
        updated_at: 0,
    };
    assert!(validate_tunnel_config(&cfg).is_err());

    cfg.local_port = 8080;
    cfg.name = "".to_string();
    assert!(validate_tunnel_config(&cfg).is_err());
}

#[test]
fn test_validate_tunnel_config_frp_requirements() {
    let mut cfg = TunnelConfig {
        id: "frp-test".to_string(),
        name: "Test FRP".to_string(),
        mode: TunnelMode::Frp,
        local_port: 8080,
        protocol: TunnelProtocol::Http,
        remote_port: None,
        frp_server: None,
        frp_token: None,
        custom_domain: None,
        auto_start: false,
        created_at: 0,
        updated_at: 0,
    };
    // Missing server
    assert!(validate_tunnel_config(&cfg).is_err());

    // Has server but missing remote_port or custom_domain
    cfg.frp_server = Some("frp.example.com".to_string());
    assert!(validate_tunnel_config(&cfg).is_err());

    // Has custom domain
    cfg.custom_domain = Some("sub.example.com".to_string());
    assert!(validate_tunnel_config(&cfg).is_ok());

    // Or has remote_port
    cfg.custom_domain = None;
    cfg.remote_port = Some(9000);
    assert!(validate_tunnel_config(&cfg).is_ok());
}

#[test]
fn test_environment_recommended_command() {
    let cf_cmd = recommended_command_for(TunnelMode::Cloudflare);
    let frp_cmd = recommended_command_for(TunnelMode::Frp);

    #[cfg(target_os = "macos")]
    {
        assert_eq!(cf_cmd, "brew install cloudflared");
        assert_eq!(frp_cmd, "brew install frpc");
    }
    #[cfg(target_os = "windows")]
    {
        assert!(cf_cmd.contains("winget"));
        assert!(frp_cmd.contains("scoop"));
    }
}

#[test]
fn test_json_serialization_roundtrip() {
    let cfg = TunnelConfig {
        id: "tunnel-1".to_string(),
        name: "My Next App".to_string(),
        mode: TunnelMode::Cloudflare,
        local_port: 3000,
        protocol: TunnelProtocol::Http,
        remote_port: None,
        frp_server: None,
        frp_token: None,
        custom_domain: None,
        auto_start: true,
        created_at: 1000,
        updated_at: 2000,
    };

    let json = serde_json::to_string(&cfg).expect("serialize");
    let parsed: TunnelConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(cfg, parsed);
}

#[test]
fn test_extract_cloudflare_url() {
    let sample_output = "2026-10-21T07:00:00Z INF |  https://my-demo-test.trycloudflare.com  |";
    let extracted = super::supervisor::extract_cloudflare_url(sample_output);
    assert_eq!(
        extracted,
        Some("https://my-demo-test.trycloudflare.com".to_string())
    );

    let sample_banner = "+--------------------------------------------------------------------------------------------+\n|  Your quick Tunnel has been created! Visit it at (it may take some time to be reachable):  |\n|  https://quick-tunnel-12345.trycloudflare.com                                                |\n+--------------------------------------------------------------------------------------------+";
    let extracted_banner = super::supervisor::extract_cloudflare_url(sample_banner);
    assert_eq!(
        extracted_banner,
        Some("https://quick-tunnel-12345.trycloudflare.com".to_string())
    );
}

#[test]
fn test_extract_cloudflare_url_negative() {
    let normal_log = "2026-10-21T07:00:00Z INF Connected to edge server";
    assert_eq!(super::supervisor::extract_cloudflare_url(normal_log), None);

    let other_url = "https://github.com/cloudflare/cloudflared";
    assert_eq!(super::supervisor::extract_cloudflare_url(other_url), None);
}

#[test]
fn test_supervisor_initial_status_is_stopped() {
    let status = super::supervisor::get_status("non-existent-tunnel-id");
    assert_eq!(status.state, "stopped");
    assert_eq!(status.public_url, None);
    assert_eq!(status.error, None);

    let all_status = super::supervisor::get_all_status();
    // Should return a vector without panic
    assert!(all_status.is_empty() || !all_status.is_empty());
}
