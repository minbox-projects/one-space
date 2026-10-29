use crate::config::ProxyConfig;
use crate::proxy::*;
use std::sync::MutexGuard;

fn cfg(enabled: bool, ty: &str, host: &str, port: u16) -> ProxyConfig {
    ProxyConfig {
        proxy_enabled: enabled,
        proxy_type: ty.to_string(),
        proxy_host: host.to_string(),
        proxy_port: port,
        proxy_username: None,
        proxy_password: None,
        check_interval: 15,
    }
}

fn cfg_auth(enabled: bool, ty: &str, host: &str, port: u16, user: &str, pass: &str) -> ProxyConfig {
    let mut c = cfg(enabled, ty, host, port);
    c.proxy_username = Some(user.to_string());
    c.proxy_password = Some(pass.to_string());
    c
}

fn port_of(url: &str) -> u16 {
    url.trim_start_matches("http://")
        .trim_start_matches("socks5://")
        .split(':')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn proxy_switch_a_then_b_routes_next_request_to_b() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new("A").await;
    let proxy_b = crate::proxy::test_support::HttpProxyRecorder::new("B").await;

    let mgr = ProxyManager::new();
    let a_cfg = cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()));
    let ga = mgr.publish(mgr.prepare(Some(a_cfg)).unwrap());

    // request through A
    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/one").send().await;

    assert_eq!(proxy_a.targets(), vec!["http://upstream.proxy.test/one".to_string()]);
    assert!(proxy_b.targets().is_empty());

    let b_cfg = cfg(true, "http", "127.0.0.1", port_of(&proxy_b.url()));
    let gb = mgr.publish(mgr.prepare(Some(b_cfg)).unwrap());
    assert!(gb > ga);

    // request through B using current client
    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/two").send().await;

    assert_eq!(proxy_a.targets(), vec!["http://upstream.proxy.test/one".to_string()]);
    assert_eq!(proxy_b.targets(), vec!["http://upstream.proxy.test/two".to_string()]);
    assert_eq!(proxy_a.hits(), 1);
}

#[tokio::test]
async fn proxy_switch_equivalent_and_interval_saves_keep_generation_and_client() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let proxy = crate::proxy::test_support::HttpProxyRecorder::new("P").await;
    let mgr = ProxyManager::new();
    let base = cfg(true, "http", "127.0.0.1", port_of(&proxy.url()));
    let g0 = mgr.publish(mgr.prepare(Some(base.clone())).unwrap());

    // equivalent save
    let g1 = mgr.publish(mgr.prepare(Some(base.clone())).unwrap());
    assert_eq!(g0, g1);

    // check-interval-only change
    let mut interval = base.clone();
    interval.check_interval = 999;
    let g2 = mgr.publish(mgr.prepare(Some(interval)).unwrap());
    assert_eq!(g0, g2);

    // observed request still goes to same proxy
    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/one").send().await;
    assert_eq!(proxy.targets(), vec!["http://upstream.proxy.test/one".to_string()]);

    // changed protocol -> greater
    let mut proto = base.clone();
    proto.proxy_type = "socks5".to_string();
    let g3 = mgr.publish(mgr.prepare(Some(proto)).unwrap());
    assert!(g3 > g2);

    // changed username/password -> greater
    let mgr2 = ProxyManager::new();
    let g0b = mgr2.publish(mgr2.prepare(Some(base.clone())).unwrap());
    let auth = cfg_auth(true, "http", "127.0.0.1", port_of(&proxy.url()), "u", "p");
    let g4 = mgr2.publish(mgr2.prepare(Some(auth)).unwrap());
    assert!(g4 > g0b);

    // changed host/port -> greater
    let mgr3 = ProxyManager::new();
    let g0c = mgr3.publish(mgr3.prepare(Some(base.clone())).unwrap());
    let mut hp = base.clone();
    hp.proxy_port = port_of(&proxy.url()) + 1;
    let g5 = mgr3.publish(mgr3.prepare(Some(hp)).unwrap());
    assert!(g5 > g0c);
}

#[tokio::test]
async fn proxy_switch_auth_protocol_change_negotiates_new_endpoint() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let socks = crate::proxy::test_support::FakeSocks5::new().await;
    let proxy_http = crate::proxy::test_support::HttpProxyRecorder::new("P").await;
    let mgr = ProxyManager::new();

    // start http
    let http_cfg = cfg(true, "http", "127.0.0.1", port_of(&proxy_http.url()));
    mgr.publish(mgr.prepare(Some(http_cfg)).unwrap());

    // switch to socks5
    let socks_cfg = cfg(true, "socks5", "127.0.0.1", port_of(&socks.url()));
    mgr.publish(mgr.prepare(Some(socks_cfg)).unwrap());

    let socks_port = port_of(&socks.url());
    let proxy = reqwest::Proxy::all(format!("socks5h://127.0.0.1:{}", socks_port)).unwrap();
    let client = reqwest::Client::builder().proxy(proxy).build().unwrap();
    let _ = client.get("http://example.com/path").send().await;
    assert_eq!(socks.target(), Some("example.com:80".to_string()));

    // switch to http proxy
    let http_cfg2 = cfg(true, "http", "127.0.0.1", port_of(&proxy_http.url()));
    mgr.publish(mgr.prepare(Some(http_cfg2)).unwrap());

    let client = mgr.get_client().unwrap();
    let _ = client.get("http://example.com/path").send().await;
    assert_eq!(proxy_http.targets(), vec!["http://example.com/path".to_string()]);

    // https target -> CONNECT
    let _ = client.get("https://example.com/secure").send().await;
    assert!(proxy_http.targets().iter().any(|t| t == "example.com:443"));
}

#[tokio::test]
async fn proxy_switch_direct_mode_ignores_inherited_proxy_env_and_loopback() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new("A").await;
    let upstream = crate::proxy::test_support::DirectUpstream::new("UP").await;

    let a_url = proxy_a.url();
    let keys = ["HTTP_PROXY","HTTPS_PROXY","ALL_PROXY","http_proxy","https_proxy","all_proxy"];
    let saved: Vec<(String, Option<String>)> = keys.iter().map(|k| (k.to_string(), std::env::var(k).ok())).collect();
    struct EnvGuard { saved: Vec<(String, Option<String>)> }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in &self.saved {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }
    let _env_guard = EnvGuard { saved };
    for k in keys {
        std::env::set_var(k, &a_url);
    }

    let mgr = ProxyManager::new();
    let disabled = mgr.prepare(Some(cfg(false, "http", "127.0.0.1", port_of(&proxy_a.url())))).unwrap();
    mgr.publish(disabled);
    let client = mgr.get_client().unwrap();
    let _ = client.get(upstream.url()).send().await;
    assert!(upstream.hits() >= 1);
    assert_eq!(proxy_a.hits(), 0);

    // enabled proxy + loopback target still direct
    let enabled = mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url())))).unwrap();
    mgr.publish(enabled);
    let client = mgr.get_client().unwrap();
    let _ = client.get(upstream.url()).send().await;
    assert!(upstream.hits() >= 2);
    assert_eq!(proxy_a.hits(), 0);
}

#[tokio::test]
async fn proxy_switch_unreachable_proxy_errors_without_fallback_and_hides_credentials() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = listener.local_addr().unwrap().port();
    drop(listener);

    let mgr = ProxyManager::new();
    let c = cfg_auth(true, "http", "127.0.0.1", closed_port, crate::proxy::test_support::SAFE_FIXTURE_USER, crate::proxy::test_support::SAFE_FIXTURE_PASS);
    let prepared = mgr.prepare(Some(c)).unwrap();
    mgr.publish(prepared);
    let client = mgr.get_client().unwrap();
    let res = client.get("http://upstream.proxy.test/x").send().await;
    assert!(res.is_err());
    let err = format!("{}", res.unwrap_err());
    let debug = format!("{:?}", mgr.current_snapshot());
    assert!(!err.contains(crate::proxy::test_support::SAFE_FIXTURE_USER));
    assert!(!err.contains(crate::proxy::test_support::SAFE_FIXTURE_PASS));
    assert!(!debug.contains(crate::proxy::test_support::SAFE_FIXTURE_USER));
    assert!(!debug.contains(crate::proxy::test_support::SAFE_FIXTURE_PASS));
}

#[tokio::test]
async fn proxy_switch_draft_test_does_not_publish() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new("A").await;
    let proxy_b = crate::proxy::test_support::HttpProxyRecorder::new("B").await;
    let mgr = ProxyManager::new();
    let a_cfg = cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()));
    let g0 = mgr.publish(mgr.prepare(Some(a_cfg)).unwrap());

    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/one").send().await;
    assert_eq!(proxy_a.targets().len(), 1);

    let b_cfg = cfg(true, "http", "127.0.0.1", port_of(&proxy_b.url()));
    let _ = ProxyManager::test_draft(Some(b_cfg)).await;

    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/one").send().await;
    assert_eq!(proxy_a.targets().len(), 2);
    assert_eq!(proxy_b.targets().len(), 1);
    assert_eq!(mgr.generation(), g0);
}

#[tokio::test]
async fn proxy_switch_policy_retention() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    // timeout preserved
    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new_delayed("A", 500).await;
    let mgr = ProxyManager::new();
    let a_cfg = cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()));
    mgr.publish(mgr.prepare(Some(a_cfg)).unwrap());
    let client = mgr.apply_builder(reqwest::Client::builder().timeout(std::time::Duration::from_millis(200)).redirect(reqwest::redirect::Policy::none())).unwrap().build().unwrap();
    let res = client.get("http://example.com/timeout").send().await;
    assert!(res.is_err());
    let err = format!("{}", res.unwrap_err()).to_lowercase();
    assert!(err.contains("timed out") || err.contains("timeout"));
    assert_eq!(proxy_a.targets(), vec!["http://example.com/timeout".to_string()]);

    // redirect preserved
    let proxy_b = crate::proxy::test_support::HttpProxyRecorder::new("B").await;
    let proxy_a2 = crate::proxy::test_support::HttpProxyRecorder::new_redirect(&format!("{}/x", proxy_b.url())).await;
    let mgr2 = ProxyManager::new();
    let a_cfg2 = cfg(true, "http", "127.0.0.1", port_of(&proxy_a2.url()));
    mgr2.publish(mgr2.prepare(Some(a_cfg2)).unwrap());

    // redirect-none: only A observed
    let client_none = mgr2.apply_builder(reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())).unwrap().build().unwrap();
    let _ = client_none.get("http://example.com/redirect").send().await;
    assert_eq!(proxy_a2.targets(), vec!["http://example.com/redirect".to_string()]);
    assert_eq!(proxy_b.targets().len(), 0);

    // default redirect: B observed
    let client_default = mgr2.apply_builder(reqwest::Client::builder()).unwrap().build().unwrap();
    let _ = client_default.get("http://example.com/redirect").send().await;
    assert_eq!(proxy_b.targets(), vec!["/x".to_string()]);

    // direct-mode client: loopback still direct
    let direct_upstream = crate::proxy::test_support::DirectUpstream::new("UP").await;
    let mgr3 = ProxyManager::new();
    let disabled = mgr3.prepare(Some(cfg(false, "http", "127.0.0.1", port_of(&proxy_a2.url())))).unwrap();
    mgr3.publish(disabled);
    let direct_client = mgr3.apply_builder(reqwest::Client::builder()).unwrap().build().unwrap();
    let _ = direct_client.get(direct_upstream.url()).send().await;
    assert!(direct_upstream.hits() >= 1);
}

// Consumer tests
#[tokio::test]
async fn ai_news_proxy_switch_sequential_sources() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let pa = crate::proxy::test_support::HttpProxyRecorder::new(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>T</title><item><title>one</title><link>http://x</link></item></channel></rss>",
    ).await;
    let pb = crate::proxy::test_support::HttpProxyRecorder::new(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>T</title><item><title>two</title><link>http://y</link></item></channel></rss>",
    ).await;

    // on-hit callback on A publishes B (only once)
    let published = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // We'll instead use shared mgr via global; need callback captures mgr + b cfg + published flag.
    // But HttpProxyRecorder::set_on_hit takes Arc<dyn Fn>. We'll set after creating mgr? mgr global.
    let mgr = crate::proxy::PROXY_MANAGER.get_or_init(|| std::sync::Arc::new(crate::proxy::ProxyManager::new()));
    let a_port: u16 = pa.url().split(':').last().unwrap().parse().unwrap();
    let b_port: u16 = pb.url().split(':').last().unwrap().parse().unwrap();
    let a_cfg = cfg(true, "http", "127.0.0.1", a_port);
    mgr.publish(mgr.prepare(Some(a_cfg)).unwrap());

    let published2 = published.clone();
    let mgr2 = mgr.clone();
    let b_cfg_port = b_port;
    let cb: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
        if !published2.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let b_cfg = cfg(true, "http", "127.0.0.1", b_cfg_port);
            mgr2.publish(mgr2.prepare(Some(b_cfg)).unwrap());
        }
    });
    pa.set_on_hit(cb);

    let src_a = crate::config::AiNewsRssSource {
        id: "a".into(),
        name: "A".into(),
        url: "http://rss-one.example.test/feed".into(),
        enabled: true,
    };
    let src_b = crate::config::AiNewsRssSource {
        id: "b".into(),
        name: "B".into(),
        url: "http://rss-two.example.test/feed".into(),
        enabled: true,
    };

    // Make proxy targets: recorder returns RSS body for any request; but target URL is recorded as request URL.
    // However production requests rss-one.example.test which won't resolve via proxy? Proxy records target from request line.
    // The request URL host is rss-one.example.test, proxy records "http://rss-one.example.test/feed".
    let _ = crate::ai_news::fetch_sources_for_sync(&[src_a, src_b], &[], 5).await;

    assert_eq!(pa.targets(), vec!["http://rss-one.example.test/feed".to_string()]);
    assert_eq!(pb.targets(), vec!["http://rss-two.example.test/feed".to_string()]);
}

#[tokio::test]
async fn proxy_switch_save_persists_before_publish_and_failure_keeps_old_state() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let pa = crate::proxy::test_support::HttpProxyRecorder::new("A").await;
    let pb = crate::proxy::test_support::HttpProxyRecorder::new("B").await;

    let mgr = crate::proxy::PROXY_MANAGER
        .get_or_init(|| std::sync::Arc::new(crate::proxy::ProxyManager::new()));
    let a_cfg = cfg(true, "http", "127.0.0.1", port_of(&pa.url()));
    mgr.publish(mgr.prepare(Some(a_cfg)).unwrap());
    let gen_a = mgr.generation();

    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/x").send().await;
    assert_eq!(pa.targets().len(), 1);

    let b_cfg = cfg(true, "http", "127.0.0.1", port_of(&pb.url()));
    let res = crate::config::save_proxy_atomically_for_test(|| Err("fail".to_string()), Some(b_cfg.clone()));
    assert!(res.is_err());
    assert_eq!(mgr.generation(), gen_a);

    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/y").send().await;
    assert_eq!(pa.targets().len(), 2);
    assert_eq!(pb.targets().len(), 0);

    // invalid B (empty host) never invokes persistence
    let mut invalid = b_cfg.clone();
    invalid.proxy_host = String::new();
    let mut called = false;
    let res = crate::config::save_proxy_atomically_for_test(
        || {
            called = true;
            Ok(())
        },
        Some(invalid),
    );
    assert!(res.is_err());
    assert!(!called);

    // successful persistence publishes B
    let res = crate::config::save_proxy_atomically_for_test(|| Ok(()), Some(b_cfg));
    assert!(res.is_ok());

    let client = mgr.get_client().unwrap();
    let _ = client.get("http://upstream.proxy.test/z").send().await;
    assert_eq!(pb.targets().len(), 1);
}

#[tokio::test]
async fn gmail_bridge_proxy_switch_uses_proxy_http_request() {
    let _g = crate::proxy::test_support::SERIAL_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let pa = crate::proxy::test_support::HttpProxyRecorder::new("ok").await;
    let pb = crate::proxy::test_support::HttpProxyRecorder::new("ok").await;
    let mgr = crate::proxy::PROXY_MANAGER
        .get_or_init(|| std::sync::Arc::new(crate::proxy::ProxyManager::new()));
    let a_cfg = cfg(true, "http", "127.0.0.1", port_of(&pa.url()));
    mgr.publish(mgr.prepare(Some(a_cfg)).unwrap());

    let mut headers = std::collections::HashMap::new();
    headers.insert("X-Test".to_string(), "value".to_string());
    let res = crate::app_runtime::proxy_http_request_for_test(
        "http://gmail.example.test/gmail/v1/users/me/profile".to_string(),
        "GET".to_string(),
        Some(headers.clone()),
        None,
    )
    .await;
    assert!(res.is_ok());
    assert_eq!(pa.targets(), vec!["http://gmail.example.test/gmail/v1/users/me/profile".to_string()]);
    assert!(pa.requests()[0].to_lowercase().contains("x-test: value"));
    assert!(pb.targets().is_empty());

    let b_cfg = cfg(true, "http", "127.0.0.1", port_of(&pb.url()));
    mgr.publish(mgr.prepare(Some(b_cfg)).unwrap());
    let res = crate::app_runtime::proxy_http_request_for_test(
        "http://gmail.example.test/gmail/v1/users/me/profile".to_string(),
        "GET".to_string(),
        Some(headers),
        None,
    )
    .await;
    assert!(res.is_ok());
    assert_eq!(pb.targets(), vec!["http://gmail.example.test/gmail/v1/users/me/profile".to_string()]);

    // 500 error contract
    let pe = crate::proxy::test_support::HttpProxyRecorder::new_with_status("boom", 500).await;
    let mgr2 = crate::proxy::ProxyManager::new();
    let e_cfg = cfg(true, "http", "127.0.0.1", port_of(&pe.url()));
    mgr2.publish(mgr2.prepare(Some(e_cfg)).unwrap());
    // use global? function uses global manager. Set global to pe manager? Can't replace. Instead call via local? proxy_http_request uses global. 
    // We'll publish pe into global by reusing mgr
    let _ = mgr.publish(mgr.prepare(Some(cfg(true,"http","127.0.0.1",port_of(&pe.url())))).unwrap());
    let err = crate::app_runtime::proxy_http_request_for_test(
        "http://gmail.example.test/err".to_string(),
        "GET".to_string(),
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err, "HTTP 500: boom");
}

#[tokio::test]
async fn proxy_switch_current_client_observes_published_snapshot() {
    let _g = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new("A").await;
    let proxy_b = crate::proxy::test_support::HttpProxyRecorder::new("B").await;

    let mgr = ProxyManager::new();
    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()))))
            .unwrap(),
    );
    let a_client = crate::proxy::current_client().unwrap();
    let _ = a_client.get("http://consumer.proxy.test/one").send().await;
    assert_eq!(
        proxy_a.targets(),
        vec!["http://consumer.proxy.test/one".to_string()]
    );

    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_b.url()))))
            .unwrap(),
    );
    let b_client = crate::proxy::current_client().unwrap();
    let _ = b_client.get("http://consumer.proxy.test/two").send().await;
    assert_eq!(
        proxy_b.targets(),
        vec!["http://consumer.proxy.test/two".to_string()]
    );
    assert_eq!(proxy_a.hits(), 1);
}

#[test]
fn proxy_switch_thread_without_publish_observes_direct_snapshot() {
    let _g = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());

    let mgr = ProxyManager::new();
    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", 4321)))
            .unwrap(),
    );
    assert!(super::current_proxy_snapshot().is_enabled());

    let observed = std::thread::spawn(|| {
        let snapshot = super::current_proxy_snapshot();
        (snapshot.is_enabled(), snapshot.generation)
    })
    .join()
    .unwrap();

    assert_eq!(observed, (false, 0));
}

#[test]
fn proxy_switch_serial_guard_clears_stale_override() {
    crate::proxy::set_test_current_snapshot(std::sync::Arc::new(ProxySnapshot {
        generation: 7,
        config: Some(cfg(true, "http", "127.0.0.1", 4321)),
    }));
    assert!(super::current_proxy_snapshot().is_enabled());

    {
        let _g = crate::proxy::test_support::SERIAL_GUARD
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(!super::current_proxy_snapshot().is_enabled());
    }

    assert!(!super::current_proxy_snapshot().is_enabled());
}

#[tokio::test]
async fn ai_env_model_fetch_proxy_switch() {
    let _g = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new(
        r#"{"data":[{"id":"model-one"}]}"#,
    )
    .await;
    let proxy_b = crate::proxy::test_support::HttpProxyRecorder::new(
        r#"{"data":[{"id":"model-one"}]}"#,
    )
    .await;
    let direct_upstream = crate::proxy::test_support::DirectUpstream::new("direct").await;

    let mgr = ProxyManager::new();
    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()))))
            .unwrap(),
    );

    let provider = serde_json::json!({
        "tool": "x",
        "base_url": "http://ai-env-model.example.test/v1",
        "api_key": "SAFE_FIXTURE_key",
    });
    let expected = "http://ai-env-model.example.test/v1/models".to_string();

    let first = crate::ai_env::service_provider_fetch_models(provider.clone()).await;
    assert!(first.is_ok());
    assert_eq!(proxy_a.targets(), vec![expected.clone()]);
    assert!(proxy_b.targets().is_empty());

    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_b.url()))))
            .unwrap(),
    );
    let second = crate::ai_env::service_provider_fetch_models(provider).await;
    assert!(second.is_ok());
    assert_eq!(proxy_b.targets(), vec![expected]);
    assert_eq!(proxy_a.hits(), 1);
    assert_eq!(direct_upstream.hits(), 0);
}

#[tokio::test]
async fn http_client_proxy_switch_send_request() {
    let _g = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new("ok").await;
    let proxy_b = crate::proxy::test_support::HttpProxyRecorder::new("ok").await;

    let mgr = ProxyManager::new();
    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()))))
            .unwrap(),
    );

    let request = || crate::http_client::HttpRequestPayload {
        method: "GET".to_string(),
        url: "http://http-tool.example.test/echo".to_string(),
        headers: std::collections::HashMap::new(),
        body: None,
        timeout_ms: Some(5000),
    };
    let expected = "http://http-tool.example.test/echo".to_string();

    let first = crate::http_client::send_http_request(request()).await;
    assert!(first.is_ok());
    assert_eq!(proxy_a.targets(), vec![expected.clone()]);
    assert!(proxy_b.targets().is_empty());

    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_b.url()))))
            .unwrap(),
    );
    let second = crate::http_client::send_http_request(request()).await;
    assert!(second.is_ok());
    assert_eq!(proxy_b.targets(), vec![expected]);
    assert_eq!(proxy_a.hits(), 1);
}

#[tokio::test]
async fn short_link_proxy_switch_preserves_timeout_and_redirect_policy() {
    let _g = crate::proxy::test_support::SERIAL_GUARD
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let temp_home = std::env::temp_dir().join(format!(
        "onespace-proxy-short-link-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).unwrap();
    let home_guard = crate::config::test_home::TestHomeGuard::set(&temp_home);
    let app_dir = crate::config::get_app_dir().unwrap();
    std::fs::create_dir_all(&app_dir).unwrap();
    let config = format!(
        r#"{{"storage_type":"local","local_storage_path":{}}}"#,
        serde_json::to_string(&temp_home.join("data")).unwrap()
    );
    std::fs::write(app_dir.join("config.json"), config).unwrap();
    crate::secrets::save_tinyurl_api_token("SAFE_FIXTURE_tinyurl_token".to_string()).unwrap();

    let proxy_a = crate::proxy::test_support::HttpProxyRecorder::new(
        r#"{"data":{"tiny_url":"https://tinyurl.com/proxy-switch-a"}}"#,
    )
    .await;
    let proxy_b = crate::proxy::test_support::HttpProxyRecorder::new(
        r#"{"data":{"tiny_url":"https://tinyurl.com/proxy-switch-b"}}"#,
    )
    .await;

    let mgr = ProxyManager::new();
    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()))))
            .unwrap(),
    );
    let endpoint = "http://tinyurl-api.example.test/create";
    let expected = endpoint.to_string();

    let first =
        crate::short_link::short_link_create_with_endpoint("https://example.com".into(), endpoint)
            .await;
    assert!(first.is_ok());
    assert_eq!(proxy_a.targets(), vec![expected.clone()]);
    assert!(proxy_b.targets().is_empty());

    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_b.url()))))
            .unwrap(),
    );
    let second =
        crate::short_link::short_link_create_with_endpoint("https://example.com".into(), endpoint)
            .await;
    assert!(second.is_ok());
    assert_eq!(proxy_b.targets(), vec![expected.clone()]);
    assert_eq!(proxy_a.hits(), 1);

    proxy_a.set_redirect(Some(format!("{}/redirected", proxy_b.url())));
    mgr.publish(
        mgr.prepare(Some(cfg(true, "http", "127.0.0.1", port_of(&proxy_a.url()))))
            .unwrap(),
    );
    let redirected =
        crate::short_link::short_link_create_with_endpoint("https://example.com".into(), endpoint)
            .await;
    assert!(redirected.is_err());
    assert_eq!(proxy_a.targets(), vec![expected.clone(), expected]);
    assert_eq!(proxy_a.hits(), 2);
    assert_eq!(proxy_b.hits(), 1);

    drop(home_guard);
    let _ = std::fs::remove_dir_all(&temp_home);
}
