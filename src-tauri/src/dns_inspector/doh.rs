use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsRecordAnswer {
    pub name: String,
    #[serde(rename = "type")]
    pub type_code: u16,
    pub type_name: String,
    #[serde(rename = "TTL")]
    pub ttl: u32,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsServerCompareItem {
    pub server_id: String,
    pub server_name: String,
    pub endpoint: String,
    pub latency_ms: u64,
    pub status: i32,
    pub status_text: String,
    pub answers: Vec<DnsRecordAnswer>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsCompareReport {
    pub domain: String,
    pub record_type: String,
    pub queried_at: i64,
    pub is_consistent: bool,
    pub servers: Vec<DnsServerCompareItem>,
}

#[derive(Debug, Deserialize)]
struct RawDohResponse {
    #[serde(rename = "Status")]
    status: i32,
    #[serde(rename = "Answer", default)]
    answer: Vec<RawDohAnswer>,
}

#[derive(Debug, Deserialize)]
struct RawDohAnswer {
    name: String,
    #[serde(rename = "type")]
    type_code: u16,
    #[serde(rename = "TTL")]
    ttl: u32,
    data: String,
}

pub fn type_code_to_name(code: u16) -> &'static str {
    match code {
        1 => "A",
        28 => "AAAA",
        5 => "CNAME",
        15 => "MX",
        16 => "TXT",
        2 => "NS",
        6 => "SOA",
        12 => "PTR",
        _ => "OTHER",
    }
}

pub fn type_name_to_code(name: &str) -> u16 {
    match name.to_uppercase().as_str() {
        "A" => 1,
        "AAAA" => 28,
        "CNAME" => 5,
        "MX" => 15,
        "TXT" => 16,
        "NS" => 2,
        "SOA" => 6,
        "PTR" => 12,
        _ => 1,
    }
}

pub struct DohServerConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub url_pattern: &'static str,
}

pub const DEFAULT_DOH_SERVERS: &[DohServerConfig] = &[
    DohServerConfig {
        id: "alidns",
        name: "AliDNS (阿里)",
        url_pattern: "https://dns.alidns.com/resolve?name={domain}&type={type}",
    },
    DohServerConfig {
        id: "dnspod",
        name: "DNSPod (腾讯)",
        url_pattern: "https://doh.pub/dns-query?name={domain}&type={type}",
    },
    DohServerConfig {
        id: "cloudflare",
        name: "Cloudflare (1.1.1.1)",
        url_pattern: "https://cloudflare-dns.com/dns-query?name={domain}&type={type}",
    },
    DohServerConfig {
        id: "google",
        name: "Google (8.8.8.8)",
        url_pattern: "https://dns.google/resolve?name={domain}&type={type}",
    },
];

pub async fn query_single_doh(
    client: &reqwest::Client,
    server: &DohServerConfig,
    domain: &str,
    record_type: &str,
) -> DnsServerCompareItem {
    let url = server
        .url_pattern
        .replace("{domain}", domain)
        .replace("{type}", record_type);

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/dns-json"));

    let start = Instant::now();
    let resp_res = client.get(&url).headers(headers).send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match resp_res {
        Ok(resp) => {
            if !resp.status().is_success() {
                return DnsServerCompareItem {
                    server_id: server.id.to_string(),
                    server_name: server.name.to_string(),
                    endpoint: url,
                    latency_ms,
                    status: resp.status().as_u16() as i32,
                    status_text: format!("HTTP {}", resp.status()),
                    answers: Vec::new(),
                    error: Some(format!("HTTP error: {}", resp.status())),
                };
            }

            match resp.json::<RawDohResponse>().await {
                Ok(raw) => {
                    let status_text = match raw.status {
                        0 => "NOERROR".to_string(),
                        1 => "FORMERR".to_string(),
                        2 => "SERVFAIL".to_string(),
                        3 => "NXDOMAIN".to_string(),
                        5 => "REFUSED".to_string(),
                        other => format!("STATUS_{}", other),
                    };

                    let answers = raw
                        .answer
                        .into_iter()
                        .map(|a| DnsRecordAnswer {
                            name: a.name,
                            type_code: a.type_code,
                            type_name: type_code_to_name(a.type_code).to_string(),
                            ttl: a.ttl,
                            data: a.data.trim().trim_matches('"').to_string(),
                        })
                        .collect();

                    DnsServerCompareItem {
                        server_id: server.id.to_string(),
                        server_name: server.name.to_string(),
                        endpoint: url,
                        latency_ms,
                        status: raw.status,
                        status_text,
                        answers,
                        error: None,
                    }
                }
                Err(err) => DnsServerCompareItem {
                    server_id: server.id.to_string(),
                    server_name: server.name.to_string(),
                    endpoint: url,
                    latency_ms,
                    status: -1,
                    status_text: "PARSE_ERR".to_string(),
                    answers: Vec::new(),
                    error: Some(err.to_string()),
                },
            }
        }
        Err(err) => DnsServerCompareItem {
            server_id: server.id.to_string(),
            server_name: server.name.to_string(),
            endpoint: url,
            latency_ms,
            status: -1,
            status_text: "CONN_ERR".to_string(),
            answers: Vec::new(),
            error: Some(err.to_string()),
        },
    }
}

pub async fn query_all_doh(domain: &str, record_type: &str) -> DnsCompareReport {
    let client = crate::proxy::apply_current_proxy_to_builder(
        reqwest::Client::builder().timeout(Duration::from_secs(6)),
    )
    .and_then(|b| b.build().map_err(|e| e.to_string()))
    .unwrap_or_else(|_| reqwest::Client::new());

    let mut tasks = Vec::new();
    for s in DEFAULT_DOH_SERVERS {
        let c = client.clone();
        let dom = domain.to_string();
        let rt = record_type.to_string();
        tasks.push(tokio::spawn(async move {
            query_single_doh(&c, s, &dom, &rt).await
        }));
    }

    let mut servers = Vec::new();
    for t in tasks {
        if let Ok(res) = t.await {
            servers.push(res);
        }
    }

    // Check consistency: compare data sets across successful responses
    let successful_servers: Vec<&DnsServerCompareItem> =
        servers.iter().filter(|s| s.error.is_none() && s.status == 0).collect();

    let is_consistent = if successful_servers.len() > 1 {
        let mut first_set: Vec<String> = successful_servers[0]
            .answers
            .iter()
            .map(|a| a.data.clone())
            .collect();
        first_set.sort();

        successful_servers.iter().skip(1).all(|s| {
            let mut other_set: Vec<String> = s.answers.iter().map(|a| a.data.clone()).collect();
            other_set.sort();
            first_set == other_set
        })
    } else {
        true
    };

    DnsCompareReport {
        domain: domain.to_string(),
        record_type: record_type.to_string(),
        queried_at: chrono::Utc::now().timestamp_millis(),
        is_consistent,
        servers,
    }
}
