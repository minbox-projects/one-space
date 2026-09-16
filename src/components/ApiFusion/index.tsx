import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Network } from "lucide-react";
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
import { UpstreamProviderDetail } from "./UpstreamProviderDetail";
import { LocalKeyList } from "./LocalKeyList";
import { TerminalSyncPanel } from "./TerminalSyncPanel";

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

  const [config, setConfig] = useState<FusionConfig | null>(null);
  const [status, setStatus] = useState<FusionStatus | null>(null);
  const [targets, setTargets] = useState<FusionTerminalTarget[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [editingProvider, setEditingProvider] =
    useState<FusionUpstreamProvider | null>(null);
  const [selectedTargetIds, setSelectedTargetIds] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [addressCopied, setAddressCopied] = useState(false);
  const [copiedKeyId, setCopiedKeyId] = useState<string | null>(null);

  const isTauri = "__TAURI_INTERNALS__" in window;

  const load = useCallback(async () => {
    if (!isTauri) return;
    try {
      const [nextConfig, nextStatus, nextTargets] = await Promise.all([
        apiFusionGetConfig(),
        apiFusionStatus(),
        apiFusionTerminalTargets(),
      ]);
      setConfig(nextConfig);
      setStatus(nextStatus);
      setTargets(nextTargets);
    } catch (err) {
      pushToast({
        title: t("apiFusionLoadFailed", "Failed to load API Fusion configuration."),
        description: errorToMessage(err),
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

  const handleToggleTarget = (providerId: string) =>
    setSelectedTargetIds((prev) =>
      prev.includes(providerId)
        ? prev.filter((id) => id !== providerId)
        : [...prev, providerId],
    );

  const handleConfigureTargets = () =>
    runAction(async () => {
      await apiFusionConfigureTerminal(selectedTargetIds);
      await applyConfig(await apiFusionGetConfig());
    }, t("apiFusionConfigureSuccess", "Terminal targets configured."));

  const handleSyncTargets = () =>
    runAction(async () => {
      await apiFusionSyncTerminal(selectedTargetIds);
      await applyConfig(await apiFusionGetConfig());
    }, t("apiFusionSyncSuccess", "Terminal targets synced."));

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
        <div className="mx-auto max-w-7xl p-6 text-sm text-muted-foreground">
          {t("loading", "Loading...")}
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto" data-testid="api-fusion-console">
      <div className="mx-auto max-w-7xl space-y-6 p-6">
        <header className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div className="space-y-1">
            <h1 className="text-2xl font-semibold tracking-tight">{t("apiFusion", "API Fusion")}</h1>
            <p className="max-w-3xl text-sm text-muted-foreground">
              {t(
                "apiFusionWorkspaceDesc",
                "Run a local OpenAI-compatible relay across multiple upstream providers, manage local keys, and push the local endpoint to OpenCode / Codex.",
              )}
            </p>
          </div>
        </header>

        <RuntimeStatusCard
          status={status}
          config={config}
          busy={busy}
          addressCopied={addressCopied}
          onStart={handleToggleService}
          onStop={handleToggleService}
          onCopyAddress={() => void handleCopyAddress()}
        />

        <div className="grid gap-6 xl:grid-cols-2">
          <UpstreamProviderList
            providers={config.providers}
            selectedProviderId={selectedProviderId}
            busy={busy}
            onSelect={(providerId) => {
              setSelectedProviderId(providerId);
              setEditingProvider(
                config.providers.find((provider) => provider.id === providerId) ?? null,
              );
            }}
            onToggleEnabled={(provider, enabled) =>
              void handleToggleProviderEnabled(provider, enabled)
            }
            onReenable={(providerId) => void handleReenableProvider(providerId)}
            onAdd={() => {
              setSelectedProviderId(null);
              setEditingProvider(emptyProvider());
            }}
          />
          {editingProvider ? (
            <UpstreamProviderDetail
              key={editingProvider.id || "new"}
              provider={editingProvider}
              busy={busy}
              onSave={(draft) => void handleSaveProvider(draft)}
              onDelete={(providerId) => void handleDeleteProvider(providerId)}
            />
          ) : (
            <section className="rounded-[24px] border border-dashed bg-card p-5 text-sm text-muted-foreground">
              {t("apiFusionSelectProviderHint", "Select a provider to edit its mappings.")}
            </section>
          )}
        </div>

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

        <TerminalSyncPanel
          targets={targets}
          config={config}
          selectedTargetIds={selectedTargetIds}
          busy={busy}
          onToggleTarget={handleToggleTarget}
          onConfigure={() => void handleConfigureTargets()}
          onSync={() => void handleSyncTargets()}
        />
      </div>
    </div>
  );
}
