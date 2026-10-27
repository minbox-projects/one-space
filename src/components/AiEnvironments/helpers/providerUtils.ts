import { v4 as uuidv4 } from 'uuid';
import {
  getActiveProviderIds,
  type AiProvider,
  type AiProvidersState,
  type ApiResp,
  type CliTool,
  type SyncedDeviceProvider,
} from '@/lib/serviceProviders';
import type { ClaudePresetModelMapping } from '../providerPresets';

export const TOOLS = ['claude', 'codex', 'antigravity', 'opencode'] as const;
export const MANAGED_TOOLS = ['claude', 'codex', 'antigravity'] as const;
export type EnvManagedState = 'enabled' | 'disabled' | 'unsupported';
export type CliVersionState = { version: string; isInstalled: boolean };
export type DetectCliVersionResult = { version: string; is_installed: boolean };
export type CliInstallCommand = { label: string; command: string };
export type CliInstallGuide = { docs_url: string; commands: CliInstallCommand[] };
export type CliEnvProbeResult = {
  tool: string;
  installed: boolean;
  version: string;
  configured: boolean;
  importable: boolean;
  install_guide: CliInstallGuide;
};
export type CliUpdateInfo = {
  tool: string;
  installed: boolean;
  current_version: string;
  current_version_normalized?: string;
  latest_version?: string;
  latest_source: string;
  latest_url: string;
  update_available: boolean;
  compare_status: string;
  update_command: string;
  error?: string;
};
export type CliUpdateApplyResult = {
  tool: string;
  success: boolean;
  terminal_launched: boolean;
  error?: string;
};
export type PresetDialogDraft = {
  id: string;
  name: string;
  description: string;
  icon: string;
  openai_base_url: string;
  anthropic_base_url: string;
  claude_default_model: string;
  claude_reasoning_effort: string;
  claude_model_mappings: ClaudePresetModelMapping[];
};

export function errorToDisplayMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message || error.stack || String(error);
  }
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object') {
    const maybe = error as { code?: unknown; message?: unknown; error?: unknown; details?: unknown };
    const code = typeof maybe.code === 'string' ? maybe.code : '';
    const message =
      typeof maybe.message === 'string'
        ? maybe.message
        : typeof maybe.error === 'string'
          ? maybe.error
          : '';
    if (code && message) {
      return `${code}: ${message}`;
    }
    if (message) {
      return message;
    }
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }
  return String(error);
}

export function getInvokeErrorCode(error: unknown): string | null {
  if (error && typeof error === 'object') {
    const maybe = error as { code?: unknown };
    if (typeof maybe.code === 'string') return maybe.code;
  }
  return null;
}

export function unwrapApiResp<T>(response: ApiResp<T>, fallbackMessage = 'Request failed'): T {
  if (response?.ok && response.data !== undefined) {
    return response.data;
  }
  throw new Error(errorToDisplayMessage(response) || fallbackMessage);
}

export function buildSyncedProviderActivationPayload(
  deviceId: string,
  provider: SyncedDeviceProvider,
  targetId: string = uuidv4(),
  now: () => number = Date.now,
): { targetId: string; targetTool: CliTool; payload: AiProvider } | null {
  const deviceSlug = String(deviceId || '')
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  const targetTool = String(provider.tool || '').toLowerCase();
  if (!TOOLS.includes(targetTool as CliTool)) {
    return null;
  }

  const payload: AiProvider = {
    id: targetId,
    name: `${provider.name} (${deviceId})`,
    tool: targetTool,
    api_key: String(provider.api_key || '').trim(),
    base_url: provider.base_url || '',
    model: provider.model || '',
    is_enabled: targetTool === 'opencode' ? provider.is_enabled ?? true : true,
    env_managed: targetTool !== 'opencode' ? true : undefined,
  };
  if (targetTool === 'opencode') {
    payload.provider_key =
      provider.provider_key ||
      `synced_${deviceSlug || 'device'}_${now()}`.replace(/[^a-zA-Z]/g, '');
  }

  return { targetId, targetTool: targetTool as CliTool, payload };
}

type FavoriteSortableItem = {
  isActiveForSort: boolean;
  isFavorite: boolean;
  favoriteAt?: number | null;
};

export const sortServiceProviderListItems = <T extends FavoriteSortableItem>(items: T[]) =>
  items
    .map((item, index) => ({ item, index }))
    .sort((a, b) => {
      if (a.item.isActiveForSort !== b.item.isActiveForSort) {
        return a.item.isActiveForSort ? -1 : 1;
      }
      if (a.item.isActiveForSort && b.item.isActiveForSort) {
        return a.index - b.index;
      }
      if (a.item.isFavorite !== b.item.isFavorite) {
        return a.item.isFavorite ? -1 : 1;
      }
      if (a.item.isFavorite && b.item.isFavorite) {
        const aTs = a.item.favoriteAt ?? 0;
        const bTs = b.item.favoriteAt ?? 0;
        if (aTs !== bTs) {
          return bTs - aTs;
        }
      }
      return a.index - b.index;
    })
    .map(({ item }) => item);

export type SavePresetResult = {
  ok: boolean;
  providerId?: string;
  provider?: AiProvider;
};

export type RequiredProviderField = 'api_key' | 'base_url' | 'provider_key' | 'code';

export function getMissingRequiredProviderFields(provider: Partial<AiProvider>): RequiredProviderField[] {
  const missing: RequiredProviderField[] = [];
  if (!String(provider.api_key || '').trim()) {
    missing.push('api_key');
  }
  if (!String(provider.base_url || '').trim()) {
    missing.push('base_url');
  }

  if (String(provider.tool || '').toLowerCase() === 'opencode') {
    if (!String(provider.provider_key || '').trim()) {
      missing.push('provider_key');
    }
  } else if (!String(provider.code || '').trim()) {
    missing.push('code');
  }

  return missing;
}

export const DEFAULT_STATE: AiProvidersState = {
  active_claude: null,
  active_codex: null,
  active_antigravity: null,
  active_opencode: [],
  providers: [],
  is_encrypted: false
};

export function isManagedTool(tool: string): tool is (typeof MANAGED_TOOLS)[number] {
  return (MANAGED_TOOLS as readonly string[]).includes(tool);
}

export function getManagedStateForTool(state: AiProvidersState, tool: CliTool): EnvManagedState {
  if (!isManagedTool(tool)) return 'unsupported';
  const [toolActiveProviderId] = getActiveProviderIds(state, tool);
  if (!toolActiveProviderId) return 'disabled';
  const toolActiveProvider =
    state.providers.find(p => p.id === toolActiveProviderId && p.tool === tool) || null;
  if (!toolActiveProvider) return 'disabled';
  return toolActiveProvider.env_managed !== false ? 'enabled' : 'disabled';
}

export function getIsGlobalForTool(state: AiProvidersState, tool: string, id: string) {
  return getActiveProviderIds(state, tool).includes(id);
}

export function uniqueProviderCode(
  providers: AiProvider[],
  toolName: string,
  presetName?: string,
): string {
  const base = `${presetName || toolName}-provider`
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32) || `${toolName}-provider`;
  const used = new Set(
    providers
      .filter((provider) => provider.tool === toolName)
      .map((provider) => String(provider.code || '').trim().toLowerCase())
      .filter(Boolean),
  );
  let candidate = base;
  let index = 2;
  while (used.has(candidate.toLowerCase())) {
    candidate = `${base}-${index}`;
    index += 1;
  }
  return candidate;
}

export function uniqueOpenCodeProviderKey(providers: AiProvider[], presetName?: string): string {
  const base = `${presetName || 'provider'}`
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 32) || 'provider';
  const used = new Set(
    providers
      .filter((provider) => provider.tool === 'opencode')
      .map((provider) => String(provider.provider_key || '').trim().toLowerCase())
      .filter(Boolean),
  );
  let candidate = base;
  let index = 2;
  while (used.has(candidate.toLowerCase())) {
    candidate = `${base}_${index}`;
    index += 1;
  }
  return candidate;
}
