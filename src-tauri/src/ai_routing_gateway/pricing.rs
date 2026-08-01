use rusqlite::{params, Connection, OptionalExtension};

use super::{
    error::{GatewayError, GatewayErrorCategory},
    types::PriceSnapshot,
};

const DECIMAL_SCALE: u128 = 1_000_000_000;
const TOKENS_PER_MILLION: u128 = 1_000_000;
const OFFICIAL_PRICE_SNAPSHOT_VERSION: &str = "openai-api-pricing-2026-08-01-r1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PriceInput<'a> {
    pub(crate) public_model_id: &'a str,
    pub(crate) account_id: Option<&'a str>,
    pub(crate) effective_at: &'a str,
    pub(crate) input_per_million_usd: Option<&'a str>,
    pub(crate) output_per_million_usd: Option<&'a str>,
    pub(crate) cache_read_per_million_usd: Option<&'a str>,
    pub(crate) cache_write_per_million_usd: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TokenUsage {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) cache_read_tokens: Option<u64>,
    pub(crate) cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CostEstimate {
    Calculable(String),
    NotCalculable,
}

pub(crate) fn save_price(
    connection: &Connection,
    input: PriceInput<'_>,
) -> Result<String, GatewayError> {
    if input.public_model_id.trim().is_empty() || input.effective_at.trim().is_empty() {
        return Err(invalid(input.public_model_id));
    }
    if let Some(account_id) = input.account_id {
        let account_type: Option<String> = connection
            .query_row(
                "SELECT account_type FROM ai_gateway_accounts WHERE id = ?1",
                [account_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| storage(input.public_model_id))?;
        if account_type.as_deref() != Some("api_key") {
            return Err(invalid(input.public_model_id));
        }
    }
    for value in [
        input.input_per_million_usd,
        input.output_per_million_usd,
        input.cache_read_per_million_usd,
        input.cache_write_per_million_usd,
    ]
    .into_iter()
    .flatten()
    {
        parse_decimal(value).ok_or_else(|| invalid(input.public_model_id))?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let source = if input.account_id.is_some() {
        "account_override"
    } else {
        "official"
    };
    connection
        .execute(
            "INSERT INTO ai_gateway_model_prices (id, public_model_id, account_id, input_per_million_usd, output_per_million_usd, cache_read_per_million_usd, cache_write_per_million_usd, source, effective_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, input.public_model_id, input.account_id, input.input_per_million_usd, input.output_per_million_usd, input.cache_read_per_million_usd, input.cache_write_per_million_usd, source, input.effective_at],
        )
        .map_err(|_| storage(input.public_model_id))?;
    Ok(id)
}

pub(crate) fn snapshot_price(
    connection: &Connection,
    public_model_id: &str,
    account_id: Option<&str>,
    at: &str,
) -> Result<Option<PriceSnapshot>, GatewayError> {
    if let Some(account_id) = account_id {
        if let Some(snapshot) = query_snapshot(
            connection,
            public_model_id,
            Some(account_id),
            at,
            "account_override",
        )? {
            return Ok(Some(snapshot));
        }
    }
    query_snapshot(connection, public_model_id, None, at, "official")
}

pub(crate) fn estimate_cost(snapshot: Option<&PriceSnapshot>, usage: TokenUsage) -> CostEstimate {
    let Some(snapshot) = snapshot else {
        return CostEstimate::NotCalculable;
    };
    let (Some(input_tokens), Some(output_tokens)) = (usage.input_tokens, usage.output_tokens)
    else {
        return CostEstimate::NotCalculable;
    };
    let mut scaled_total = 0u128;
    let components = [
        (
            Some(input_tokens),
            snapshot.input_per_million_usd.as_deref(),
        ),
        (
            Some(output_tokens),
            snapshot.output_per_million_usd.as_deref(),
        ),
        (
            usage.cache_read_tokens,
            snapshot.cache_read_per_million_usd.as_deref(),
        ),
        (
            usage.cache_write_tokens,
            snapshot.cache_write_per_million_usd.as_deref(),
        ),
    ];
    for (tokens, price) in components {
        let (Some(tokens), Some(price)) = (tokens, price) else {
            if tokens.is_none() && price.is_none() {
                continue;
            }
            return CostEstimate::NotCalculable;
        };
        let Some(price) = parse_decimal(price) else {
            return CostEstimate::NotCalculable;
        };
        let Some(component) = (tokens as u128).checked_mul(price) else {
            return CostEstimate::NotCalculable;
        };
        let Some(total) = scaled_total.checked_add(component / TOKENS_PER_MILLION) else {
            return CostEstimate::NotCalculable;
        };
        scaled_total = total;
    }
    CostEstimate::Calculable(format_decimal(scaled_total))
}

pub(crate) fn official_price_snapshot_version() -> &'static str {
    OFFICIAL_PRICE_SNAPSHOT_VERSION
}

fn query_snapshot(
    connection: &Connection,
    public_model_id: &str,
    account_id: Option<&str>,
    at: &str,
    source: &str,
) -> Result<Option<PriceSnapshot>, GatewayError> {
    connection
        .query_row(
            "SELECT public_model_id, account_id, source, effective_at, input_per_million_usd, output_per_million_usd, cache_read_per_million_usd, cache_write_per_million_usd FROM ai_gateway_model_prices WHERE public_model_id = ?1 AND ((?2 IS NULL AND account_id IS NULL) OR account_id = ?2) AND source = ?3 AND effective_at <= ?4 ORDER BY effective_at DESC, id DESC LIMIT 1",
            params![public_model_id, account_id, source, at],
            |row| {
                Ok(PriceSnapshot {
                    public_model_id: row.get(0)?,
                    account_id: row.get(1)?,
                    source: row.get(2)?,
                    effective_at: row.get(3)?,
                    input_per_million_usd: row.get(4)?,
                    output_per_million_usd: row.get(5)?,
                    cache_read_per_million_usd: row.get(6)?,
                    cache_write_per_million_usd: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|_| storage(public_model_id))
}

fn parse_decimal(value: &str) -> Option<u128> {
    if value.is_empty() || value.starts_with('-') {
        return None;
    }
    let mut parts = value.split('.');
    let whole = parts.next()?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 9
    {
        return None;
    }
    let whole = whole.parse::<u128>().ok()?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u128>().ok()? * 10u128.pow((9 - fraction.len()) as u32)
    };
    whole.checked_mul(DECIMAL_SCALE)?.checked_add(fraction)
}

fn format_decimal(scaled: u128) -> String {
    let whole = scaled / DECIMAL_SCALE;
    let fraction = scaled % DECIMAL_SCALE;
    if fraction == 0 {
        return whole.to_string();
    }
    let fraction = format!("{fraction:09}").trim_end_matches('0').to_owned();
    format!("{whole}.{fraction}")
}

fn invalid(entity_id: &str) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::InvalidInput, Some(entity_id))
}

fn storage(entity_id: &str) -> GatewayError {
    GatewayError::new(GatewayErrorCategory::StorageUnavailable, Some(entity_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_sqlite;

    fn database() -> (std::path::PathBuf, Connection) {
        let path =
            std::env::temp_dir().join(format!("onespace-pricing-{}.sqlite3", uuid::Uuid::new_v4()));
        let connection = shared_sqlite::open_at(&path).unwrap();
        connection
            .execute(
                "INSERT INTO ai_gateway_models (id, display_name) VALUES ('gpt-test', 'GPT Test')",
                [],
            )
            .unwrap();
        connection
            .execute("INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('account-1', 'api_key', 'Account', 'default')", [])
            .unwrap();
        connection
            .execute("INSERT INTO ai_gateway_accounts (id, account_type, name, group_id) VALUES ('oauth-account', 'oauth', 'OAuth Account', 'default')", [])
            .unwrap();
        (path, connection)
    }

    fn cleanup(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    #[test]
    fn account_override_wins_and_snapshot_does_not_change_retroactively() {
        let (path, connection) = database();
        save_price(
            &connection,
            PriceInput {
                public_model_id: "gpt-test",
                account_id: None,
                effective_at: "2026-01-01T00:00:00Z",
                input_per_million_usd: Some("1.25"),
                output_per_million_usd: Some("5"),
                cache_read_per_million_usd: Some("0.25"),
                cache_write_per_million_usd: None,
            },
        )
        .unwrap();
        let official = snapshot_price(
            &connection,
            "gpt-test",
            Some("account-1"),
            "2026-02-01T00:00:00Z",
        )
        .unwrap()
        .unwrap();
        assert_eq!(official.source, "official");
        save_price(
            &connection,
            PriceInput {
                public_model_id: "gpt-test",
                account_id: Some("account-1"),
                effective_at: "2026-01-15T00:00:00Z",
                input_per_million_usd: Some("2"),
                output_per_million_usd: Some("6"),
                cache_read_per_million_usd: Some("0.5"),
                cache_write_per_million_usd: Some("3"),
            },
        )
        .unwrap();
        let overridden = snapshot_price(
            &connection,
            "gpt-test",
            Some("account-1"),
            "2026-02-01T00:00:00Z",
        )
        .unwrap()
        .unwrap();
        assert_eq!(overridden.source, "account_override");
        assert_eq!(official.input_per_million_usd.as_deref(), Some("1.25"));
        assert_eq!(overridden.input_per_million_usd.as_deref(), Some("2"));
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn missing_price_or_usage_is_not_zero_and_decimal_cost_is_deterministic() {
        let snapshot = PriceSnapshot {
            public_model_id: "gpt-test".into(),
            account_id: None,
            source: "official".into(),
            effective_at: "2026-01-01T00:00:00Z".into(),
            input_per_million_usd: Some("2".into()),
            output_per_million_usd: Some("10".into()),
            cache_read_per_million_usd: Some("1".into()),
            cache_write_per_million_usd: None,
        };
        assert_eq!(
            estimate_cost(
                Some(&snapshot),
                TokenUsage {
                    input_tokens: Some(1_000_000),
                    output_tokens: Some(500_000),
                    cache_read_tokens: Some(250_000),
                    cache_write_tokens: None
                }
            ),
            CostEstimate::Calculable("7.25".into())
        );
        assert_eq!(
            estimate_cost(
                None,
                TokenUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None
                }
            ),
            CostEstimate::NotCalculable
        );
        assert_eq!(
            estimate_cost(
                Some(&snapshot),
                TokenUsage {
                    input_tokens: None,
                    output_tokens: Some(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None
                }
            ),
            CostEstimate::NotCalculable
        );
        assert_eq!(
            estimate_cost(
                Some(&snapshot),
                TokenUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(1),
                    cache_read_tokens: None,
                    cache_write_tokens: Some(1)
                }
            ),
            CostEstimate::NotCalculable
        );
        assert_eq!(
            estimate_cost(
                Some(&snapshot),
                TokenUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None
                }
            ),
            CostEstimate::NotCalculable
        );
    }

    #[test]
    fn account_overrides_are_allowed_only_for_api_key_accounts() {
        let (path, connection) = database();
        let result = save_price(
            &connection,
            PriceInput {
                public_model_id: "gpt-test",
                account_id: Some("oauth-account"),
                effective_at: "2026-01-01T00:00:00Z",
                input_per_million_usd: Some("1"),
                output_per_million_usd: Some("2"),
                cache_read_per_million_usd: None,
                cache_write_per_million_usd: None,
            },
        );
        assert_eq!(
            result.unwrap_err().category(),
            GatewayErrorCategory::InvalidInput
        );
        drop(connection);
        cleanup(&path);
    }

    #[test]
    fn new_database_contains_one_idempotent_versioned_official_snapshot() {
        let (path, connection) = database();
        let first_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_model_prices WHERE source = 'official'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(first_count, 3);
        let model_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_models WHERE id LIKE 'gpt-5.6-%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(model_count, 3);
        let versioned_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_model_prices WHERE id LIKE ?1",
                [format!("official-{OFFICIAL_PRICE_SNAPSHOT_VERSION}-%")],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(versioned_count, 3);
        drop(connection);

        let reopened = shared_sqlite::open_at(&path).unwrap();
        let second_count: i64 = reopened
            .query_row(
                "SELECT COUNT(*) FROM ai_gateway_model_prices WHERE source = 'official'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(second_count, first_count);
        drop(reopened);
        cleanup(&path);
    }

    #[test]
    fn malformed_or_negative_prices_are_rejected() {
        let (path, connection) = database();
        for value in ["-1", "1e3", "1.1234567890", ""] {
            assert_eq!(
                save_price(
                    &connection,
                    PriceInput {
                        public_model_id: "gpt-test",
                        account_id: None,
                        effective_at: "2026-01-01T00:00:00Z",
                        input_per_million_usd: Some(value),
                        output_per_million_usd: Some("1"),
                        cache_read_per_million_usd: None,
                        cache_write_per_million_usd: None,
                    }
                )
                .unwrap_err()
                .category(),
                GatewayErrorCategory::InvalidInput
            );
        }
        drop(connection);
        cleanup(&path);
    }
}
