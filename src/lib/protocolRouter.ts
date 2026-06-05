import { invoke } from '@tauri-apps/api/core';

export type ProtocolWireApi = 'open_ai_chat' | 'open_ai_responses';

export interface ModelMapping {
  claude_model: string;
  upstream_model: string;
}

export interface ProtocolRoute {
  id: string;
  name: string;
  claude_provider_id: string;
  claude_provider_name: string;
  upstream_provider_id: string;
  upstream_provider_name: string;
  base_url: string;
  auth_header?: string | null;
  api_key?: string;
  wire_api: ProtocolWireApi;
  default_model?: string | null;
  mappings: ModelMapping[];
  enabled: boolean;
}

export interface ProtocolRouterConfig {
  enabled: boolean;
  port: number;
  token: string;
  retention_days: number;
  routes: ProtocolRoute[];
}

export interface ProtocolRouterStatus {
  running: boolean;
  enabled: boolean;
  port: number;
  route_count: number;
}

export interface ProtocolRouterCallRecord {
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

export interface ProtocolRouterStatsSummary {
  total_calls: number;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  by_route: AggregateRow[];
  by_provider: AggregateRow[];
  by_model: AggregateRow[];
  calls: ProtocolRouterCallRecord[];
}

export async function protocolRouterGetConfig() {
  return invoke<ProtocolRouterConfig>('protocol_router_get_config');
}

export async function protocolRouterSaveConfig(config: ProtocolRouterConfig) {
  return invoke<ProtocolRouterConfig>('protocol_router_save_config', { config });
}

export async function protocolRouterStart() {
  return invoke<ProtocolRouterStatus>('protocol_router_start');
}

export async function protocolRouterStop() {
  return invoke<ProtocolRouterStatus>('protocol_router_stop');
}

export async function protocolRouterStatus() {
  return invoke<ProtocolRouterStatus>('protocol_router_status');
}

export async function protocolRouterRotateToken() {
  return invoke<ProtocolRouterConfig>('protocol_router_rotate_token');
}

export async function protocolRouterTestConnection(input: { route_id?: string; claude_provider_id?: string; model?: string | null }) {
  return invoke<ProtocolRouterCallRecord>('protocol_router_test_connection', { input });
}

export async function protocolRouterStats(days?: number) {
  return invoke<ProtocolRouterStatsSummary>('protocol_router_stats', {
    query: days ? { days } : null,
  });
}
