import { invoke } from "@tauri-apps/api/core";

export const API_FUSION_DEFAULT_PORT = 17688;

/** Masked secret value the backend echoes for stored provider api keys and local keys. */
export const API_FUSION_KEY_MASK = "********";

/** Terminal tools API Fusion is allowed to configure; claude/antigravity are excluded. */
export const API_FUSION_SUPPORTED_TERMINAL_TOOLS = ["opencode", "codex"] as const;

export interface FusionModelMapping {
  local_model: string;
  upstream_model: string;
}

/** Upstream endpoint family a provider exposes; request bodies are not translated. */
export type FusionUpstreamProtocol = "chat_completions" | "responses";

export interface FusionUpstreamProvider {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
  default_model: string | null;
  protocol?: FusionUpstreamProtocol;
  mappings: FusionModelMapping[];
  enabled: boolean;
  auto_disabled: boolean;
  disabled_reason: string | null;
  disabled_at: number | null;
  consecutive_failures: number;
  last_error_at: number | null;
}

export interface FusionKey {
  id: string;
  label: string;
  value: string;
  enabled: boolean;
  created_at: number;
}

export interface FusionTerminalSyncRecord {
  provider_id: string;
  tool: string;
  synced_key_id: string;
  synced_base_url: string;
  synced_at: number;
}

export interface FusionConfig {
  enabled: boolean;
  port: number;
  providers: FusionUpstreamProvider[];
  keys: FusionKey[];
  default_key_id: string | null;
  terminal_syncs: FusionTerminalSyncRecord[];
}

export interface FusionStatus {
  running: boolean;
  enabled: boolean;
  port: number;
  local_base_url: string;
  provider_count: number;
  auto_disabled_count: number;
  key_count: number;
  default_key_id: string | null;
}

export interface FusionTerminalTarget {
  provider_id: string;
  tool: string;
  name: string;
  base_url: string | null;
  api_key: string;
  synced: boolean;
  pending_sync: boolean;
  synced_key_id: string | null;
  synced_at: number | null;
}

/**
 * Resolve the effective default local key id.
 *
 * Mirrors the backend rule: a manual choice wins while it points at an enabled
 * key; otherwise the search advances to the next enabled key in list order
 * (wrapping), and empty means no enabled key is available.
 */
export function resolveDefaultKeyId(
  keys: FusionKey[],
  stored: string | null | undefined,
): string | null {
  if (keys.length === 0) return null;
  const storedIndex = stored
    ? keys.findIndex((key) => key.id === stored)
    : -1;

  if (storedIndex >= 0) {
    if (keys[storedIndex].enabled) return keys[storedIndex].id;
    for (let offset = 1; offset <= keys.length; offset += 1) {
      const candidate = (storedIndex + offset) % keys.length;
      if (keys[candidate].enabled) return keys[candidate].id;
    }
    return null;
  }

  return keys.find((key) => key.enabled)?.id ?? null;
}

/** Build the local OpenAI-compatible base address the listener binds to. */
export function localBaseUrl(port: number): string {
  return `http://127.0.0.1:${port}`;
}

/**
 * Resolve the upstream model forwarded for a requested local model.
 *
 * Exact mapping match wins, then the provider default model; `null` means the
 * provider cannot serve the requested model.
 */
export function resolveUpstreamModelPreview(
  provider: Pick<FusionUpstreamProvider, "mappings" | "default_model">,
  localModel: string,
): string | null {
  const mapping = provider.mappings.find(
    (entry) => entry.local_model === localModel,
  );
  if (mapping) return mapping.upstream_model;
  return provider.default_model ?? null;
}

/**
 * Decide whether a terminal target must be synced again.
 *
 * Pending-sync is derived purely from the persisted `terminal_syncs` ledger
 * compared against the current default key id and local base address. The
 * redacted `api_key` echoed by the backend is intentionally never consulted.
 */
export function isTerminalSyncPending(
  target: Pick<FusionTerminalTarget, "provider_id">,
  config: Pick<
    FusionConfig,
    "keys" | "default_key_id" | "terminal_syncs" | "port"
  >,
): boolean {
  const record = config.terminal_syncs.find(
    (entry) => entry.provider_id === target.provider_id,
  );
  if (!record) return true;

  const currentKeyId = resolveDefaultKeyId(config.keys, config.default_key_id);
  if (!currentKeyId) return true;

  return (
    record.synced_key_id !== currentKeyId ||
    record.synced_base_url !== localBaseUrl(config.port)
  );
}

/** Redact a secret for display while keeping head/tail recognizable. */
export function maskSecret(value: string): string {
  if (!value) return "";
  if (value.length <= 8) return "•".repeat(value.length);
  return `${value.slice(0, 3)}${"•".repeat(6)}${value.slice(-4)}`;
}

/** Render a unix-seconds timestamp as a stable `YYYY-MM-DD HH:mm` string. */
export function formatFusionTimestamp(ts: number | null | undefined): string | null {
  if (ts === null || ts === undefined) return null;
  const date = new Date(ts * 1000);
  const pad = (part: number) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

export function apiFusionGetConfig() {
  return invoke<FusionConfig>("api_fusion_get_config");
}

export function apiFusionSaveConfig(config: FusionConfig) {
  return invoke<FusionConfig>("api_fusion_save_config", { config });
}

export function apiFusionUpsertProvider(provider: FusionUpstreamProvider) {
  return invoke<FusionConfig>("api_fusion_upsert_provider", { provider });
}

export function apiFusionDeleteProvider(providerId: string) {
  return invoke<FusionConfig>("api_fusion_delete_provider", { providerId });
}

export function apiFusionSetProviderEnabled(
  providerId: string,
  enabled: boolean,
) {
  return invoke<FusionConfig>("api_fusion_set_provider_enabled", {
    providerId,
    enabled,
  });
}

export function apiFusionReenableProvider(providerId: string) {
  return invoke<FusionConfig>("api_fusion_reenable_provider", { providerId });
}

export function apiFusionUpsertKey(key: FusionKey) {
  return invoke<FusionConfig>("api_fusion_upsert_key", { key });
}

export function apiFusionDeleteKey(keyId: string) {
  return invoke<FusionConfig>("api_fusion_delete_key", { keyId });
}

export function apiFusionSetDefaultKey(keyId: string) {
  return invoke<FusionConfig>("api_fusion_set_default_key", { keyId });
}

export function apiFusionStart() {
  return invoke<FusionStatus>("api_fusion_start");
}

export function apiFusionStop() {
  return invoke<FusionStatus>("api_fusion_stop");
}

export function apiFusionStatus() {
  return invoke<FusionStatus>("api_fusion_status");
}

export function apiFusionTerminalTargets() {
  return invoke<FusionTerminalTarget[]>("api_fusion_terminal_targets");
}

export function apiFusionConfigureTerminal(targetIds: string[]) {
  return invoke<FusionTerminalSyncRecord[]>("api_fusion_configure_terminal", {
    targetIds,
  });
}

export function apiFusionSyncTerminal(targetIds?: string[]) {
  return invoke<FusionTerminalSyncRecord[]>(
    "api_fusion_sync_terminal",
    targetIds ? { targetIds } : {},
  );
}
