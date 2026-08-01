CREATE TABLE ai_gateway_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    port INTEGER NOT NULL DEFAULT 17688 CHECK (port BETWEEN 1 AND 65535),
    global_quota_threshold_percent INTEGER NOT NULL DEFAULT 10 CHECK (global_quota_threshold_percent BETWEEN 0 AND 100),
    log_retention_days INTEGER CHECK (log_retention_days IN (7, 30, 90, 180) OR log_retention_days IS NULL),
    run_enabled INTEGER NOT NULL DEFAULT 1 CHECK (run_enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO ai_gateway_settings (id, port, global_quota_threshold_percent, log_retention_days, run_enabled)
VALUES (1, 17688, 10, 90, 1);

CREATE TABLE ai_gateway_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX ai_gateway_groups_one_default ON ai_gateway_groups (is_default) WHERE is_default = 1;
CREATE INDEX ai_gateway_groups_sort ON ai_gateway_groups (sort_order, id);
INSERT INTO ai_gateway_groups (id, name, sort_order, is_default) VALUES ('default', 'Default', 0, 1);

CREATE TABLE ai_gateway_accounts (
    id TEXT PRIMARY KEY,
    stable_external_id TEXT,
    account_type TEXT NOT NULL CHECK (account_type IN ('oauth', 'api_key')),
    name TEXT NOT NULL,
    group_id TEXT NOT NULL REFERENCES ai_gateway_groups(id) ON DELETE RESTRICT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    note TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    health_status TEXT NOT NULL DEFAULT 'unknown',
    health_reason_code TEXT,
    last_used_at TEXT,
    quota_threshold_override_percent INTEGER CHECK (quota_threshold_override_percent BETWEEN 0 AND 100),
    base_url TEXT,
    auth_method TEXT,
    upstream_protocol TEXT CHECK (upstream_protocol IN ('responses', 'chat_completions') OR upstream_protocol IS NULL),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (account_type, stable_external_id)
);
CREATE INDEX ai_gateway_accounts_group_sort ON ai_gateway_accounts (group_id, sort_order, id);
CREATE INDEX ai_gateway_accounts_routing ON ai_gateway_accounts (enabled, health_status, sort_order, last_used_at, id);

CREATE TABLE ai_gateway_tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE ai_gateway_account_tags (
    account_id TEXT NOT NULL REFERENCES ai_gateway_accounts(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES ai_gateway_tags(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, tag_id)
);
CREATE INDEX ai_gateway_account_tags_by_tag ON ai_gateway_account_tags (tag_id, account_id);

CREATE TABLE ai_gateway_credentials (
    account_id TEXT PRIMARY KEY REFERENCES ai_gateway_accounts(id) ON DELETE CASCADE,
    record_type TEXT NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL CHECK (length(nonce) = 12),
    cipher_version INTEGER NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE ai_gateway_models (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    source TEXT NOT NULL DEFAULT 'official',
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE ai_gateway_account_model_mappings (
    account_id TEXT NOT NULL REFERENCES ai_gateway_accounts(id) ON DELETE CASCADE,
    public_model_id TEXT NOT NULL REFERENCES ai_gateway_models(id) ON DELETE CASCADE,
    upstream_model_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (account_id, public_model_id)
);
CREATE INDEX ai_gateway_model_mappings_public ON ai_gateway_account_model_mappings (public_model_id, enabled, account_id);

CREATE TABLE ai_gateway_quota_windows (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES ai_gateway_accounts(id) ON DELETE CASCADE,
    upstream_window_id TEXT,
    name TEXT NOT NULL,
    scope_type TEXT NOT NULL CHECK (scope_type IN ('global', 'model', 'endpoint', 'capability', 'unknown')),
    scope_value TEXT,
    used_percent REAL,
    remaining_percent REAL,
    resets_at TEXT,
    duration_seconds INTEGER,
    last_succeeded_at TEXT,
    is_stale INTEGER NOT NULL DEFAULT 0 CHECK (is_stale IN (0, 1)),
    raw_kind TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (account_id, upstream_window_id)
);
CREATE INDEX ai_gateway_quota_scope_reset ON ai_gateway_quota_windows (account_id, scope_type, scope_value, resets_at);
CREATE INDEX ai_gateway_quota_routing ON ai_gateway_quota_windows (account_id, is_stale, remaining_percent, resets_at);

CREATE TABLE ai_gateway_api_keys (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    key_hash BLOB NOT NULL,
    hash_salt BLOB NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    expires_at TEXT,
    revoked_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX ai_gateway_api_keys_hash ON ai_gateway_api_keys (key_hash);
CREATE INDEX ai_gateway_api_keys_status ON ai_gateway_api_keys (enabled, revoked_at, expires_at);
CREATE TABLE ai_gateway_api_key_groups (
    api_key_id TEXT NOT NULL REFERENCES ai_gateway_api_keys(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL REFERENCES ai_gateway_groups(id) ON DELETE CASCADE,
    PRIMARY KEY (api_key_id, group_id)
);
CREATE INDEX ai_gateway_api_key_groups_group ON ai_gateway_api_key_groups (group_id, api_key_id);
CREATE TABLE ai_gateway_api_key_models (
    api_key_id TEXT NOT NULL REFERENCES ai_gateway_api_keys(id) ON DELETE CASCADE,
    model_id TEXT NOT NULL REFERENCES ai_gateway_models(id) ON DELETE CASCADE,
    PRIMARY KEY (api_key_id, model_id)
);
CREATE INDEX ai_gateway_api_key_models_model ON ai_gateway_api_key_models (model_id, api_key_id);

CREATE TABLE ai_gateway_model_prices (
    id TEXT PRIMARY KEY,
    public_model_id TEXT NOT NULL REFERENCES ai_gateway_models(id) ON DELETE CASCADE,
    account_id TEXT REFERENCES ai_gateway_accounts(id) ON DELETE CASCADE,
    input_per_million_usd TEXT,
    output_per_million_usd TEXT,
    cache_read_per_million_usd TEXT,
    cache_write_per_million_usd TEXT,
    source TEXT NOT NULL CHECK (source IN ('official', 'account_override')),
    effective_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (public_model_id, account_id, effective_at)
);
CREATE INDEX ai_gateway_model_prices_lookup ON ai_gateway_model_prices (public_model_id, account_id, effective_at DESC);

CREATE TABLE ai_gateway_request_logs (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    local_date TEXT NOT NULL,
    timezone_name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    public_model_id TEXT NOT NULL,
    upstream_model_id_snapshot TEXT,
    api_key_id TEXT REFERENCES ai_gateway_api_keys(id) ON DELETE SET NULL,
    api_key_name_snapshot TEXT,
    account_id TEXT REFERENCES ai_gateway_accounts(id) ON DELETE SET NULL,
    account_name_snapshot TEXT,
    group_id_snapshot TEXT,
    group_name_snapshot TEXT,
    status TEXT NOT NULL,
    error_code TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cache_read_tokens INTEGER,
    cache_write_tokens INTEGER,
    total_tokens INTEGER,
    price_snapshot_json TEXT,
    estimated_cost_usd TEXT,
    cost_calculable INTEGER NOT NULL DEFAULT 0 CHECK (cost_calculable IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX ai_gateway_request_logs_time ON ai_gateway_request_logs (started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_logs_account_time ON ai_gateway_request_logs (account_id, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_logs_group_time ON ai_gateway_request_logs (group_id_snapshot, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_logs_model_time ON ai_gateway_request_logs (public_model_id, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_logs_key_time ON ai_gateway_request_logs (api_key_id, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_logs_status_time ON ai_gateway_request_logs (status, error_code, started_at DESC, id DESC);

CREATE TABLE ai_gateway_request_attempts (
    id TEXT PRIMARY KEY,
    request_log_id TEXT NOT NULL REFERENCES ai_gateway_request_logs(id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 3),
    account_id TEXT REFERENCES ai_gateway_accounts(id) ON DELETE SET NULL,
    account_name_snapshot TEXT NOT NULL,
    group_id_snapshot TEXT,
    group_name_snapshot TEXT,
    upstream_model_id_snapshot TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL,
    error_code TEXT,
    emitted_client_bytes INTEGER NOT NULL DEFAULT 0 CHECK (emitted_client_bytes IN (0, 1)),
    affected_health INTEGER NOT NULL DEFAULT 0 CHECK (affected_health IN (0, 1)),
    UNIQUE (request_log_id, attempt_number)
);
CREATE INDEX ai_gateway_request_attempts_request ON ai_gateway_request_attempts (request_log_id, attempt_number);
CREATE INDEX ai_gateway_request_attempts_account_time ON ai_gateway_request_attempts (account_id, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_attempts_status_time ON ai_gateway_request_attempts (status, error_code, started_at DESC, id DESC);

CREATE TABLE ai_gateway_daily_aggregates (
    local_date TEXT NOT NULL,
    timezone_name TEXT NOT NULL,
    account_id_snapshot TEXT NOT NULL DEFAULT '',
    account_name_snapshot TEXT,
    group_id_snapshot TEXT NOT NULL DEFAULT '',
    group_name_snapshot TEXT,
    public_model_id TEXT NOT NULL DEFAULT '',
    request_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cache_read_tokens INTEGER,
    cache_write_tokens INTEGER,
    total_tokens INTEGER,
    estimated_cost_usd TEXT,
    cost_calculable INTEGER NOT NULL DEFAULT 1 CHECK (cost_calculable IN (0, 1)),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (local_date, timezone_name, account_id_snapshot, group_id_snapshot, public_model_id)
);
CREATE INDEX ai_gateway_daily_aggregates_range ON ai_gateway_daily_aggregates (local_date, public_model_id, group_id_snapshot, account_id_snapshot);
