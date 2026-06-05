import { invoke } from '@tauri-apps/api/core';

export type ProtocolWireApi = 'open_ai_chat' | 'open_ai_responses' | 'anthropic_messages';

export interface CatalogModel {
  id: string;
  object?: string | null;
  created?: number | null;
  owned_by?: string | null;
}

export interface ModelCatalogSource {
  id: string;
  name: string;
  models_url: string;
  base_url: string;
  auth_header?: string | null;
  api_key: string;
  model_id_prefix?: string | null;
  default_wire_api: ProtocolWireApi;
  enabled: boolean;
  last_loaded_at?: number | null;
  cached_models: CatalogModel[];
}

export interface ModelMapping {
  claude_model: string;
  upstream_model: string;
}

export interface ProtocolRoute {
  id: string;
  name: string;
  provider_id: string;
  provider_name: string;
  base_url: string;
  auth_header?: string | null;
  api_key: string;
  wire_api: ProtocolWireApi;
  default_model?: string | null;
  mappings: ModelMapping[];
  enabled: boolean;
}

export interface ProtocolProxyConfig {
  enabled: boolean;
  port: number;
  token: string;
  retention_days: number;
  routes: ProtocolRoute[];
  catalog_sources: ModelCatalogSource[];
}

export interface ProtocolProxyStatus {
  running: boolean;
  enabled: boolean;
  port: number;
  route_count: number;
}

export interface ProtocolProxyCallRecord {
  ts: number;
  route_id: string;
  provider: string;
  model: string;
  endpoint: string;
  wire_api: ProtocolWireApi;
  status: number;
  latency_ms: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  error_summary?: string | null;
}

export interface AggregateRow {
  key: string;
  calls: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
}

export interface ProtocolProxyStatsSummary {
  total_calls: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  by_route: AggregateRow[];
  by_provider: AggregateRow[];
  by_model: AggregateRow[];
  calls: ProtocolProxyCallRecord[];
}

export async function protocolProxyGetConfig() {
  return invoke<ProtocolProxyConfig>('protocol_proxy_get_config');
}

export async function protocolProxySaveConfig(config: ProtocolProxyConfig) {
  return invoke<ProtocolProxyConfig>('protocol_proxy_save_config', { config });
}

export async function protocolProxyStart() {
  return invoke<ProtocolProxyStatus>('protocol_proxy_start');
}

export async function protocolProxyStop() {
  return invoke<ProtocolProxyStatus>('protocol_proxy_stop');
}

export async function protocolProxyStatus() {
  return invoke<ProtocolProxyStatus>('protocol_proxy_status');
}

export async function protocolProxyRotateToken() {
  return invoke<ProtocolProxyConfig>('protocol_proxy_rotate_token');
}

export async function protocolProxyFetchModels(sourceId: string) {
  return invoke<CatalogModel[]>('protocol_proxy_fetch_models', { sourceId });
}

export async function protocolProxyTestConnection(input: { route_id: string; model?: string | null }) {
  return invoke<ProtocolProxyCallRecord>('protocol_proxy_test_connection', { input });
}

export async function protocolProxyStats(days?: number) {
  return invoke<ProtocolProxyStatsSummary>('protocol_proxy_stats', {
    query: days ? { days } : null,
  });
}
