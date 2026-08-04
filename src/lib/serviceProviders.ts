import { invoke } from '@tauri-apps/api/core';

export type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
  code?: string;
  message?: string;
  details?: unknown;
};

export type OpenCodeProviderConfig = Record<string, unknown>;

export function serviceProviderReadOpenCodeConfig(
  providerKey: string,
): Promise<ApiResp<OpenCodeProviderConfig>> {
  return invoke<ApiResp<OpenCodeProviderConfig>>('service_provider_read_opencode_config', {
    providerKey,
  });
}
