ALTER TABLE ai_gateway_request_logs ADD COLUMN api_key_id_snapshot TEXT;
ALTER TABLE ai_gateway_request_logs ADD COLUMN account_id_snapshot TEXT;
ALTER TABLE ai_gateway_daily_aggregates ADD COLUMN details_covered_through TEXT;

UPDATE ai_gateway_request_logs
SET api_key_id_snapshot = api_key_id
WHERE api_key_id_snapshot IS NULL AND api_key_id IS NOT NULL;

UPDATE ai_gateway_request_logs
SET account_id_snapshot = account_id
WHERE account_id_snapshot IS NULL AND account_id IS NOT NULL;

-- 旧 metadata 未受凭据密钥保护，迁移时清除；新的 OAuth 秘密只写入 AES-GCM 载荷。
UPDATE ai_gateway_credentials SET metadata_json = NULL WHERE metadata_json IS NOT NULL;

CREATE INDEX ai_gateway_request_logs_account_snapshot_time
    ON ai_gateway_request_logs (account_id_snapshot, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_logs_key_snapshot_time
    ON ai_gateway_request_logs (api_key_id_snapshot, started_at DESC, id DESC);
