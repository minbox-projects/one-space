ALTER TABLE ai_gateway_api_keys ADD COLUMN key_suffix TEXT;
ALTER TABLE ai_gateway_api_keys ADD COLUMN ciphertext BLOB;
ALTER TABLE ai_gateway_api_keys ADD COLUMN nonce BLOB CHECK (nonce IS NULL OR length(nonce) = 12);
ALTER TABLE ai_gateway_api_keys ADD COLUMN cipher_version INTEGER;
