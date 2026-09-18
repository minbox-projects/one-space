import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { emit } from "@tauri-apps/api/event";
import {
  BarChart3,
  KeyRound,
  Network,
  ScrollText,
  Server,
  TerminalSquare,
} from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { errorToMessage } from "@/lib/messages";
import {
  API_GATEWAY_STATUS_UPDATED_EVENT,
  apiGatewayConfigureTerminal,
  apiGatewayDeleteKey,
  apiGatewayDeleteProvider,
  apiGatewayGetConfig,
  apiGatewayReenableProvider,
  apiGatewaySetDefaultKey,
  apiGatewaySetProviderEnabled,
  apiGatewayStart,
  apiGatewayStatus,
  apiGatewayStop,
  apiGatewaySyncTerminal,
  apiGatewayTerminalTargets,
  apiGatewayUpsertKey,
  apiGatewayUpsertProvider,
  localBaseUrl,
  resolveDefaultKeyId,
  type GatewayConfig,
  type GatewayKey,
  type GatewayStatus,
  type GatewayTerminalTarget,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";
import { RuntimeStatusCard } from "./RuntimeStatusCard";
import { UpstreamProviderList } from "./UpstreamProviderList";
import { ProviderDetailDialog } from "./ProviderDetailDialog";
import { AggregatedModelsDialog } from "./AggregatedModelsDialog";
import { LocalKeyDialog } from "./LocalKeyDialog";
import { LocalKeyList } from "./LocalKeyList";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import { UsageStatsPanel } from "./UsageStatsPanel";
import { UsageLogsPanel } from "./UsageLogsPanel";

type ApiGatewayTab = "providers" | "keys" | "terminals" | "usage" | "logs";

function emptyProvider(): GatewayUpstreamProvider {
  return {
    id: "",
    name: "",
    base_url: "",
    api_key: "",
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
  };
}

export function ApiGateway({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const ToolIcon = Network;
  const iconClassName = "bg-primary/10 text-primary";

  const [activeTab, setActiveTab] = useState<ApiGatewayTab>("providers");
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [targets, setTargets] = useState<GatewayTerminalTarget[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [editingProvider, setEditingProvider] =
    useState<GatewayUpstreamProvider | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [isKeyDialogOpen, setIsKeyDialogOpen] = useState(false);
  const [isModelsDialogOpen, setIsModelsDialogOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [syncingTools, setSyncingTools] = useState<Record<string, boolean>>({});
  const [addressCopied, setAddressCopied] = useState(false);
  const [copiedKeyId, setCopiedKeyId] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const isTauri = "__TAURI_INTERNALS__" in window;

  const load = useCallback(async () => {
    if (!isTauri) {
      // 在非 Tauri 浏览器预览环境下提供默认初始配置，避免页面永久 Loading
      setConfig({
        enabled: false,
        port: 17688,
        providers: [],
        keys: [],
        default_key_id: null,
        terminal_syncs: [],
      });
      setStatus({
        running: false,
        enabled: false,
        port: 17688,
        local_base_url: "http://127.0.0.1:17688/v1",
        provider_count: 0,
        auto_disabled_count: 0,
        key_count: 0,
        default_key_id: null,
      });
      setTargets([]);
      setLoadError(null);
      return;
    }

    setLoadError(null);
    try {
      const [nextConfig, nextStatus, nextTargets] = await Promise.all([
        apiGatewayGetConfig(),
        apiGatewayStatus(),
        apiGatewayTerminalTargets(),
      ]);
      setConfig(nextConfig);
      setStatus(nextStatus);
      setTargets(nextTargets ?? []);
    } catch (err) {
      const msg = errorToMessage(err);
      setLoadError(msg);
      pushToast({
        title: t("apiGatewayLoadFailed", "Failed to load API Gateway configuration."),
        description: msg,
        kind: "error",
      });
    }
  }, [isTauri, pushToast, t]);

  useEffect(() => {
    if (!isVisible) return;
    void load();
  }, [isVisible, load]);

  const applyConfig = useCallback(async (next: GatewayConfig) => {
    setConfig(next);
    const [nextStatus, nextTargets] = await Promise.all([
      apiGatewayStatus(),
      apiGatewayTerminalTargets(),
    ]);
    setStatus(nextStatus);
    setTargets(nextTargets);
  }, []);

  const runAction = useCallback(
    async (
      action: () => Promise<void>,
      successTitle: string,
    ): Promise<boolean> => {
      setBusy(true);
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
        return true;
      } catch (err) {
        pushToast({
          title: t("apiGatewayActionFailed", "Action failed"),
          description: errorToMessage(err),
          kind: "error",
        });
        return false;
      } finally {
        setBusy(false);
      }
    },
    [pushToast, t],
  );

  const handleToggleService = () =>
    runAction(async () => {
      const running = Boolean(status?.running);
      const nextStatus = running ? await apiGatewayStop() : await apiGatewayStart();
      setStatus(nextStatus);
      setConfig(await apiGatewayGetConfig());
      await emit(API_GATEWAY_STATUS_UPDATED_EVENT).catch(() => {});
    }, t("apiGatewaySaved", "Saved."));

  const handleToggleProviderEnabled = (provider: GatewayUpstreamProvider, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await apiGatewaySetProviderEnabled(provider.id, enabled));
    }, t("apiGatewaySaved", "Saved."));

  const handleReenableProvider = (providerId: string) =>
    runAction(async () => {
      await applyConfig(await apiGatewayReenableProvider(providerId));
    }, t("apiGatewaySaved", "Saved."));

  const handleSaveProvider = (draft: GatewayUpstreamProvider) =>
    runAction(async () => {
      const next = await apiGatewayUpsertProvider(draft);
      setConfig(next);
      const saved = draft.id
        ? next.providers.find((provider) => provider.id === draft.id) ?? null
        : null;
      setEditingProvider(saved);
      setSelectedProviderId(saved?.id ?? null);
      const [nextStatus, nextTargets] = await Promise.all([
        apiGatewayStatus(),
        apiGatewayTerminalTargets(),
      ]);
      setStatus(nextStatus);
      setTargets(nextTargets);
    }, t("apiGatewayProviderSaved", "Provider saved."));

  const handleDeleteProvider = (providerId: string) =>
    runAction(async () => {
      await applyConfig(await apiGatewayDeleteProvider(providerId));
      setEditingProvider(null);
      setSelectedProviderId(null);
      setIsDialogOpen(false);
    }, t("apiGatewayDeleted", "Deleted."));

  const handleSaveKey = (key: GatewayKey): Promise<boolean> =>
    runAction(async () => {
      await applyConfig(await apiGatewayUpsertKey(key));
    }, t("apiGatewayKeySaved", "Key saved."));

  const handleDeleteKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await apiGatewayDeleteKey(keyId));
    }, t("apiGatewayDeleted", "Deleted."));

  const handleToggleKeyEnabled = (key: GatewayKey, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await apiGatewayUpsertKey({ ...key, enabled }));
    }, t("apiGatewaySaved", "Saved."));

  const handleSetDefaultKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await apiGatewaySetDefaultKey(keyId));
    }, t("apiGatewaySaved", "Saved."));

  const runTerminalAction = useCallback(
    async (tool: string, action: () => Promise<void>, successTitle: string) => {
      setSyncingTools((prev) => ({ ...prev, [tool]: true }));
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
      } catch (err) {
        pushToast({
          title: t("apiGatewayActionFailed", "Action failed"),
          description: errorToMessage(err),
          kind: "error",
        });
      } finally {
        setSyncingTools((prev) => {
          const next = { ...prev };
          delete next[tool];
          return next;
        });
      }
    },
    [pushToast, t],
  );

  const handleConfigureTool = (tool: string) =>
    void runTerminalAction(
      tool,
      async () => {
        await apiGatewayConfigureTerminal([tool]);
        await applyConfig(await apiGatewayGetConfig());
      },
      t("apiGatewayConfigureSuccess", "Terminal targets configured."),
    );

  const handleSyncTool = (tool: string) =>
    void runTerminalAction(
      tool,
      async () => {
        await apiGatewaySyncTerminal([tool]);
        await applyConfig(await apiGatewayGetConfig());
      },
      t("apiGatewaySyncSuccess", "Terminal targets synced."),
    );

  const handleCopyAddress = async () => {
    if (!config) return;
    try {
      await navigator.clipboard.writeText(localBaseUrl(config.port));
      setAddressCopied(true);
      pushToast({ title: t("apiGatewayCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("apiGatewayCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const handleCopyKey = async (key: GatewayKey) => {
    try {
      await navigator.clipboard.writeText(key.value);
      setCopiedKeyId(key.id);
      pushToast({ title: t("apiGatewayCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("apiGatewayCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const defaultKeyId = config
    ? resolveDefaultKeyId(config.keys, config.default_key_id)
    : null;

  if (!config) {
    return (
      <div className="h-full overflow-y-auto">
        <div className="mx-auto max-w-7xl p-6">
          {loadError ? (
            <div className="flex flex-col items-center justify-center rounded-2xl border border-destructive/20 bg-destructive/5 p-8 text-center">
              <div className="rounded-full bg-destructive/10 p-3 text-destructive">
                <Network className="h-6 w-6" />
              </div>
              <h3 className="mt-3 text-base font-semibold text-foreground">
                {t("apiGatewayLoadFailed", "Failed to load API Gateway configuration.")}
              </h3>
              <p className="mt-1 max-w-md text-xs text-muted-foreground">{loadError}</p>
              <button
                type="button"
                onClick={() => void load()}
                className="mt-4 inline-flex items-center rounded-lg bg-primary px-4 py-2 text-xs font-medium text-primary-foreground shadow-sm hover:bg-primary/90"
              >
                {t("retry", "Retry")}
              </button>
            </div>
          ) : (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <span className="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent" />
              <span>{t("loading", "Loading...")}</span>
            </div>
          )}
        </div>
      </div>
    );
  }

  const pendingSyncCount = (targets ?? []).filter(
    (target) => target.pending_sync,
  ).length;
  const autoDisabledCount = status?.auto_disabled_count ?? 0;

  const tabs: Array<{
    id: ApiGatewayTab;
    label: string;
    icon: typeof Server;
    count?: number;
    hasAlert?: boolean;
  }> = [
    {
      id: "providers",
      label: t("apiGatewayProviders", "Upstream providers"),
      icon: Server,
      count: config?.providers?.length ?? 0,
      hasAlert: autoDisabledCount > 0,
    },
    {
      id: "keys",
      label: t("apiGatewayKeys", "Api Keys"),
      icon: KeyRound,
      count: config?.keys?.length ?? 0,
    },
    {
      id: "terminals",
      label: t("apiGatewayTerminalSync", "AI terminal integration"),
      icon: TerminalSquare,
      hasAlert: pendingSyncCount > 0,
    },
    {
      id: "usage",
      label: t("apiGatewayUsageTab", "Usage"),
      icon: BarChart3,
    },
    {
      id: "logs",
      label: t("apiGatewayLogsTab", "Request logs"),
      icon: ScrollText,
    },
  ];

  return (
    <div className="h-full overflow-y-auto" data-testid="api-gateway-console">
      <div className="mx-auto max-w-7xl space-y-4 p-6">
        {/* 头部标题与简介（对齐 AiEnvironments 规范） */}
        <header className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div className="space-y-0.5">
            <h1 className="text-xl font-bold tracking-tight text-foreground">
              {t("apiGateway", "API Gateway")}
            </h1>
            <p className="max-w-3xl text-xs text-muted-foreground">
              {t(
                "apiGatewayWorkspaceDesc",
                "Run a local OpenAI-compatible relay across multiple upstream providers, manage local keys, and push the local endpoint to OpenCode / Codex.",
              )}
            </p>
          </div>
        </header>

        {/* 常驻运行时服务状态卡片 */}
        <RuntimeStatusCard
          status={status}
          config={config}
          busy={busy}
          addressCopied={addressCopied}
          targets={targets}
          onSelectTab={setActiveTab}
          onShowModels={() => setIsModelsDialogOpen(true)}
          onStart={handleToggleService}
          onStop={handleToggleService}
          onCopyAddress={() => void handleCopyAddress()}
        />

        {/* 工作区 Tabs 标签页导航 */}
        <div
          role="tablist"
          aria-label={t("apiGatewayWorkspaceTabs", "API Gateway tabs")}
          className="flex flex-wrap items-center gap-1.5 rounded-xl border bg-muted/50 p-1.5 shadow-xs"
        >
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={isActive}
                onClick={() => setActiveTab(tab.id)}
                className={`group inline-flex h-9 items-center gap-2 rounded-lg px-3.5 text-xs font-medium transition-all ${
                  isActive
                    ? "bg-background text-foreground font-semibold shadow-xs ring-1 ring-border/80 dark:ring-border"
                    : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
                }`}
              >
                <Icon
                  className={`h-4 w-4 transition-colors ${
                    isActive
                      ? "text-primary"
                      : "text-muted-foreground/70 group-hover:text-foreground"
                  }`}
                />
                <span>{tab.label}</span>
                {tab.count !== undefined ? (
                  <span
                    className={`rounded-full px-2 py-0.5 text-[11px] font-semibold transition-colors ${
                      isActive
                        ? "bg-primary/10 text-primary"
                        : "bg-muted text-muted-foreground group-hover:bg-muted/80 group-hover:text-foreground"
                    }`}
                  >
                    {tab.count}
                  </span>
                ) : null}
                {tab.hasAlert ? (
                  <span
                    className="relative flex h-2 w-2"
                    title={t("apiGatewayHasPendingItems", "Has items needing attention")}
                  >
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
                    <span className="relative inline-flex h-2 w-2 rounded-full bg-amber-500" />
                  </span>
                ) : null}
              </button>
            );
          })}
        </div>

        {/* Tab 1: 上游服务商 */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayProviders", "Upstream providers")}
          className={activeTab === "providers" ? "block" : "hidden"}
        >
          <UpstreamProviderList
            providers={config.providers}
            selectedProviderId={selectedProviderId}
            busy={busy}
            onSelect={(providerId) => {
              setSelectedProviderId(providerId);
              const found =
                config.providers.find((provider) => provider.id === providerId) ?? null;
              setEditingProvider(found);
              setIsDialogOpen(true);
            }}
            onToggleEnabled={(provider, enabled) =>
              void handleToggleProviderEnabled(provider, enabled)
            }
            onReenable={(providerId) => void handleReenableProvider(providerId)}
            onAdd={() => {
              setSelectedProviderId(null);
              setEditingProvider(emptyProvider());
              setIsDialogOpen(true);
            }}
            onDelete={(providerId) => void handleDeleteProvider(providerId)}
          />
        </div>

        {/* Tab 2: 本地密钥 */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayKeys", "Api Keys")}
          className={activeTab === "keys" ? "block" : "hidden"}
        >
          <LocalKeyList
            keys={config.keys}
            defaultKeyId={defaultKeyId}
            busy={busy}
            copiedKeyId={copiedKeyId}
            onAdd={() => setIsKeyDialogOpen(true)}
            onDelete={(keyId) => void handleDeleteKey(keyId)}
            onSetDefault={(keyId) => void handleSetDefaultKey(keyId)}
            onToggleEnabled={(key, enabled) => void handleToggleKeyEnabled(key, enabled)}
            onCopy={(key) => void handleCopyKey(key)}
          />
        </div>

        {/* Tab 3: AI 终端集成 */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayTerminalSync", "AI terminal integration")}
          className={activeTab === "terminals" ? "block" : "hidden"}
        >
          <TerminalSyncPanel
            targets={targets}
            config={config}
            syncingTools={syncingTools}
            onConfigureTool={(tool) => handleConfigureTool(tool)}
            onSyncTool={(tool) => handleSyncTool(tool)}
          />
        </div>

        {/* Tab 4: 用量统计（面板常驻以保留范围等状态） */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayUsageTab", "Usage")}
          className={activeTab === "usage" ? "block" : "hidden"}
        >
          <UsageStatsPanel isActive={activeTab === "usage"} />
        </div>

        {/* Tab 5: 请求日志（面板常驻以保留分组/页码等状态） */}
        <div
          role="tabpanel"
          aria-label={t("apiGatewayLogsTab", "Request logs")}
          className={activeTab === "logs" ? "block" : "hidden"}
        >
          <UsageLogsPanel isActive={activeTab === "logs"} />
        </div>

        {/* 服务商新增与编辑模态弹窗 */}
        <ProviderDetailDialog
          open={isDialogOpen}
          onOpenChange={(open) => {
            setIsDialogOpen(open);
            if (!open) {
              setSelectedProviderId(null);
              setEditingProvider(null);
            }
          }}
          provider={editingProvider}
          busy={busy}
          onSave={(draft) => void handleSaveProvider(draft)}
          onDelete={(providerId) => void handleDeleteProvider(providerId)}
        />

        {/* 本地密钥新增模态弹窗 */}
        <LocalKeyDialog
          open={isKeyDialogOpen}
          onOpenChange={setIsKeyDialogOpen}
          busy={busy}
          onSave={handleSaveKey}
        />

        {/* 聚合模型列表模态弹窗 */}
        <AggregatedModelsDialog
          open={isModelsDialogOpen}
          onOpenChange={setIsModelsDialogOpen}
          providers={config.providers}
        />
      </div>
    </div>
  );
}
