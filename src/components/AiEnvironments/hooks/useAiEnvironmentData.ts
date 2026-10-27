import { useEffect } from 'react';
import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import { invoke } from '@tauri-apps/api/core';
import {
  claudeProfileList,
  serviceProvidersAutoImportFromSystem,
  serviceProvidersList,
  serviceProvidersListSyncedOtherDevices,
  type AiProvidersState,
  type ApiResp,
  type AutoImportResult,
  type ClaudeProfileSummary,
  type CliTool,
  type SyncedDeviceProvidersView,
} from '@/lib/serviceProviders';
import { getAiSessionStorageConfig } from '@/lib/aiSessions';
import type { ServiceProviderPresetRecord, ServiceProviderPresetsState } from '../providerPresets';
import {
  DEFAULT_STATE,
  MANAGED_TOOLS,
  TOOLS,
  type CliEnvProbeResult,
  type CliUpdateInfo,
  type CliVersionState,
  type DetectCliVersionResult,
} from '../helpers/providerUtils';

interface UseAiEnvironmentDataParams {
  t: TFunction;
  isTauri: boolean;
  isVisible: boolean;
  activeTool: string;
  currentProviderId: string | null;
  state: AiProvidersState;
  setState: Dispatch<SetStateAction<AiProvidersState>>;
  setCurrentProviderId: Dispatch<SetStateAction<string | null>>;
  setMessage: Dispatch<SetStateAction<{ type: string; text: string }>>;
  setLoading: Dispatch<SetStateAction<boolean>>;
  setProviderPresets: Dispatch<SetStateAction<ServiceProviderPresetRecord[]>>;
  setSyncedOtherDeviceProviders: Dispatch<SetStateAction<SyncedDeviceProvidersView[]>>;
  setUnsavedNewProviderIds: Dispatch<SetStateAction<Set<string>>>;
  setClaudeProfiles: Dispatch<SetStateAction<ClaudeProfileSummary[]>>;
  setClaudeLaunchCommand: Dispatch<SetStateAction<string>>;
  setAutoImportInactiveNotice: Dispatch<SetStateAction<Partial<Record<CliTool, string>>>>;
  setCliVersions: Dispatch<SetStateAction<Partial<Record<CliTool, CliVersionState>>>>;
  setCheckingVersions: Dispatch<SetStateAction<Partial<Record<CliTool, boolean>>>>;
  setCheckingAllVersions: Dispatch<SetStateAction<boolean>>;
  setCliUpdates: Dispatch<SetStateAction<Partial<Record<CliTool, CliUpdateInfo>>>>;
  setCheckingUpdates: Dispatch<SetStateAction<Partial<Record<CliTool, boolean>>>>;
  setProbingTool: Dispatch<SetStateAction<Partial<Record<CliTool, boolean>>>>;
  versionCheckRunIdRef: MutableRefObject<number>;
  probeRunIdRef: MutableRefObject<number>;
  isVisibleRef: MutableRefObject<boolean>;
  cliProbeInitializedRef: MutableRefObject<boolean>;
  autoImportInitializedRef: MutableRefObject<boolean>;
}

export function useAiEnvironmentData({
  t,
  isTauri,
  isVisible,
  activeTool,
  currentProviderId,
  state,
  setState,
  setCurrentProviderId,
  setMessage,
  setLoading,
  setProviderPresets,
  setSyncedOtherDeviceProviders,
  setUnsavedNewProviderIds,
  setClaudeProfiles,
  setClaudeLaunchCommand,
  setAutoImportInactiveNotice,
  setCliVersions,
  setCheckingVersions,
  setCheckingAllVersions,
  setCliUpdates,
  setCheckingUpdates,
  setProbingTool,
  versionCheckRunIdRef,
  probeRunIdRef,
  isVisibleRef,
  cliProbeInitializedRef,
  autoImportInitializedRef,
}: UseAiEnvironmentDataParams) {
  const loadProviders = async (silent = false) => {
    if (!isTauri) return;
    if (!silent) setLoading(true);
    try {
      const res = await serviceProvidersList();
      if (silent && !isVisibleRef.current) return;

      if (res.data.providers && res.data.providers.length > 0) {
        setState(res.data);
      } else {
        // Only set default if it was truly empty and we didn't have existing state
        // This prevents wiping state if backend temporarily returns empty
        setState(prev => prev.providers.length > 0 ? prev : DEFAULT_STATE);
      }
      try {
        const presetsRes = await invoke<ApiResp<ServiceProviderPresetsState>>('service_provider_presets_list');
        if (silent && !isVisibleRef.current) return;
        setProviderPresets(presetsRes.data.presets || []);
      } catch (presetErr) {
        console.warn('Failed to load service provider presets:', presetErr);
      }
      setUnsavedNewProviderIds(new Set());
      try {
        const syncedRes = await serviceProvidersListSyncedOtherDevices();
        if (silent && !isVisibleRef.current) return;
        setSyncedOtherDeviceProviders(syncedRes.data || []);
      } catch (syncErr) {
        console.warn('Failed to load synced other-device providers:', syncErr);
        if (!silent) {
          setSyncedOtherDeviceProviders([]);
        }
      }
    } catch (e: any) {
      console.error('Failed to load AI providers:', e);
      setMessage({ type: 'error', text: `Failed to load providers: ${e.toString()}` });
    } finally {
      if (!silent) setLoading(false);
    }
  };

  const loadClaudeProfiles = async () => {
    if (!isTauri) return;
    try {
      const res = await claudeProfileList();
      if (res.data) {
        setClaudeProfiles(res.data);
      }
    } catch (e: any) {
      console.error('Failed to load Claude profiles:', e);
    }
  };

  async function detectAllVersions(runId: number = ++versionCheckRunIdRef.current) {
    if (!isTauri) return;
    const initialCheckingState = TOOLS.reduce((acc, tool) => {
      acc[tool] = true;
      return acc;
    }, {} as Partial<Record<CliTool, boolean>>);
    setCheckingAllVersions(true);
    setCheckingVersions(initialCheckingState);
    setCheckingUpdates(initialCheckingState);
    try {
      const results = await Promise.all(
        TOOLS.map(async tool => {
          try {
            const result = await invoke<DetectCliVersionResult>('detect_cli_version', { tool });
            return { tool, state: { version: result.version, isInstalled: result.is_installed } };
          } catch (e) {
            console.error(`Failed to detect ${tool} version:`, e);
            return { tool, state: { version: '', isInstalled: false } };
          }
        })
      );
      if (versionCheckRunIdRef.current !== runId) return;
      const nextVersions = results.reduce((acc, item) => {
        acc[item.tool] = item.state;
        return acc;
      }, {} as Partial<Record<CliTool, CliVersionState>>);
      setCliVersions(prev => ({
        ...prev,
        ...nextVersions
      }));
      setCheckingVersions({});

      // Fetch update info for all tools
      const updateResults = await Promise.all(
        TOOLS.map(async tool => {
          try {
            const updateInfo = await invoke<CliUpdateInfo>('check_cli_update', { tool });
            return { tool, info: updateInfo };
          } catch (e) {
            console.error(`Failed to check ${tool} update:`, e);
            return { tool, info: undefined };
          }
        })
      );
      if (versionCheckRunIdRef.current !== runId) return;
      const nextUpdates = updateResults.reduce((acc, item) => {
        if (item.info) {
          acc[item.tool] = item.info;
        } else {
          delete acc[item.tool];
        }
        return acc;
      }, {} as Partial<Record<CliTool, CliUpdateInfo>>);
      setCliUpdates(prev => ({ ...prev, ...nextUpdates }));
    } finally {
      if (versionCheckRunIdRef.current === runId) {
        setCheckingUpdates({});
        setCheckingAllVersions(false);
      }
    }
  }

  async function preloadCliMetaAndAutoImport(runId: number = ++probeRunIdRef.current) {
    if (!isTauri) return;

    if (!cliProbeInitializedRef.current) {
      const initialProbingState = TOOLS.reduce((acc, tool) => {
        acc[tool] = true;
        return acc;
      }, {} as Partial<Record<CliTool, boolean>>);
      setProbingTool(initialProbingState);
      const results = await Promise.all(
        TOOLS.map(async tool => {
          try {
            const res = await invoke<ApiResp<CliEnvProbeResult>>('cli_env_probe', { tool });
            return { tool, data: res.data };
          } catch (e) {
            console.error(`Failed to probe ${tool} cli env:`, e);
            return { tool, data: undefined };
          }
        })
      );
      if (probeRunIdRef.current !== runId) return;
      const nextProbe = results.reduce((acc, item) => {
        if (item.data) {
          acc[item.tool] = item.data;
        }
        return acc;
      }, {} as Partial<Record<CliTool, CliEnvProbeResult>>);
      if (Object.keys(nextProbe).length > 0) {
        // Probing results are tracked via cliProbeInitializedRef only; individual results
        // are passed through CliVersionCards props, so no local state update is needed here.
      }
      setProbingTool({});
      cliProbeInitializedRef.current = true;
    }
    if (probeRunIdRef.current !== runId) return;

    if (!autoImportInitializedRef.current) {
      const autoImportResults = await Promise.all(
        MANAGED_TOOLS.map(async tool => {
          try {
            const res = await serviceProvidersAutoImportFromSystem(tool);
            return { tool, data: res.data };
          } catch (e) {
            console.error(`Auto import failed for ${tool}:`, e);
            return { tool, data: null as AutoImportResult | null };
          }
        })
      );
      if (probeRunIdRef.current !== runId) return;
      const importedAny = autoImportResults.some(item => !!item.data?.imported);
      setAutoImportInactiveNotice(prev => {
        const next = { ...prev };
        for (const item of autoImportResults) {
          if (!item.data?.imported) continue;
          if (item.data.activated === false) {
            const missingFieldLabels = (item.data.missing_fields || []).map(field => {
              if (field === 'api_key') return t('apiKey', 'API Key');
              if (field === 'base_url') return t('baseUrl', 'Base URL');
              return field;
            });
            const missingText = missingFieldLabels.join(' + ');
            next[item.tool] = missingText
              ? t(
                  'autoImportedButInactiveMissingFields',
                  { fields: missingText }
                )
              : t('autoImportedButInactive');
          } else {
            delete next[item.tool];
          }
        }
        return next;
      });
      autoImportInitializedRef.current = true;
      if (importedAny) {
        await loadProviders(true);
        if (probeRunIdRef.current !== runId) return;
        setMessage({ type: 'success', text: t('systemConfigImported') });
        setTimeout(() => setMessage({ type: '', text: '' }), 3000);
      }
    }
  }

  useEffect(() => {
    if (activeTool === 'claude') {
      void loadClaudeProfiles();
    }
  }, [activeTool, state.providers]);

  useEffect(() => {
    if (!isTauri) return;
    (async () => {
      try {
        const cfg = await getAiSessionStorageConfig();
        if (cfg.ai_model_launch_commands?.claude) {
          setClaudeLaunchCommand(cfg.ai_model_launch_commands.claude);
        }
      } catch (e) {
        console.error('Failed to load Claude launch command:', e);
      }
    })();
  }, [isTauri]);

  useEffect(() => {
    isVisibleRef.current = isVisible;
  }, [isVisible]);

  useEffect(() => {
    if (!isVisible || !isTauri) return;
    void loadProviders(true);
    const versionRunId = ++versionCheckRunIdRef.current;
    const probeRunId = ++probeRunIdRef.current;
    const runCheck = () => {
      void detectAllVersions(versionRunId);
      void preloadCliMetaAndAutoImport(probeRunId);
    };
    const idleCallback = (window as Window & {
      requestIdleCallback?: (callback: () => void, options?: { timeout: number }) => number;
      cancelIdleCallback?: (id: number) => void;
    }).requestIdleCallback;
    const cancelIdleCallback = (window as Window & {
      cancelIdleCallback?: (id: number) => void;
    }).cancelIdleCallback;
    if (idleCallback) {
      const id = idleCallback(runCheck, { timeout: 2500 });
      return () => {
        versionCheckRunIdRef.current += 1;
        probeRunIdRef.current += 1;
        if (cancelIdleCallback) cancelIdleCallback(id);
      };
    }
    const timer = window.setTimeout(runCheck, 600);
    return () => {
      versionCheckRunIdRef.current += 1;
      probeRunIdRef.current += 1;
      window.clearTimeout(timer);
    };
  }, [isVisible]);

  useEffect(() => {
    const current = state.providers.find(p => p.id === currentProviderId);
    if (!current || current.tool !== activeTool) {
      setCurrentProviderId(null);
    }
  }, [activeTool]);

  return {
    loadProviders,
    loadClaudeProfiles,
    detectAllVersions,
    preloadCliMetaAndAutoImport,
  };
}

export type AiEnvironmentDataBindings = ReturnType<typeof useAiEnvironmentData>;
