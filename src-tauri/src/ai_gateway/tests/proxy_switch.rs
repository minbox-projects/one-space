use crate::ai_gateway::forwarding::forward_non_streaming_with_protocol;
use super::provider;
use crate::ai_gateway::UpstreamProtocol;
use crate::proxy::test_support::{HttpProxyRecorder, SERIAL_GUARD};
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn gateway_forwarding_proxy_switch_a_then_b() {
    let _g = SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let proxy_a = HttpProxyRecorder::new("A").await;
    let proxy_b = HttpProxyRecorder::new("B").await;

    fn port_of(url: &str) -> u16 {
        url.trim_start_matches("http://").split(':').nth(1).unwrap().parse().unwrap()
    }

    let manager = crate::proxy::PROXY_MANAGER.get_or_init(|| Arc::new(crate::proxy::ProxyManager::new()));
    manager.publish(manager.prepare(Some(crate::config::ProxyConfig {
        proxy_enabled: true,
        proxy_type: "http".to_string(),
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: port_of(&proxy_a.url()),
        proxy_username: None,
        proxy_password: None,
        check_interval: 15,
    })).unwrap());

    let mut p = provider("p");
    p.base_url = "http://upstream.gateway.test/v1".to_string();
    let settings = crate::ai_gateway::GatewaySettings::default();
    let headers = HashMap::new();

    let _ = forward_non_streaming_with_protocol(&p, "/chat", b"{}", "m", &headers, UpstreamProtocol::ChatCompletions, &settings).await;
    assert_eq!(proxy_a.targets(), vec!["http://upstream.gateway.test/v1/chat".to_string()]);

    manager.publish(manager.prepare(Some(crate::config::ProxyConfig {
        proxy_enabled: true,
        proxy_type: "http".to_string(),
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: port_of(&proxy_b.url()),
        proxy_username: None,
        proxy_password: None,
        check_interval: 15,
    })).unwrap());

    let _ = forward_non_streaming_with_protocol(&p, "/chat", b"{}", "m", &headers, UpstreamProtocol::ChatCompletions, &settings).await;
    assert_eq!(proxy_b.targets(), vec!["http://upstream.gateway.test/v1/chat".to_string()]);

    // 500-then-retry: A returns 500, switch to B before retry
    let proxy_a2 = HttpProxyRecorder::new_with_status("A", 500).await;
    let proxy_b2 = HttpProxyRecorder::new("B").await;
    manager.publish(manager.prepare(Some(crate::config::ProxyConfig {
        proxy_enabled: true,
        proxy_type: "http".to_string(),
        proxy_host: "127.0.0.1".to_string(),
        proxy_port: port_of(&proxy_a2.url()),
        proxy_username: None,
        proxy_password: None,
        check_interval: 15,
    })).unwrap());
    proxy_a2.set_on_hit(Arc::new({
        let manager = manager.clone();
        let proxy_b2_url = proxy_b2.url();
        move || {
            let mgr = manager.clone();
            let url = proxy_b2_url.clone();
            // publish B synchronously via blocking? manager.prepare ok
            let cfg = crate::config::ProxyConfig {
                proxy_enabled: true,
                proxy_type: "http".to_string(),
                proxy_host: "127.0.0.1".to_string(),
                proxy_port: port_of(&url),
                proxy_username: None,
                proxy_password: None,
                check_interval: 15,
            };
            mgr.publish(mgr.prepare(Some(cfg)).unwrap());
        }
    }));

    let r1 = forward_non_streaming_with_protocol(&p, "/chat", b"{}", "m", &headers, UpstreamProtocol::ChatCompletions, &settings).await;
    assert!(r1.is_ok());
    assert_eq!(r1.unwrap().status, 500);

    // retry now goes to B
    let _ = forward_non_streaming_with_protocol(&p, "/chat", b"{}", "m", &headers, UpstreamProtocol::ChatCompletions, &settings).await;
    assert_eq!(proxy_b2.targets(), vec!["http://upstream.gateway.test/v1/chat".to_string()]);
}
