use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::Deserialize;

use super::types::SslDohPrecheckResult;

#[derive(Deserialize)]
struct DohAnswer {
    #[serde(rename = "type")]
    type_code: u16,
    data: String,
}

#[derive(Deserialize)]
struct DohResponse {
    #[serde(default)]
    #[serde(rename = "Status")]
    status: u32,
    #[serde(default)]
    #[serde(rename = "Answer")]
    answer: Vec<DohAnswer>,
}

pub async fn doh_check_txt(record_name: &str, expected_value: &str) -> SslDohPrecheckResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_default();

    // Query AliDNS DoH and Cloudflare DoH for redundancy
    let endpoints = [
        format!("https://dns.alidns.com/resolve?name={}&type=TXT", record_name),
        format!("https://cloudflare-dns.com/dns-query?name={}&type=TXT", record_name),
    ];

    let mut found_values = Vec::new();
    let mut resolved = false;

    let clean_expected = expected_value.trim().trim_matches('"');

    for url in &endpoints {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/dns-json"));

        if let Ok(resp) = client.get(url).headers(headers).send().await {
            if let Ok(doh_resp) = resp.json::<DohResponse>().await {
                if doh_resp.status == 0 {
                    for ans in doh_resp.answer {
                        // TXT type code is 16
                        if ans.type_code == 16 {
                            let clean_data = ans.data.trim().trim_matches('"').to_string();
                            if !found_values.contains(&clean_data) {
                                found_values.push(clean_data.clone());
                            }
                            if clean_data == clean_expected {
                                resolved = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        if resolved {
            break;
        }
    }

    SslDohPrecheckResult {
        resolved,
        found_values,
    }
}

// Cloudflare DNS Automation
#[derive(Deserialize)]
#[allow(dead_code)]
struct CfZoneItem {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct CfZonesResponse {
    success: bool,
    result: Vec<CfZoneItem>,
}

#[derive(Deserialize)]
struct CfRecordItem {
    id: String,
}

#[derive(Deserialize)]
struct CfRecordResponse {
    success: bool,
    result: CfRecordItem,
}

pub async fn cloudflare_add_txt_record(
    api_token: &str,
    domain: &str,
    record_name: &str,
    record_value: &str,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    // 1. Find zone
    // Root domain usually consists of the last two labels (e.g. example.com)
    let parts: Vec<&str> = domain.trim_start_matches("*.").split('.').collect();
    let zone_name = if parts.len() >= 2 {
        format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        domain.to_string()
    };

    let zones_url = format!("https://api.cloudflare.com/client/v4/zones?name={}", zone_name);
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", api_token.trim()))
            .map_err(|e| e.to_string())?,
    );

    let res = client
        .get(&zones_url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| format!("Failed to reach Cloudflare API: {}", e))?;

    let zones:板 = res
        .json::<CfZonesResponse>()
        .await
        .map_err(|e| format!("Failed to parse Cloudflare zones response: {}", e))?;

    let zone = zones
        .result
        .first()
        .ok_or_else(|| format!("Cloudflare Zone not found for {}", zone_name))?;

    // 2. Add DNS record
    let records_url = format!(
        "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
        zone.id
    );
    let payload = serde_json::json!({
        "type": "TXT",
        "name": record_name,
        "content": record_value,
        "ttl": 120
    });

    let add_res = client
        .post(&records_url)
        .headers(headers.clone())
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to add Cloudflare record: {}", e))?;

    let rec: CfRecordResponse = add_res
        .json()
        .await
        .map_err(|e| format!("Failed to parse add record response: {}", e))?;

    if !rec.success {
        return Err("Cloudflare rejected DNS record creation".to_string());
    }

    Ok(format!("{}:{}", zone.id, rec.result.id))
}

pub async fn cloudflare_delete_txt_record(api_token: &str, handle: &str) -> Result<(), String> {
    let parts: Vec<&str> = handle.split(':').collect();
    if parts.len() != 2 {
        return Ok(());
    }
    let zone_id = parts[0];
    let record_id = parts[1];

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let del_url = format!(
        "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
        zone_id, record_id
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", api_token.trim()))
            .map_err(|e| e.to_string())?,
    );

    let _ = client.delete(&del_url).headers(headers).send().await;
    Ok(())
}
type 板 = CfZonesResponse;
