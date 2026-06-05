import { RefreshCw, Loader2, CheckCircle2, AlertTriangle, ArrowUpCircle, CircleOff, ShieldAlert, TerminalSquare } from 'lucide-react';
import { useCallback } from 'react';
import { ClaudeIcon, OpenAIIcon, GeminiIcon, OpenCodeIcon } from './icons';

type CliTool = 'claude' | 'codex' | 'gemini' | 'opencode';
type CliVersionState = { version: string; isInstalled: boolean };
type CliUpdateInfo = {
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

const TOOLS: readonly CliTool[] = ['claude', 'codex', 'gemini', 'opencode'];
const TOOL_LABELS: Record<CliTool, string> = {
  claude: 'Claude',
  codex: 'Codex',
  gemini: 'Gemini',
  opencode: 'OpenCode',
};

interface CliVersionCardsProps {
  cliVersions: Partial<Record<CliTool, CliVersionState>>;
  activeTool: string;
  checkingVersions: Partial<Record<CliTool, boolean>>;
  cliUpdates: Partial<Record<CliTool, CliUpdateInfo>>;
  checkingAllVersions: boolean;
  probingTool: Partial<Record<CliTool, boolean>>;
  checkingUpdates: Partial<Record<CliTool, boolean>>;
  updatingTool: Partial<Record<CliTool, boolean>>;
  stateProviders: Array<{ tool: string; id: string }>;
  providerCounts: Record<CliTool, number>;
  unsavedNewProviderIds: Set<string>;
  setActiveTool: (tool: string) => void;
  setCurrentProviderId: (id: string | null) => void;
  detectAllVersions: (runId: number) => Promise<void>;
  preloadCliMetaAndAutoImport: (runId: number) => Promise<void>;
  handleApplyCliUpdate: (tool: CliTool) => Promise<void>;
  getManagedStateForTool: (tool: CliTool) => 'enabled' | 'disabled' | 'unsupported';
  t: (key: string, options?: any) => string;
  versionCheckRunIdRef: React.MutableRefObject<number>;
  probeRunIdRef: React.MutableRefObject<number>;
  cliProbeInitializedRef: React.MutableRefObject<boolean>;
}

export function CliVersionCards({
  cliVersions,
  activeTool,
  checkingVersions,
  cliUpdates,
  checkingAllVersions,
  probingTool,
  checkingUpdates,
  updatingTool,
  stateProviders,
  providerCounts,
  unsavedNewProviderIds,
  setActiveTool,
  setCurrentProviderId,
  detectAllVersions,
  preloadCliMetaAndAutoImport,
  handleApplyCliUpdate,
  getManagedStateForTool,
  t,
  versionCheckRunIdRef,
  probeRunIdRef,
  cliProbeInitializedRef,
}: CliVersionCardsProps) {
  const handleRefreshAll = useCallback(() => {
    const versionRunId = ++versionCheckRunIdRef.current;
    const probeRunId = ++probeRunIdRef.current;
    cliProbeInitializedRef.current = false;
    void detectAllVersions(versionRunId);
    void preloadCliMetaAndAutoImport(probeRunId);
  }, [detectAllVersions, preloadCliMetaAndAutoImport, versionCheckRunIdRef, probeRunIdRef, cliProbeInitializedRef]);

  const isRefreshing = checkingAllVersions || Object.keys(probingTool).length > 0;

  return (
    <div className="border rounded-xl bg-card p-3 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold tracking-wide uppercase text-muted-foreground">
          {t('cliVersion')}
        </h3>
        <button
          onClick={handleRefreshAll}
          disabled={isRefreshing}
          className="p-2 hover:bg-secondary rounded-md transition-colors disabled:opacity-50"
          title={t('checkVersion')}
        >
          <RefreshCw className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} />
        </button>
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {TOOLS.map(tool => {
          const versionInfo = cliVersions[tool];
          const hasVersionResult = typeof versionInfo !== 'undefined';
          const isChecking = !!checkingVersions[tool] || !hasVersionResult;
          const isInstalled = versionInfo?.isInstalled;
          const updateInfo = cliUpdates[tool];
          const isCheckingUpdate = !!checkingUpdates[tool];
          const isUpdating = !!updatingTool[tool];
          const hasUpdate = updateInfo?.update_available === true;
          const toolEnvManagedState = getManagedStateForTool(tool);
          const opencodeConfiguredCount = tool === 'opencode'
            ? stateProviders.filter(p => p.tool === 'opencode' && !unsavedNewProviderIds.has(p.id)).length
            : 0;
          const opencodeConfigured = tool === 'opencode' && opencodeConfiguredCount > 0;
          return (
            <button
              key={tool}
              type="button"
              onClick={() => {
                setActiveTool(tool);
                setCurrentProviderId(null);
              }}
              className={`rounded-lg border px-4 py-3 text-left transition-colors ${
                activeTool === tool ? 'border-primary bg-primary/5' : 'hover:bg-muted/40'
              }`}
            >
              <div className="flex items-center gap-2">
                {(() => {
                  switch (tool.toLowerCase()) {
                    case 'claude': return <ClaudeIcon className="w-5 h-5" />;
                    case 'codex': return <OpenAIIcon className="w-5 h-5" />;
                    case 'gemini': return <GeminiIcon className="w-5 h-5" />;
                    case 'opencode': return <OpenCodeIcon className="w-5 h-5" />;
                    default: return <TerminalSquare className="w-5 h-5" />;
                  }
                })()}
                <span className="text-sm font-semibold">
                  {TOOL_LABELS[tool]}（{providerCounts[tool] ?? 0}）
                </span>
              </div>
              <div className="mt-2.5 flex items-center gap-2">
                {isChecking ? (
                  <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
                ) : isInstalled ? (
                  <CheckCircle2 className="w-4 h-4 text-green-600" />
                ) : (
                  <AlertTriangle className="w-4 h-4 text-amber-600" />
                )}
                <span className={`text-sm leading-none ${isChecking ? 'text-muted-foreground' : isInstalled ? 'text-foreground' : 'text-amber-600'}`}>
                  {isChecking ? t('checking', 'Checking...') : isInstalled ? `v${versionInfo?.version}` : t('notInstalled')}
                </span>
                {!isChecking && isInstalled && (
                  isCheckingUpdate ? (
                    <Loader2 className="w-3 h-3 animate-spin text-muted-foreground" />
                  ) : hasUpdate ? (
                    <button
                      type="button"
                      onClick={(e) => { e.stopPropagation(); void handleApplyCliUpdate(tool); }}
                      disabled={isUpdating}
                      className="ml-auto p-1 rounded-md hover:bg-amber-100 transition-colors disabled:opacity-50"
                      title={t('cliUpdate')}
                    >
                      {isUpdating ? <Loader2 className="w-3.5 h-3.5 animate-spin text-amber-600" /> : <ArrowUpCircle className="w-3.5 h-3.5 text-amber-600" />}
                    </button>
                  ) : null
                )}
              </div>
              {!isChecking && isInstalled && updateInfo?.latest_version && (
                <div className="mt-1 flex items-center gap-1.5">
                  <span className="text-xs text-muted-foreground">
                    {t('cliLatestVersion')}: v{updateInfo.latest_version}
                  </span>
                  {hasUpdate ? (
                    <span className="text-xs text-amber-600 font-medium">{t('cliUpdateAvailable')}</span>
                  ) : updateInfo.compare_status === 'current' ? (
                    <span className="text-xs text-green-600">{t('cliUpToDate')}</span>
                  ) : null}
                </div>
              )}
              <div className="mt-2.5 flex items-center gap-2">
                {tool === 'opencode' ? (
                  opencodeConfigured ? (
                    <CheckCircle2 className="w-4 h-4 text-green-600" />
                  ) : (
                    <ShieldAlert className="w-4 h-4 text-amber-600" />
                  )
                ) : toolEnvManagedState === 'enabled' ? (
                  <CheckCircle2 className="w-4 h-4 text-green-600" />
                ) : toolEnvManagedState === 'disabled' ? (
                  <ShieldAlert className="w-4 h-4 text-amber-600" />
                ) : (
                  <CircleOff className="w-4 h-4 text-muted-foreground" />
                )}
                <span
                  className={`text-xs leading-none ${
                    tool === 'opencode'
                      ? (opencodeConfigured ? 'text-green-700' : 'text-amber-700')
                      : toolEnvManagedState === 'enabled'
                      ? 'text-green-700'
                      : toolEnvManagedState === 'disabled'
                        ? 'text-amber-700'
                        : 'text-muted-foreground'
                  }`}
                >
                  {tool === 'opencode'
                    ? (opencodeConfigured
                        ? t('opencodeProvidersConfiguredStatus', {
                            count: opencodeConfiguredCount,
                            defaultValue: 'Configured {{count}} providers'
                          })
                        : t('opencodeProvidersNotConfiguredStatus', 'No providers configured'))
                    : toolEnvManagedState === 'enabled'
                      ? t('envManagedStatusEnabled', 'Enabled')
                      : toolEnvManagedState === 'disabled'
                        ? t('envManagedStatusDisabled', 'Disabled')
                        : t('envManagedStatusUnsupported', 'Unsupported')}
                </span>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
