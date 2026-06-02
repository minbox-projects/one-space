import { Loader2 } from 'lucide-react';
import { ToolAvatarIcon } from './icons';

interface SyncedDeviceProvider {
  id: string;
  name: string;
  tool: string;
  api_key: string;
  base_url?: string;
  model?: string;
  provider_key?: string;
  is_enabled?: boolean;
}

interface SyncedDevicesProps {
  syncedOtherDeviceProviders: Array<{
    device_id: string;
    active?: Record<string, string>;
    providers: SyncedDeviceProvider[];
  }>;
  activeTool: string;
  onActivate: (deviceId: string, provider: SyncedDeviceProvider) => void;
  loading: boolean;
  activatingSyncedKey: string | null;
  t: (key: string, options?: any) => string;
}

export function SyncedDevices({
  syncedOtherDeviceProviders,
  activeTool,
  onActivate,
  loading,
  activatingSyncedKey,
  t,
}: SyncedDevicesProps) {
  const toolGroups = syncedOtherDeviceProviders
    .map(device => ({
      deviceId: device.device_id,
      activeId: (device.active || {})[activeTool] || null,
      providers: (device.providers || []).filter(p => p.tool === activeTool),
    }))
    .filter(g => g.providers.length > 0);

  if (toolGroups.length === 0) return null;

  return (
    <div className="space-y-2">
      <div className="px-2 py-1 text-[10px] font-semibold text-muted-foreground uppercase tracking-wider">
        {t('syncedFromOtherDevices', 'Synced from other devices')}
      </div>
      {toolGroups.map(group => (
        <div key={`${activeTool}-${group.deviceId}`} className="space-y-1">
          <div className="px-2 text-[10px] text-muted-foreground">{group.deviceId}</div>
          {group.providers.map(provider => {
            const actionKey = `${group.deviceId}:${provider.tool}:${provider.id}`;
            const canActivate = !!String(provider.api_key || '').trim();
            const isActive = group.activeId === provider.id;
            return (
              <button
                key={`synced-${group.deviceId}-${provider.id}`}
                type="button"
                onClick={() => onActivate(group.deviceId, provider)}
                disabled={!canActivate || loading || activatingSyncedKey === actionKey}
                title={
                  canActivate
                    ? t('activateSyncedProvider', 'Import and activate')
                    : t('syncedProviderMissingApiKey', 'This environment lacks a decryptable API Key and cannot be activated directly.')
                }
                className="w-full flex items-center gap-2 px-3 py-2 text-sm rounded-md transition-colors bg-muted/20 hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <span className="shrink-0 text-muted-foreground">
                  <ToolAvatarIcon tool={provider.tool} className="w-4 h-4" />
                </span>
                <span className="truncate flex-1 text-left">{provider.name}</span>
                {provider.model && (
                  <span className="text-[10px] px-1.5 py-0.5 rounded border bg-blue-500/10 text-blue-700 border-blue-500/30">
                    {provider.model}
                  </span>
                )}
                {activatingSyncedKey === actionKey && (
                  <Loader2 className="w-3.5 h-3.5 animate-spin text-muted-foreground" />
                )}
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}
