DROP INDEX ai_gateway_request_attempts_request;
DROP INDEX ai_gateway_request_attempts_account_time;
DROP INDEX ai_gateway_request_attempts_status_time;

ALTER TABLE ai_gateway_request_attempts RENAME TO ai_gateway_request_attempts_v1;

CREATE TABLE ai_gateway_request_attempts (
    id TEXT PRIMARY KEY,
    request_log_id TEXT NOT NULL REFERENCES ai_gateway_request_logs(id) ON DELETE CASCADE,
    attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 6),
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

INSERT INTO ai_gateway_request_attempts (
    id,
    request_log_id,
    attempt_number,
    account_id,
    account_name_snapshot,
    group_id_snapshot,
    group_name_snapshot,
    upstream_model_id_snapshot,
    started_at,
    completed_at,
    status,
    error_code,
    emitted_client_bytes,
    affected_health
)
SELECT
    id,
    request_log_id,
    attempt_number,
    account_id,
    account_name_snapshot,
    group_id_snapshot,
    group_name_snapshot,
    upstream_model_id_snapshot,
    started_at,
    completed_at,
    status,
    error_code,
    emitted_client_bytes,
    affected_health
FROM ai_gateway_request_attempts_v1;

DROP TABLE ai_gateway_request_attempts_v1;

CREATE INDEX ai_gateway_request_attempts_request ON ai_gateway_request_attempts (request_log_id, attempt_number);
CREATE INDEX ai_gateway_request_attempts_account_time ON ai_gateway_request_attempts (account_id, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_attempts_status_time ON ai_gateway_request_attempts (status, error_code, started_at DESC, id DESC);
