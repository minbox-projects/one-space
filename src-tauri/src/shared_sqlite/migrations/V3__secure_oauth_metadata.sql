ALTER TABLE ai_gateway_request_logs ADD COLUMN api_key_id_snapshot TEXT;
ALTER TABLE ai_gateway_request_logs ADD COLUMN account_id_snapshot TEXT;
ALTER TABLE ai_gateway_daily_aggregates ADD COLUMN details_covered_through TEXT;

UPDATE ai_gateway_request_logs
SET api_key_id_snapshot = api_key_id
WHERE api_key_id_snapshot IS NULL AND api_key_id IS NOT NULL;

UPDATE ai_gateway_request_logs
SET account_id_snapshot = account_id
WHERE account_id_snapshot IS NULL AND account_id IS NOT NULL;

-- 旧 metadata 未受凭据密钥保护。无法安全迁移的 OAuth 私密字段触发重新授权，随后清除明文。
UPDATE ai_gateway_accounts
SET health_status = 'authorization_invalid',
    health_reason_code = 'oauth_reauthorization_required',
    updated_at = CURRENT_TIMESTAMP
WHERE id IN (
    SELECT account_id
    FROM ai_gateway_credentials
    WHERE record_type = 'oauth_token_bundle'
      AND metadata_json IS NOT NULL
      AND NOT json_valid(metadata_json)
);

UPDATE ai_gateway_accounts
SET health_status = 'authorization_invalid',
    health_reason_code = 'oauth_reauthorization_required',
    updated_at = CURRENT_TIMESTAMP
WHERE id IN (
    SELECT credentials.account_id
    FROM ai_gateway_credentials AS credentials
    WHERE credentials.record_type = 'oauth_token_bundle'
      AND CASE
          WHEN json_valid(credentials.metadata_json) THEN EXISTS (
              SELECT 1
              FROM json_tree(credentials.metadata_json) AS metadata
              WHERE lower(CAST(metadata.key AS TEXT)) IN (
                  'access_token', 'refresh_token', 'client_id', 'client_secret',
                  'api_key', 'authorization', 'credential', 'password',
                  'private_key', 'secret', 'token'
              )
          )
          ELSE 0
      END
);

-- 仅保留公开 token endpoint；新的 OAuth 秘密只写入 AES-GCM 载荷。
UPDATE ai_gateway_credentials
SET metadata_json = CASE
    WHEN record_type = 'oauth_token_bundle'
         AND json_valid(metadata_json)
    THEN CASE
        WHEN json_type(metadata_json, '$.token_endpoint') = 'text'
             AND trim(json_extract(metadata_json, '$.token_endpoint')) <> ''
        THEN json_object('token_endpoint', json_extract(metadata_json, '$.token_endpoint'))
        ELSE NULL
    END
    ELSE NULL
END
WHERE metadata_json IS NOT NULL;

CREATE INDEX ai_gateway_request_logs_account_snapshot_time
    ON ai_gateway_request_logs (account_id_snapshot, started_at DESC, id DESC);
CREATE INDEX ai_gateway_request_logs_key_snapshot_time
    ON ai_gateway_request_logs (api_key_id_snapshot, started_at DESC, id DESC);
