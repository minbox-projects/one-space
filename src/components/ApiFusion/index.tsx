import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { KeyRound, Network, Server, TerminalSquare } from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { errorToMessage } from "@/lib/messages";
import {
  apiFusionConfigureTerminal,
  apiFusionDeleteKey,
  apiFusionDeleteProvider,
  apiFusionGetConfig,
  apiFusionReenableProvider,
  apiFusionSetDefaultKey,
  apiFusionSetProviderEnabled,
  apiFusionStart,
  apiFusionStatus,
  apiFusionStop,
  apiFusionSyncTerminal,
  apiFusionTerminalTargets,
  apiFusionUpsertKey,
  apiFusionUpsertProvider,
  localBaseUrl,
  resolveDefaultKeyId,
  type FusionConfig,
  type FusionKey,
  type FusionStatus,
  type FusionTerminalTarget,
  type FusionUpstreamProvider,
} from "@/lib/apiFusion";
import { RuntimeStatusCard } from "./RuntimeStatusCard";
import { UpstreamProviderList } from "./UpstreamProviderList";
import { ProviderDetailDialog } from "./ProviderDetailDialog";
import { LocalKeyList } from "./LocalKeyList";
import { TerminalSyncPanel } from "./TerminalSyncPanel";

type ApiFusionTab = "providers" | "keys" | "terminals";

function emptyProvider(): FusionUpstreamProvider {
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

export function ApiFusion({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const ToolIcon = Network;
  const iconClassName = "bg-indigo-500/10 text-indigo-600";

  const [activeTab, setActiveTab] = useState<ApiFusionTab>("providers");
  const [config, setConfig] = useState<FusionConfig | null>(null);
  const [status, setStatus] = useState<FusionStatus | null>(null);
  const [targets, setTargets] = useState<FusionTerminalTarget[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [editingProvider, setEditingProvider] =
    useState<FusionUpstreamProvider | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
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
        local_base_url: "http://127.0.0.1:17688",
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
        apiFusionGetConfig(),
        apiFusionStatus(),
        apiFusionTerminalTargets(),
      ]);
      setConfig(nextConfig);
      setStatus(nextStatus);
      setTargets(nextTargets ?? []);
    } catch (err) {
      const msg = errorToMessage(err);
      setLoadError(msg);
      pushToast({
        title: t("apiFusionLoadFailed", "Failed to load API Gateway configuration."),
        description: msg,
        kind: "error",
      });
    }
  }, [isTauri, pushToast, t]);

  useEffect(() => {
    if (!isVisible) return;
    void load();
  }, [isVisible, load]);

  const applyConfig = useCallback(async (next: FusionConfig) => {
    setConfig(next);
    const [nextStatus, nextTargets] = await Promise.all([
      apiFusionStatus(),
      apiFusionTerminalTargets(),
    ]);
    setStatus(nextStatus);
    setTargets(nextTargets);
  }, []);

  const runAction = useCallback(
    async (action: () => Promise<void>, successTitle: string) => {
      setBusy(true);
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
      } catch (err) {
        pushToast({
          title: t("apiFusionActionFailed", "Action failed"),
          description: errorToMessage(err),
          kind: "error",
        });
      } finally {
        setBusy(false);
      }
    },
    [pushToast, t],
  );

  const handleToggleService = () =>
    runAction(async () => {
      const running = Boolean(status?.running);
      const nextStatus = running ? await apiFusionStop() : await apiFusionStart();
      setStatus(nextStatus);
      setConfig(await apiFusionGetConfig());
    }, t("apiFusionSaved", "Saved."));

  const handleToggleProviderEnabled = (provider: FusionUpstreamProvider, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await apiFusionSetProviderEnabled(provider.id, enabled));
    }, t("apiFusionSaved", "Saved."));

  const handleReenableProvider = (providerId: string) =>
    runAction(async () => {
      await applyConfig(await apiFusionReenableProvider(providerId));
    }, t("apiFusionSaved", "Saved."));

  const handleSaveProvider = (draft: FusionUpstreamProvider) =>
    runAction(async () => {
      const next = await apiFusionUpsertProvider(draft);
      setConfig(next);
      const saved = draft.id
        ? next.providers.find((provider) => provider.id === draft.id) ?? null
        : null;
      setEditingProvider(saved);
      setSelectedProviderId(saved?.id ?? null);
      const [nextStatus, nextTargets] = await Promise.all([
        apiFusionStatus(),
        apiFusionTerminalTargets(),
      ]);
      setStatus(nextStatus);
      setTargets(nextTargets);
    }, t("apiFusionProviderSaved", "Provider saved."));

  const handleDeleteProvider = (providerId: string) =>
    runAction(async () => {
      await applyConfig(await apiFusionDeleteProvider(providerId));
      setEditingProvider(null);
      setSelectedProviderId(null);
      setIsDialogOpen(false);
    }, t("apiFusionDeleted", "Deleted."));

  const handleSaveKey = (key: FusionKey) =>
    runAction(async () => {
      await applyConfig(await apiFusionUpsertKey(key));
    }, t("apiFusionKeySaved", "Key saved."));

  const handleDeleteKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await apiFusionDeleteKey(keyId));
    }, t("apiFusionDeleted", "Deleted."));

  const handleToggleKeyEnabled = (key: FusionKey, enabled: boolean) =>
    runAction(async () => {
      await applyConfig(await apiFusionUpsertKey({ ...key, enabled }));
    }, t("apiFusionSaved", "Saved."));

  const handleSetDefaultKey = (keyId: string) =>
    runAction(async () => {
      await applyConfig(await apiFusionSetDefaultKey(keyId));
    }, t("apiFusionSaved", "Saved."));

  const runTerminalAction = useCallback(
    async (tool: string, action: () => Promise<void>, successTitle: string) => {
      setSyncingTools((prev) => ({ ...prev, [tool]: true }));
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
      } catch (err) {
        pushToast({
          title: t("apiFusionActionFailed", "Action failed"),
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
        await apiFusionConfigureTerminal([tool]);
        await applyConfig(await apiFusionGetConfig());
      },
      t("apiFusionConfigureSuccess", "Terminal targets configured."),
    );

  const handleSyncTool = (tool: string) =>
    void runTerminalAction(
      tool,
      async () => {
        await apiFusionSyncTerminal([tool]);
        await applyConfig(await apiFusionGetConfig());
      },
      t("apiFusionSyncSuccess", "Terminal targets synced."),
    );

  const handleCopyAddress = async () => {
    if (!config) return;
    try {
      await navigator.clipboard.writeText(localBaseUrl(config.port));
      setAddressCopied(true);
      pushToast({ title: t("apiFusionCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("apiFusionCopyFailed", "Copy failed"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const handleCopyKey = async (key: FusionKey) => {
    try {
      await navigator.clipboard.writeText(key.value);
      setCopiedKeyId(key.id);
      pushToast({ title: t("apiFusionCopied", "Copied to clipboard"), kind: "success" });
    } catch (err) {
      pushToast({
        title: t("apiFusionCopyFailed", "Copy failed"),
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
                {t("apiFusionLoadFailed", "Failed to load API Gateway configuration.")}
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
    id: ApiFusionTab;
    label: string;
    icon: typeof Server;
    count?: number;
    hasAlert?: boolean;
  }> = [
    {
      id: "providers",
      label: t("apiFusionProviders", "Upstream providers"),
      icon: Server,
      count: config?.providers?.length ?? 0,
      hasAlert: autoDisabledCount > 0,
    },
    {
      id: "keys",
      label: t("apiFusionKeys", "Api Keys"),
      icon: KeyRound,
      count: config?.keys?.length ?? 0,
    },
    {
      id: "terminals",
      label: t("apiFusionTerminalSync", "Terminal sync"),
      icon: TerminalSquare,
      hasAlert: pendingSyncCount > 0,
    },
  ];

  return (
    <div className="h-full overflow-y-auto" data-testid="api-fusion-console">
      <div className="mx-auto max-w-7xl space-y-4 p-6">
        {/* 头部标题与简介（对齐 AiEnvironments 规范） */}
        <header className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div className="space-y-0.5">
            <h1 className="text-xl font-bold tracking-tight text-foreground">
              {t("apiFusion", "API Gateway")}
            </h1>
            <p className="max-w-3xl text-xs text-muted-foreground">
              {t(
                "apiFusionWorkspaceDesc",
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
          onStart={handleToggleService}
          onStop={handleToggleService}
          onCopyAddress={() => void handleCopyAddress()}
        />

        {/* 工作区 Tabs 标签页导航（对齐 AiEnvironments 的紧凑导航规范） */}
        <div
          role="tablist"
          aria-label={t("apiFusionWorkspaceTabs", "API Gateway tabs")}
          className="flex flex-wrap items-center gap-1 rounded-lg border bg-muted/40 p-1"
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
                className={`inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-xs font-medium transition-all ${
                  isActive
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-background/50 hover:text-foreground"
                }`}
              >
                <Icon className={`h-3.5 w-3.5 ${isActive ? "text-indigo-600" : ""}`} />
                <span>{tab.label}</span>
                {tab.count !== undefined ? (
                  <span
                    className={`rounded-full px-1.5 py-0.2 text-[10px] font-semibold ${
                      isActive
                        ? "bg-muted text-foreground"
                        : "bg-muted/70 text-muted-foreground"
                    }`}
                  >
                    {tab.count}
                  </span>
                ) : null}
                {tab.hasAlert ? (
                  <span
                    className="h-1.5 w-1.5 rounded-full bg-amber-500"
                    title={t("apiFusionHasPendingItems", "Has items needing attention")}
                  />
                ) : null}
              </button>
            );
          })}
        </div>

        {/* Tab 1: 上游服务商 */}
        <div
          role="tabpanel"
          aria-label={t("apiFusionProviders", "Upstream providers")}
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
          aria-label={t("apiFusionKeys", "Api Keys")}
          className={activeTab === "keys" ? "block" : "hidden"}
        >
          <LocalKeyList
            keys={config.keys}
            defaultKeyId={defaultKeyId}
            busy={busy}
            copiedKeyId={copiedKeyId}
            onSave={(key) => void handleSaveKey(key)}
            onDelete={(keyId) => void handleDeleteKey(keyId)}
            onSetDefault={(keyId) => void handleSetDefaultKey(keyId)}
            onToggleEnabled={(key, enabled) => void handleToggleKeyEnabled(key, enabled)}
            onCopy={(key) => void handleCopyKey(key)}
          />
        </div>

        {/* Tab 3: 终端同步 */}
        <div
          role="tabpanel"
          aria-label={t("apiFusionTerminalSync", "Terminal sync")}
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
      </div>
    </div>
  );
}
