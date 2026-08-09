CREATE TABLE ai_gateway_key_display_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK (length(trim(name)) > 0),
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX ai_gateway_key_display_groups_one_default
    ON ai_gateway_key_display_groups (is_default)
    WHERE is_default = 1;

CREATE TRIGGER ai_gateway_key_display_groups_protect_default_delete
BEFORE DELETE ON ai_gateway_key_display_groups
WHEN OLD.is_default = 1
BEGIN
    SELECT RAISE(ABORT, 'ai_gateway_key_display_groups_default_required');
END;

CREATE TRIGGER ai_gateway_key_display_groups_protect_default_update
BEFORE UPDATE ON ai_gateway_key_display_groups
WHEN OLD.is_default = 1
 AND (NEW.id <> OLD.id OR NEW.name <> OLD.name OR NEW.is_default <> 1)
BEGIN
    SELECT RAISE(ABORT, 'ai_gateway_key_display_groups_default_immutable');
END;

INSERT INTO ai_gateway_key_display_groups (id, name, is_default)
VALUES ('gateway-key-default', 'Default', 1);

ALTER TABLE ai_gateway_api_keys
    ADD COLUMN display_group_id TEXT NOT NULL DEFAULT 'gateway-key-default'
    REFERENCES ai_gateway_key_display_groups(id) ON DELETE RESTRICT;

CREATE INDEX ai_gateway_api_keys_display_group_created
    ON ai_gateway_api_keys (display_group_id, created_at DESC, id DESC);

CREATE TABLE ai_gateway_key_provider_conversions (
    gateway_key_id TEXT NOT NULL REFERENCES ai_gateway_api_keys(id) ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK (tool IN ('claude', 'codex', 'gemini', 'opencode')),
    service_provider_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (gateway_key_id, tool)
);

CREATE INDEX ai_gateway_key_provider_conversions_provider
    ON ai_gateway_key_provider_conversions (service_provider_id);
