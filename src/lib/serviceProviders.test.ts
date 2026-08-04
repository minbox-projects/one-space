import { beforeEach, describe, expect, it } from 'vitest';
import {
  serviceProviderReadOpenCodeConfig,
  type ApiResp,
  type OpenCodeProviderConfig,
} from '@/lib/serviceProviders';
import { invokeMock, resetTauriMocks } from '@/test/mocks/tauri';

describe('service provider typed IPC facade', () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it('映射 OpenCode 配置读取 command 和 providerKey 参数，并透传响应', async () => {
    const response: ApiResp<OpenCodeProviderConfig> = {
      ok: true,
      data: { name: 'Fixture Provider', options: { apiKey: 'fixture-key' } },
      meta: { schema_version: 1, revision: 2 },
    };
    invokeMock.mockResolvedValue(response);

    await expect(serviceProviderReadOpenCodeConfig('FixtureProvider')).resolves.toBe(response);
    expect(invokeMock).toHaveBeenCalledWith('service_provider_read_opencode_config', {
      providerKey: 'FixtureProvider',
    });
  });

  it('原样传播 invoke rejection', async () => {
    const rejection = { code: 'invalid_json', message: 'fixture failure' };
    invokeMock.mockRejectedValue(rejection);

    await expect(serviceProviderReadOpenCodeConfig('FixtureProvider')).rejects.toBe(rejection);
  });
});
