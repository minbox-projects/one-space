import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertTriangle,
  BarChart3,
  Check,
  ChevronLeft,
  ChevronRight,
  Clipboard,
  KeyRound,
  Loader2,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  RotateCw,
  Save,
  Search,
  Settings,
  ShieldAlert,
  Square,
  Trash2,
  Users,
  X,
} from "lucide-react";
import {
  aiRoutingGatewayAccountCreateApiKeyWithConfiguration,
  aiRoutingGatewayAccountDelete,
  aiRoutingGatewayAccountDeleteConfirmation,
  aiRoutingGatewayAccountMove,
  aiRoutingGatewayAccountUpdate,
  aiRoutingGatewayAccountsDelete,
  aiRoutingGatewayAccountsDeleteConfirmation,
  aiRoutingGatewayAccountsDisable,
  aiRoutingGatewayBootstrap,
  aiRoutingGatewayGroupCreate,
  aiRoutingGatewayGroupDelete,
  aiRoutingGatewayGroupRename,
  aiRoutingGatewayKeyCreate,
  aiRoutingGatewayKeyCopy,
  aiRoutingGatewayKeyGroupsUpdate,
  aiRoutingGatewayKeyRegenerate,
  aiRoutingGatewayKeyRevoke,
  aiRoutingGatewayKeySetEnabled,
  aiRoutingGatewayLogAttempts,
  aiRoutingGatewayLogsClear,
  aiRoutingGatewayLogsQuery,
  aiRoutingGatewayMaintenanceRun,
  aiRoutingGatewayMappingList,
  aiRoutingGatewayMappingSave,
  aiRoutingGatewayPriceSave,
  aiRoutingGatewayPricesList,
  aiRoutingGatewayQuotaList,
  aiRoutingGatewayRuntimeStart,
  aiRoutingGatewayRuntimeStop,
  aiRoutingGatewaySettingsSave,
  aiRoutingGatewayStatsHome,
  subscribeAiRoutingGatewayEvents,
  type GatewayAccount,
  type GatewayBootstrap,
  type GatewayHomepage,
  type GatewayKeyRecord,
  type GatewaySettings,
  type HomepageFilters,
  type ModelMapping,
  type PriceRecord,
  type QuotaWindow,
  type RequestAttempt,
  type RequestLog,
} from "@/lib/aiRoutingGateway";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

type TabId = "home" | "accounts" | "keys" | "logs" | "settings";
type TrendDays = 7 | 15 | 30;
type TrendMode = "tokens" | "cost";
type HomepageFilterState = {
  accountId: string;
  groupId: string;
  publicModelId: string;
};

const TABS: Array<{ id: TabId; icon: typeof Activity }> = [
  { id: "home", icon: BarChart3 },
  { id: "accounts", icon: Users },
  { id: "keys", icon: KeyRound },
  { id: "logs", icon: Activity },
  { id: "settings", icon: Settings },
];

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function compact(value?: number | null) {
  if (value == null) return "-";
  return new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(value);
}

function dateInput(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function toHomepageFilters(filters: HomepageFilterState): HomepageFilters | undefined {
  const next: HomepageFilters = {};
  if (filters.accountId) next.accountId = filters.accountId;
  if (filters.groupId) next.groupId = filters.groupId;
  if (filters.publicModelId) next.publicModelId = filters.publicModelId;
  return Object.keys(next).length > 0 ? next : undefined;
}

function StatusBanner({ data }: { data: GatewayBootstrap }) {
  const { t } = useTranslation();
  const runtime = data.runtime;
  if (runtime.state === "running") return null;
  const locked = runtime.state === "locked";
  const conflict = runtime.error_code === "port_conflict";
  return (
    <div
      className={`flex items-start gap-3 rounded-md border px-4 py-3 text-sm ${locked || conflict ? "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300" : "bg-muted/40 text-muted-foreground"}`}
      role="status"
      data-testid={`ai-gateway-state-${runtime.state}`}
    >
      {locked ? <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0" /> : <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />}
      <div>
        <div className="font-medium">
          {locked
            ? t("aiRoutingGateway.states.locked")
            : conflict
              ? t("aiRoutingGateway.states.portConflict")
              : t("aiRoutingGateway.states.stopped")}
        </div>
        <div className="mt-0.5 text-xs opacity-80">
          {locked
            ? t(`aiRoutingGateway.errors.${runtime.lock_reason}`, runtime.lock_reason || "")
            : conflict
              ? t("aiRoutingGateway.states.portConflictHint", { port: runtime.port })
              : t("aiRoutingGateway.states.stoppedHint")}
        </div>
      </div>
    </div>
  );
}

function Metric({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return (
    <div className="min-w-0 rounded-md border bg-card p-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 truncate text-xl font-semibold" title={value}>{value}</div>
      {detail ? <div className="mt-1 truncate text-xs text-muted-foreground">{detail}</div> : null}
    </div>
  );
}

function HomeTab({
  data,
  homepage,
  days,
  mode,
  filters,
  onDays,
  onMode,
  onFilters,
}: {
  data: GatewayBootstrap;
  homepage: GatewayHomepage;
  days: TrendDays;
  mode: TrendMode;
  filters: HomepageFilterState;
  onDays: (days: TrendDays) => void;
  onMode: (mode: TrendMode) => void;
  onFilters: (filters: HomepageFilterState) => void;
}) {
  const { t } = useTranslation();
  const max = Math.max(
    1,
    ...homepage.trend.map((point) => {
      if (mode === "tokens") return point.usage.totalTokens ?? 0;
      return point.costCalculable && point.estimatedCostUsd != null ? Number(point.estimatedCostUsd) : 0;
    }),
  );
  return (
    <div className="space-y-5" data-testid="ai-gateway-tab-home">
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
        <select
          aria-label={t("aiRoutingGateway.filters.account")}
          value={filters.accountId}
          onChange={(event) => onFilters({ ...filters, accountId: event.target.value })}
          className="h-9 rounded-md border bg-background px-3 text-sm"
        >
          <option value="">{t("aiRoutingGateway.filters.allAccounts")}</option>
          {data.accounts.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
        <select
          aria-label={t("aiRoutingGateway.filters.group")}
          value={filters.groupId}
          onChange={(event) => onFilters({ ...filters, groupId: event.target.value })}
          className="h-9 rounded-md border bg-background px-3 text-sm"
        >
          <option value="">{t("aiRoutingGateway.filters.allGroups")}</option>
          {data.groups.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
        <select
          aria-label={t("aiRoutingGateway.filters.model")}
          value={filters.publicModelId}
          onChange={(event) => onFilters({ ...filters, publicModelId: event.target.value })}
          className="h-9 rounded-md border bg-background px-3 text-sm"
        >
          <option value="">{t("aiRoutingGateway.filters.allModels")}</option>
          {data.models.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}
        </select>
      </div>
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Metric label={t("aiRoutingGateway.home.accounts")} value={`${homepage.availableCount}/${homepage.accountCount}`} detail={t("aiRoutingGateway.home.available")} />
        <Metric label={t("aiRoutingGateway.home.quotaWindows")} value={homepage.staleCount ? t("aiRoutingGateway.home.staleCount", { count: homepage.staleCount }) : t("aiRoutingGateway.home.fresh")} detail={t("aiRoutingGateway.home.windowTypes")} />
        <Metric
          label={t("aiRoutingGateway.home.todayTokens")}
          value={compact(homepage.today.usage.totalTokens)}
          detail={`${t("aiRoutingGateway.home.input")} ${compact(homepage.today.usage.inputTokens)} · ${t("aiRoutingGateway.home.output")} ${compact(homepage.today.usage.outputTokens)} · ${t("aiRoutingGateway.home.cacheRead")} ${compact(homepage.today.usage.cacheReadTokens)} · ${t("aiRoutingGateway.home.cacheWrite")} ${compact(homepage.today.usage.cacheWriteTokens)}`}
        />
        <Metric label={t("aiRoutingGateway.home.estimatedCost")} value={homepage.today.costCalculable ? `$${homepage.today.estimatedCostUsd ?? "-"}` : t("aiRoutingGateway.common.notCalculable")} detail={t("aiRoutingGateway.home.costDisclaimer")} />
      </div>
      <section className="rounded-md border">
        <div className="flex flex-col gap-3 border-b px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="text-sm font-semibold">{t("aiRoutingGateway.home.trend")}</h2>
          <div className="flex flex-wrap gap-2">
            <div className="flex rounded-md border p-0.5">
              {([7, 15, 30] as TrendDays[]).map((value) => (
                <button key={value} type="button" onClick={() => onDays(value)} className={`h-7 px-2 text-xs ${days === value ? "rounded bg-muted font-medium" : "text-muted-foreground"}`}>
                  {value}{t("aiRoutingGateway.common.daysSuffix")}
                </button>
              ))}
            </div>
            <div className="flex rounded-md border p-0.5">
              {(["tokens", "cost"] as TrendMode[]).map((value) => (
                <button key={value} type="button" onClick={() => onMode(value)} className={`h-7 px-2 text-xs ${mode === value ? "rounded bg-muted font-medium" : "text-muted-foreground"}`}>
                  {t(`aiRoutingGateway.home.${value}`)}
                </button>
              ))}
            </div>
          </div>
        </div>
        {homepage.trend.every((point) => point.requestCount === 0) ? (
          <div className="p-8 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.home.emptyTrend")}</div>
        ) : (
          <div className="flex h-44 items-end gap-1 px-4 pb-4 pt-8" aria-label={t("aiRoutingGateway.home.trend")}>
            {homepage.trend.map((point) => {
              const value = mode === "tokens"
                ? point.usage.totalTokens
                : point.costCalculable && point.estimatedCostUsd != null
                  ? Number(point.estimatedCostUsd)
                  : null;
              const height = value == null ? 2 : Math.max(2, (value / max) * 112);
              return (
                <div key={point.localDate} className="group flex min-w-0 flex-1 flex-col items-center justify-end gap-1" title={`${point.localDate}: ${value == null ? t("aiRoutingGateway.common.notCalculable") : value}`}>
                  <div className={`w-full max-w-7 rounded-t-sm ${value == null ? "bg-muted" : "bg-primary/70"}`} style={{ height: `${height}px` }} />
                  <span className="hidden text-[10px] text-muted-foreground sm:block">{point.localDate.slice(5)}</span>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
}

type CreatePriceValues = { input: string; output: string; cacheRead: string; cacheWrite: string };

function AccountDetail({ account, data, onBack, onChanged }: { account?: GatewayAccount; data: GatewayBootstrap; onBack: () => Promise<void>; onChanged: () => Promise<void> }) {
  const { t } = useTranslation();
  const creating = !account;
  const editableConfiguration = creating || account.account_type === "api_key";
  const [name, setName] = useState(account?.name ?? "");
  const [groupId, setGroupId] = useState(account?.group_id ?? data.groups[0]?.id ?? "");
  const [note, setNote] = useState(account?.note ?? "");
  const [tags, setTags] = useState(account?.tags.join(", ") ?? "");
  const [threshold, setThreshold] = useState(account?.quota_threshold_override_percent?.toString() ?? "");
  const [baseUrl, setBaseUrl] = useState(account?.base_url ?? "");
  const [apiKey, setApiKey] = useState("");
  const [authMethod, setAuthMethod] = useState<"bearer" | "api_key_header">((account?.auth_method as "bearer" | "api_key_header") ?? "bearer");
  const [protocol, setProtocol] = useState<"responses" | "chat_completions">(account?.upstream_protocol ?? "responses");
  const [quotas, setQuotas] = useState<QuotaWindow[]>([]);
  const [mappings, setMappings] = useState<ModelMapping[]>(() => data.models.map((model) => ({ account_id: "", public_model_id: model.id, upstream_model_id: model.id, enabled: true })));
  const [createPrices, setCreatePrices] = useState<Record<string, CreatePriceValues>>(() => Object.fromEntries(data.models.map((model) => [model.id, { input: "", output: "", cacheRead: "", cacheWrite: "" }])));
  const [mappingModel, setMappingModel] = useState(data.models[0]?.id ?? "");
  const [upstreamModel, setUpstreamModel] = useState("");
  const [prices, setPrices] = useState<PriceRecord[]>([]);
  const [priceModel, setPriceModel] = useState(data.models[0]?.id ?? "");
  const [priceValues, setPriceValues] = useState({ input: "", output: "", cacheRead: "", cacheWrite: "" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const defaultGroupId = data.groups.find((group) => group.is_default)?.id ?? data.groups[0]?.id ?? "";
  const parsedThreshold = threshold === "" ? null : Number(threshold);
  const thresholdValid = parsedThreshold == null || (Number.isFinite(parsedThreshold) && parsedThreshold >= 0 && parsedThreshold <= 100);

  useEffect(() => {
    if (!account) return;
    void Promise.all([aiRoutingGatewayQuotaList(account.id), aiRoutingGatewayMappingList(account.id), aiRoutingGatewayPricesList()])
      .then(([nextQuotas, nextMappings, nextPrices]) => { setQuotas(nextQuotas); setMappings(nextMappings); setPrices(nextPrices); })
      .catch((value) => setError(errorText(value)));
  }, [account]);

  const save = async () => {
    setBusy(true); setError("");
    try {
      if (creating) {
        await aiRoutingGatewayAccountCreateApiKeyWithConfiguration({
          name,
          baseUrl,
          apiKey,
          authMethod,
          upstreamProtocol: protocol,
          ...(groupId !== defaultGroupId ? { groupId } : {}),
          ...(tags.trim() ? { tags: tags.split(",").map((value) => value.trim()).filter(Boolean) } : {}),
          ...(parsedThreshold != null ? { quotaThresholdOverridePercent: parsedThreshold } : {}),
          note,
          mappings: mappings.map((mapping) => ({ publicModelId: mapping.public_model_id, upstreamModelId: mapping.upstream_model_id, enabled: mapping.enabled })),
          prices: data.models.map((model) => ({
            publicModelId: model.id,
            inputPerMillionUsd: createPrices[model.id]?.input || null,
            outputPerMillionUsd: createPrices[model.id]?.output || null,
            cacheReadPerMillionUsd: createPrices[model.id]?.cacheRead || null,
            cacheWritePerMillionUsd: createPrices[model.id]?.cacheWrite || null,
          })),
        });
      } else {
        await aiRoutingGatewayAccountUpdate({
          accountId: account.id,
          name,
          groupId,
          sortOrder: account.sort_order,
          note,
          enabled: account.enabled,
          quotaThresholdOverridePercent: threshold === "" ? null : Number(threshold),
          tags: tags.split(",").map((value) => value.trim()).filter(Boolean),
          ...(account.account_type === "api_key" ? { baseUrl, apiKey: apiKey || null, authMethod, upstreamProtocol: protocol } : {}),
        });
      }
      setApiKey("");
      await onChanged();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const savePrice = async () => {
    if (!priceModel || !account || account.account_type !== "api_key") return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayPriceSave({
        publicModelId: priceModel,
        accountId: account.id,
        effectiveAt: new Date().toISOString(),
        inputPerMillionUsd: priceValues.input || null,
        outputPerMillionUsd: priceValues.output || null,
        cacheReadPerMillionUsd: priceValues.cacheRead || null,
        cacheWritePerMillionUsd: priceValues.cacheWrite || null,
      });
      setPrices(await aiRoutingGatewayPricesList());
      setPriceValues({ input: "", output: "", cacheRead: "", cacheWrite: "" });
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const saveMapping = async () => {
    if (!mappingModel || !upstreamModel.trim() || !account || account.account_type !== "api_key") return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayMappingSave({ accountId: account.id, publicModelId: mappingModel, upstreamModelId: upstreamModel.trim(), enabled: true });
      setMappings(await aiRoutingGatewayMappingList(account.id));
      setUpstreamModel("");
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const toggleMapping = async (mapping: ModelMapping) => {
    if (creating) {
      setMappings((current) => current.map((item) => item.public_model_id === mapping.public_model_id ? { ...item, enabled: !item.enabled } : item));
      return;
    }
    if (account.account_type !== "api_key") return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayMappingSave({
        accountId: mapping.account_id,
        publicModelId: mapping.public_model_id,
        upstreamModelId: mapping.upstream_model_id,
        enabled: !mapping.enabled,
      });
      setMappings(await aiRoutingGatewayMappingList(account.id));
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-4" data-testid={creating ? "account-create-detail" : "account-edit-detail"}>
      <div className="flex items-center gap-3 border-b pb-3">
        <button type="button" onClick={() => void onBack()} className="h-9 w-9 rounded-md border" aria-label={t("back")}><ChevronLeft className="mx-auto h-4 w-4" /></button>
        <div className="min-w-0"><h2 className="truncate text-base font-semibold">{creating ? t("aiRoutingGateway.accounts.addThirdParty") : account.name}</h2><p className="text-xs text-muted-foreground">{creating ? "API Key" : account.account_type === "oauth" ? "OAuth" : "API Key"}</p></div>
      </div>
      {error ? <div className="flex flex-wrap gap-x-1 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive" role="alert">{creating ? <><span>{t("aiRoutingGateway.accounts.createErrorPrefix")}</span><span>{error}</span></> : error}</div> : null}
      <div className="grid gap-3 md:grid-cols-2">
        <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.name")}</span><input value={name} onChange={(event) => setName(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <label className="space-y-1 text-xs"><span>{t(creating ? "aiRoutingGateway.accounts.createGroupField" : "aiRoutingGateway.filters.group")}</span><select value={groupId} onChange={(event) => setGroupId(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm">{data.groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}</select></label>
        <label className="space-y-1 text-xs"><span>{t(creating ? "aiRoutingGateway.accounts.createTagsField" : "aiRoutingGateway.accounts.tags")}</span><input value={tags} onChange={(event) => setTags(event.target.value)} placeholder={t("aiRoutingGateway.accounts.tagsPlaceholder")} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <label className="space-y-1 text-xs"><span>{t(creating ? "aiRoutingGateway.accounts.createThresholdField" : "aiRoutingGateway.accounts.threshold")}</span><input type="number" min={0} max={100} value={threshold} onChange={(event) => setThreshold(event.target.value)} placeholder={t("aiRoutingGateway.accounts.inherit")} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <label className="space-y-1 text-xs md:col-span-2"><span>{t("aiRoutingGateway.accounts.note")}</span><textarea value={note} onChange={(event) => setNote(event.target.value)} className="min-h-20 w-full rounded-md border bg-background p-3 text-sm" /></label>
        {editableConfiguration ? <>
          <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.baseUrl")}</span><input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
          <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.apiKey")}</span><input type="password" autoComplete="off" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={creating ? t("aiRoutingGateway.accounts.apiKey") : t("aiRoutingGateway.accounts.keepApiKey")} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
          <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.authMethod")}</span><select value={authMethod} onChange={(event) => setAuthMethod(event.target.value as typeof authMethod)} className="h-9 w-full rounded-md border bg-background px-3 text-sm"><option value="bearer">Bearer</option><option value="api_key_header">X-API-Key</option></select></label>
          <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.upstreamProtocol")}</span><select value={protocol} onChange={(event) => setProtocol(event.target.value as typeof protocol)} className="h-9 w-full rounded-md border bg-background px-3 text-sm"><option value="responses">Responses</option><option value="chat_completions">Chat Completions</option></select></label>
        </> : <div className="space-y-1 text-xs md:col-span-2"><span>{t("aiRoutingGateway.accounts.baseUrl")}</span><div className="break-all rounded-md border bg-muted/20 px-3 py-2 text-sm">{baseUrl || "-"}</div></div>}
      </div>
      {!creating ? <button type="button" onClick={save} disabled={busy || !name.trim() || !groupId || !thresholdValid} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50"><Save className="h-4 w-4" />{t("aiRoutingGateway.common.save")}</button> : null}
      {creating ? (
        <section className="space-y-3 border-t pt-4">
          <div><h3 className="text-sm font-semibold">{t("aiRoutingGateway.accounts.mappingAndPricing")}</h3><p className="mt-1 text-xs text-muted-foreground">{t("aiRoutingGateway.accounts.mappingAndPricingHint")}</p></div>
          {data.models.map((model) => {
            const mapping = mappings.find((item) => item.public_model_id === model.id)!;
            const values = createPrices[model.id];
            return <div key={model.id} className="space-y-3 rounded-md border p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <label className="flex min-w-0 items-center gap-2 text-sm"><input type="checkbox" aria-label={t("aiRoutingGateway.accounts.toggleMapping", { model: model.displayName })} checked={mapping.enabled} onChange={() => void toggleMapping(mapping)} /><span className="break-words font-medium">{model.displayName}</span></label>
                <input aria-label={`${model.displayName} ${t("aiRoutingGateway.accounts.upstreamModel")}`} value={mapping.upstream_model_id} onChange={(event) => setMappings((current) => current.map((item) => item.public_model_id === model.id ? { ...item, upstream_model_id: event.target.value } : item))} className="h-9 min-w-0 rounded-md border bg-background px-2 text-sm" />
              </div>
              <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                {(["input", "output", "cacheRead", "cacheWrite"] as const).map((field) => <input key={field} aria-label={`${model.displayName} ${t(`aiRoutingGateway.settings.${field}Price`)}`} value={values[field]} onChange={(event) => setCreatePrices((current) => ({ ...current, [model.id]: { ...current[model.id], [field]: event.target.value } }))} placeholder={t(`aiRoutingGateway.settings.${field}Price`)} className="h-9 min-w-0 rounded-md border bg-background px-2 text-sm" />)}
              </div>
            </div>;
          })}
          <div className="flex flex-wrap gap-2 border-t pt-4">
            <button type="button" onClick={save} disabled={busy || !name.trim() || !groupId || !baseUrl.trim() || !apiKey || !thresholdValid} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50">{busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}{t("aiRoutingGateway.common.save")}</button>
            <button type="button" onClick={() => void onBack()} disabled={busy} className="h-9 rounded-md border px-3 text-sm disabled:opacity-50">{t("aiRoutingGateway.common.cancel")}</button>
          </div>
        </section>
      ) : null}
      {!creating ? <>
      <div className="grid gap-4 lg:grid-cols-2">
        <section className="rounded-md border bg-background p-3">
          <h3 className="text-sm font-semibold">{t("aiRoutingGateway.accounts.quota")}</h3>
          {account.account_type !== "oauth" ? (
            <p className="mt-3 text-sm text-muted-foreground">{t("aiRoutingGateway.accounts.quotaOAuthOnly")}</p>
          ) : quotas.length === 0 ? (
            <p className="mt-3 text-sm text-muted-foreground">{t("aiRoutingGateway.accounts.noQuota")}</p>
          ) : (
            <div className="mt-3 space-y-3">
              {quotas.map((quota) => (
                <div key={quota.id} className="rounded-md border p-2 text-xs">
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-medium">{quota.name}</span>
                    <span className={quota.is_stale ? "text-amber-600" : "text-muted-foreground"}>{quota.is_stale ? t("aiRoutingGateway.accounts.stale") : quota.scope_type}</span>
                  </div>
                  <div className="mt-1 grid grid-cols-2 gap-1 text-muted-foreground">
                    <span>{t("aiRoutingGateway.accounts.used")}: {quota.used_percent == null ? "-" : `${quota.used_percent}%`}</span>
                    <span>{t("aiRoutingGateway.accounts.remaining")}: {quota.remaining_percent == null ? "-" : `${quota.remaining_percent}%`}</span>
                    <span>{t("aiRoutingGateway.accounts.reset")}: {quota.resets_at ?? "-"}</span>
                    <span>{t("aiRoutingGateway.accounts.duration")}: {quota.duration_seconds == null ? "-" : `${quota.duration_seconds}s`}</span>
                  </div>
                  {quota.scope_value || quota.raw_kind ? <div className="mt-1 truncate text-muted-foreground">{quota.scope_value ?? quota.raw_kind}</div> : null}
                </div>
              ))}
            </div>
          )}
        </section>
        <section className="rounded-md border bg-background p-3">
          <h3 className="text-sm font-semibold">{t("aiRoutingGateway.accounts.mappings")}</h3>
          {account.account_type === "api_key" ? <div className="mt-3 flex gap-2">
            <select value={mappingModel} onChange={(event) => setMappingModel(event.target.value)} className="h-9 min-w-0 flex-1 rounded-md border bg-background px-2 text-sm">
              {data.models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}
            </select>
            <input value={upstreamModel} onChange={(event) => setUpstreamModel(event.target.value)} placeholder={t("aiRoutingGateway.accounts.upstreamModel")} className="h-9 min-w-0 flex-1 rounded-md border bg-background px-2 text-sm" />
            <button type="button" onClick={saveMapping} disabled={busy} className="h-9 w-9 rounded-md border disabled:opacity-50" title={t("aiRoutingGateway.common.add")}><Plus className="mx-auto h-4 w-4" /></button>
          </div> : null}
          <div className="mt-3 space-y-1">
            {mappings.length === 0 ? <p className="text-sm text-muted-foreground">{t("aiRoutingGateway.accounts.noMappings")}</p> : mappings.map((mapping) => (
              <div key={mapping.public_model_id} className="flex items-center gap-2 text-xs">
                <span className="min-w-0 flex-1 truncate">{mapping.public_model_id} → {mapping.upstream_model_id}</span>
                {account.account_type === "api_key" ? <button
                  type="button"
                  aria-pressed={mapping.enabled}
                  aria-label={t("aiRoutingGateway.accounts.toggleMapping", { model: mapping.public_model_id })}
                  onClick={() => void toggleMapping(mapping)}
                  disabled={busy}
                  className={`h-7 rounded-md border px-2 text-xs disabled:opacity-50 ${mapping.enabled ? "text-emerald-600" : "text-muted-foreground"}`}
                >
                  {mapping.enabled ? t("aiRoutingGateway.common.enabled") : t("aiRoutingGateway.common.disabled")}
                </button> : <span className="text-muted-foreground">{mapping.enabled ? t("aiRoutingGateway.common.enabled") : t("aiRoutingGateway.common.disabled")}</span>}
              </div>
            ))}
          </div>
        </section>
      </div>
      <section className="rounded-md border bg-background p-3">
        <h3 className="text-sm font-semibold">{t("aiRoutingGateway.settings.pricing")}</h3>
        <div className="mt-3 overflow-x-auto"><table className="w-full min-w-[760px] text-left text-xs"><thead><tr className="text-muted-foreground"><th className="pb-2">{t("aiRoutingGateway.settings.model")}</th><th>{t("aiRoutingGateway.settings.inputPrice")}</th><th>{t("aiRoutingGateway.settings.outputPrice")}</th><th>{t("aiRoutingGateway.settings.cacheReadPrice")}</th><th>{t("aiRoutingGateway.settings.cacheWritePrice")}</th></tr></thead><tbody>{data.models.map((model) => { const modelPrices = prices.filter((price) => price.public_model_id === model.id); const effective = modelPrices.find((price) => price.account_id === account.id) ?? modelPrices.find((price) => price.account_id == null); return <tr key={model.id} className="border-t"><td className="py-2">{model.displayName}</td><td>{effective?.input_per_million_usd ?? "-"}</td><td>{effective?.output_per_million_usd ?? "-"}</td><td>{effective?.cache_read_per_million_usd ?? "-"}</td><td>{effective?.cache_write_per_million_usd ?? "-"}</td></tr>; })}</tbody></table></div>
        {account.account_type === "api_key" ? <div className="mt-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-6"><select aria-label={t("aiRoutingGateway.settings.model")} value={priceModel} onChange={(event) => setPriceModel(event.target.value)} className="h-9 rounded-md border bg-background px-2 text-sm">{data.models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}</select>{(["input", "output", "cacheRead", "cacheWrite"] as const).map((field) => <input key={field} aria-label={t(`aiRoutingGateway.settings.${field}Price`)} value={priceValues[field]} onChange={(event) => setPriceValues((current) => ({ ...current, [field]: event.target.value }))} placeholder={t(`aiRoutingGateway.settings.${field}Price`)} className="h-9 rounded-md border bg-background px-2 text-sm" />)}<button type="button" onClick={() => void savePrice()} disabled={busy} className="h-9 rounded-md border px-3 text-sm disabled:opacity-50">{t("aiRoutingGateway.common.save")}</button></div> : null}
      </section>
      </> : null}
    </div>
  );
}

function AccountGroupManagerDialog({
  open,
  groups,
  busy,
  error,
  onOpenChange,
  onCreate,
  onRename,
  onDelete,
}: {
  open: boolean;
  groups: GatewayBootstrap["groups"];
  busy: boolean;
  error: string;
  onOpenChange: (open: boolean) => void;
  onCreate: (name: string) => Promise<void>;
  onRename: (groupId: string, name: string) => Promise<void>;
  onDelete: (groupId: string, name: string) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [newName, setNewName] = useState("");
  const [editingGroupId, setEditingGroupId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");

  const changeOpen = (nextOpen: boolean) => {
    if (!nextOpen) {
      setNewName("");
      setEditingGroupId(null);
      setEditingName("");
    }
    onOpenChange(nextOpen);
  };

  const create = async () => {
    if (!newName.trim()) return;
    try {
      await onCreate(newName.trim());
      setNewName("");
    } catch {
      // 调用方已展示错误，保留输入以便重试。
    }
  };

  const rename = async (groupId: string) => {
    if (!editingName.trim()) return;
    try {
      await onRename(groupId, editingName.trim());
      setEditingGroupId(null);
      setEditingName("");
    } catch {
      // 调用方已展示错误，保留输入以便重试。
    }
  };

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      {open ? <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("aiRoutingGateway.accounts.manageGroups")}</DialogTitle>
          <DialogDescription>{t("aiRoutingGateway.accounts.manageGroupsDescription")}</DialogDescription>
        </DialogHeader>
        {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive" role="alert">{error}</div> : null}
        <div className="space-y-4">
          <div className="flex gap-2 rounded-md border bg-muted/20 p-3">
            <input
              value={newName}
              onChange={(event) => setNewName(event.target.value)}
              placeholder={t("aiRoutingGateway.accounts.groupNamePlaceholder")}
              className="h-9 min-w-0 flex-1 rounded-md border bg-background px-3 text-sm"
            />
            <button type="button" onClick={() => void create()} disabled={busy || !newName.trim()} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50">
              {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
              {t("aiRoutingGateway.accounts.createGroup")}
            </button>
          </div>
          <div className="space-y-2">
            {groups.map((group) => {
              const editing = editingGroupId === group.id;
              return <div key={group.id} className="flex flex-wrap items-center gap-2 rounded-md border p-3">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{group.name}</div>
                  <div className="text-xs text-muted-foreground">{t(group.is_default ? "aiRoutingGateway.accounts.defaultGroupHint" : "aiRoutingGateway.accounts.customGroupHint")}</div>
                </div>
                {editing ? <>
                  <input value={editingName} onChange={(event) => setEditingName(event.target.value)} className="h-9 min-w-40 flex-1 rounded-md border bg-background px-3 text-sm" />
                  <button type="button" onClick={() => void rename(group.id)} disabled={busy || !editingName.trim()} className="h-9 rounded-md bg-primary px-3 text-sm text-primary-foreground disabled:opacity-50">{t("aiRoutingGateway.common.save")}</button>
                  <button type="button" onClick={() => { setEditingGroupId(null); setEditingName(""); }} disabled={busy} className="h-9 rounded-md border px-3 text-sm disabled:opacity-50">{t("aiRoutingGateway.common.cancel")}</button>
                </> : !group.is_default ? <>
                  <button type="button" onClick={() => { setEditingGroupId(group.id); setEditingName(group.name); }} disabled={busy} className="h-9 w-9 rounded-md border disabled:opacity-50" title={t("aiRoutingGateway.accounts.renameGroup")}><Pencil className="mx-auto h-4 w-4" /></button>
                  <button type="button" onClick={() => void onDelete(group.id, group.name)} disabled={busy} className="h-9 w-9 rounded-md border text-destructive disabled:opacity-50" title={t("aiRoutingGateway.accounts.deleteGroup")}><Trash2 className="mx-auto h-4 w-4" /></button>
                </> : null}
              </div>;
            })}
          </div>
        </div>
        <DialogFooter>
          <button type="button" onClick={() => changeOpen(false)} className="h-9 rounded-md border px-3 text-sm">{t("aiRoutingGateway.common.close")}</button>
        </DialogFooter>
      </DialogContent> : null}
    </Dialog>
  );
}

function AccountsTab({ data, reload }: { data: GatewayBootstrap; reload: () => Promise<void> }) {
  const { t } = useTranslation();
  const [viewMode, setViewMode] = useState<"list" | "detail">("list");
  const [detailMode, setDetailMode] = useState<"create" | "edit">("create");
  const [selectedAccountId, setSelectedAccountId] = useState<string | null>(null);
  const orderedGroups = useMemo(() => [
    ...data.groups.filter((group) => group.is_default),
    ...data.groups.filter((group) => !group.is_default),
  ], [data.groups]);
  const defaultGroupId = orderedGroups.find((group) => group.is_default)?.id ?? orderedGroups[0]?.id ?? "";
  const [activeGroupId, setActiveGroupId] = useState(defaultGroupId);
  const [searchText, setSearchText] = useState("");
  const [selectedAccountIds, setSelectedAccountIds] = useState<Set<string>>(() => new Set());
  const [groupManagerOpen, setGroupManagerOpen] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const groupAccounts = useMemo(
    () => data.accounts.filter((account) => account.group_id === activeGroupId),
    [activeGroupId, data.accounts],
  );
  const visible = useMemo(() => {
    const query = searchText.trim().toLocaleLowerCase();
    return groupAccounts.filter((account) => {
      if (!query) return true;
      const searchable = [
        account.name,
        account.base_url,
        account.auth_method,
        account.upstream_protocol,
        account.note,
        ...(account.tags ?? []),
        ...(account.model_mappings ?? []).flatMap((mapping) => [mapping.public_model_id, mapping.upstream_model_id]),
      ].filter(Boolean).join(" ").toLocaleLowerCase();
      return searchable.includes(query);
    });
  }, [groupAccounts, searchText]);
  const selectedVisibleIds = useMemo(
    () => visible.filter((account) => selectedAccountIds.has(account.id)).map((account) => account.id),
    [selectedAccountIds, visible],
  );
  const allVisibleSelected = visible.length > 0 && selectedVisibleIds.length === visible.length;

  useEffect(() => {
    if (!orderedGroups.some((group) => group.id === activeGroupId)) {
      setActiveGroupId(defaultGroupId);
    }
  }, [activeGroupId, defaultGroupId, orderedGroups]);

  useEffect(() => {
    const visibleIds = new Set(visible.map((account) => account.id));
    setSelectedAccountIds((current) => {
      const next = new Set([...current].filter((accountId) => visibleIds.has(accountId)));
      return next.size === current.size ? current : next;
    });
  }, [visible]);

  const showDetail = (mode: "create" | "edit", accountId: string | null = null) => {
    setDetailMode(mode); setSelectedAccountId(accountId); setViewMode("detail"); setError("");
  };
  const returnToList = async () => { setViewMode("list"); setSelectedAccountId(null); await reload(); };

  const createGroup = async (name: string) => {
    setBusy(true); setError("");
    try {
      const sortOrder = Math.max(-1, ...data.groups.map((group) => group.sort_order)) + 1;
      await aiRoutingGatewayGroupCreate({ name, sortOrder });
      await reload();
    } catch (value) {
      setError(errorText(value));
      throw value;
    } finally {
      setBusy(false);
    }
  };

  const renameGroup = async (groupId: string, name: string) => {
    if (orderedGroups.find((group) => group.id === groupId)?.is_default) return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayGroupRename({ groupId, name });
      await reload();
    } catch (value) {
      setError(errorText(value));
      throw value;
    } finally {
      setBusy(false);
    }
  };

  const deleteGroup = async (groupId: string, name: string) => {
    if (orderedGroups.find((group) => group.id === groupId)?.is_default) return;
    if (!window.confirm(t("aiRoutingGateway.accounts.deleteGroupConfirm", { name }))) return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayGroupDelete(groupId);
      if (activeGroupId === groupId) setActiveGroupId(defaultGroupId);
      await reload();
    } catch (value) {
      setError(errorText(value));
      throw value;
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (account: GatewayAccount) => {
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayAccountUpdate({ accountId: account.id, name: account.name, groupId: account.group_id, sortOrder: account.sort_order, note: account.note, enabled: !account.enabled, quotaThresholdOverridePercent: account.quota_threshold_override_percent, tags: account.tags });
      await reload();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const move = async (account: GatewayAccount, direction: -1 | 1) => {
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayAccountMove(account.id, direction);
      await reload();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (account: GatewayAccount) => {
    if (!window.confirm(t("aiRoutingGateway.accounts.deleteConfirm", { name: account.name }))) return;
    setBusy(true); setError("");
    try {
      const token = await aiRoutingGatewayAccountDeleteConfirmation(account.id);
      await aiRoutingGatewayAccountDelete(account.id, token);
      await reload();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const toggleSelection = (accountId: string, selected: boolean) => {
    const visibleIds = new Set(visible.map((account) => account.id));
    if (!visibleIds.has(accountId)) return;
    setSelectedAccountIds((current) => {
      const next = new Set([...current].filter((id) => visibleIds.has(id)));
      if (selected) next.add(accountId); else next.delete(accountId);
      return next;
    });
  };

  const toggleSelectAll = () => {
    setSelectedAccountIds(allVisibleSelected ? new Set() : new Set(visible.map((account) => account.id)));
  };

  const disableSelected = async () => {
    const accountIds = visible.filter((account) => selectedAccountIds.has(account.id)).map((account) => account.id);
    if (accountIds.length === 0) return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayAccountsDisable(accountIds);
      await reload();
      setSelectedAccountIds(new Set());
    } catch (value) {
      setError(t("aiRoutingGateway.accounts.batchError", { error: errorText(value) }));
    } finally {
      setBusy(false);
    }
  };

  const deleteSelected = async () => {
    const accountIds = visible.filter((account) => selectedAccountIds.has(account.id)).map((account) => account.id);
    if (accountIds.length === 0 || !window.confirm(t("aiRoutingGateway.accounts.bulkDeleteConfirm", { count: accountIds.length }))) return;
    setBusy(true); setError("");
    try {
      const token = await aiRoutingGatewayAccountsDeleteConfirmation(accountIds);
      await aiRoutingGatewayAccountsDelete(accountIds, token);
      await reload();
      setSelectedAccountIds(new Set());
    } catch (value) {
      setError(t("aiRoutingGateway.accounts.batchError", { error: errorText(value) }));
    } finally {
      setBusy(false);
    }
  };

  if (viewMode === "detail") {
    const account = detailMode === "edit" ? data.accounts.find((item) => item.id === selectedAccountId) : undefined;
    return <AccountDetail account={account} data={data} onBack={returnToList} onChanged={returnToList} />;
  }

  return (
    <div className="space-y-4" data-testid="ai-gateway-tab-accounts" data-selected-count={selectedAccountIds.size}>
      <div className="flex items-center gap-2 overflow-x-auto border-b" role="tablist" aria-label={t("aiRoutingGateway.accounts.groupTabsLabel")}>
        {orderedGroups.map((group) => <button
          key={group.id}
          type="button"
          role="tab"
          aria-selected={activeGroupId === group.id}
          onClick={() => setActiveGroupId(group.id)}
          className={`h-10 shrink-0 border-b-2 px-3 text-sm ${activeGroupId === group.id ? "border-primary font-medium" : "border-transparent text-muted-foreground"}`}
        >{group.name}</button>)}
        <button type="button" onClick={() => { setError(""); setGroupManagerOpen(true); }} className="ml-auto h-9 w-9 shrink-0 rounded-md border" title={t("aiRoutingGateway.accounts.manageGroups")} aria-label={t("aiRoutingGateway.accounts.manageGroups")}><Settings className="mx-auto h-4 w-4" /></button>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <label className="relative min-w-0 flex-1 sm:max-w-md">
          <Search className="pointer-events-none absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
          <input aria-label={t("aiRoutingGateway.accounts.search")} value={searchText} onChange={(event) => setSearchText(event.target.value)} placeholder={t("aiRoutingGateway.accounts.searchPlaceholder")} className="h-9 w-full rounded-md border bg-background pl-9 pr-3 text-sm" />
        </label>
        {groupAccounts.length > 0 ? <button type="button" onClick={() => showDetail("create")} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground"><Plus className="h-4 w-4" />{t("aiRoutingGateway.accounts.addThirdParty")}</button> : null}
      </div>
      <input type="hidden" aria-hidden="true" placeholder={t("aiRoutingGateway.accounts.newGroup")} />
      {visible.length > 0 ? <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border bg-muted/20 px-3 py-2">
        <label className="inline-flex min-w-0 items-center gap-2 text-sm">
          <input type="checkbox" checked={allVisibleSelected} onChange={toggleSelectAll} aria-label={t(allVisibleSelected ? "aiRoutingGateway.accounts.clearVisibleSelection" : "aiRoutingGateway.accounts.selectAllVisible")} />
          <span className="break-words">{t(allVisibleSelected ? "aiRoutingGateway.accounts.clearVisibleSelection" : "aiRoutingGateway.accounts.selectAllVisible")}</span>
        </label>
        {selectedVisibleIds.length > 0 ? <div className="flex flex-wrap items-center gap-2">
          <span className="text-xs text-muted-foreground">{t("aiRoutingGateway.accounts.selectedCount", { count: selectedVisibleIds.length })}</span>
          <button type="button" onClick={() => void disableSelected()} disabled={busy} className="h-8 rounded-md border bg-background px-3 text-xs font-medium disabled:opacity-50">{t("aiRoutingGateway.accounts.bulkDisable")}</button>
          <button type="button" onClick={() => void deleteSelected()} disabled={busy} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-destructive/30 bg-background px-3 text-xs font-medium text-destructive disabled:opacity-50"><Trash2 className="h-3.5 w-3.5" />{t("aiRoutingGateway.accounts.bulkDelete")}</button>
        </div> : null}
      </div> : null}
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive" role="alert">{error}</div> : null}
      {visible.length === 0 ? (
        <div className="rounded-md border border-dashed p-8 text-center text-sm text-muted-foreground">
          <div>{t(groupAccounts.length > 0 ? "aiRoutingGateway.accounts.emptySearch" : "aiRoutingGateway.accounts.empty")}</div>
          {groupAccounts.length === 0 ? <button type="button" onClick={() => showDetail("create")} className="mt-4 inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground"><Plus className="h-4 w-4" />{t("aiRoutingGateway.accounts.addThirdParty")}</button> : null}
        </div>
      ) : (
        <div className="space-y-3">
          {visible.map((account) => {
            const groupIndex = groupAccounts.findIndex((item) => item.id === account.id);
            const accountType = account.account_type === "oauth" ? "OAuth" : "API Key";
            return <article key={account.id} className="rounded-md border bg-card p-4">
              <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-start">
                <div className="flex min-w-0 items-start gap-3">
                  <input type="checkbox" checked={selectedAccountIds.has(account.id)} onChange={(event) => toggleSelection(account.id, event.target.checked)} aria-label={t("aiRoutingGateway.accounts.selectAccount", { name: account.name })} className="mt-1 shrink-0" />
                  <button type="button" onClick={() => showDetail("edit", account.id)} aria-label={`${account.name} ${accountType}`} className="min-w-0 flex-1 text-left">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="min-w-0 break-words text-sm font-semibold">{account.name}</span>
                      <span className={`shrink-0 rounded border px-2 py-0.5 text-[11px] ${account.enabled ? "border-emerald-500/30 text-emerald-600" : "text-muted-foreground"}`}>{account.enabled ? t("aiRoutingGateway.common.enabled") : t("aiRoutingGateway.common.disabled")}</span>
                      <span className="shrink-0 rounded border bg-muted/30 px-2 py-0.5 text-[11px] text-muted-foreground">{accountType}</span>
                      {account.auth_method ? <span className="shrink-0 rounded border px-2 py-0.5 text-[11px] text-muted-foreground">{account.auth_method === "api_key_header" ? "X-API-Key" : "Bearer"}</span> : null}
                      {account.upstream_protocol ? <span className="shrink-0 rounded border px-2 py-0.5 text-[11px] text-muted-foreground">{account.upstream_protocol === "chat_completions" ? "Chat Completions" : "Responses"}</span> : null}
                    </div>
                    <div className="mt-2 break-all text-xs text-muted-foreground">{account.base_url ?? "-"}</div>
                    {account.note ? <div className="mt-2 break-words text-sm text-muted-foreground">{account.note}</div> : null}
                    <div className="mt-3 flex flex-wrap gap-1.5">
                      {(account.tags ?? []).map((tag) => <span key={tag} className="max-w-full break-words rounded border bg-background px-2 py-0.5 text-[11px] text-muted-foreground">{tag}</span>)}
                      {(account.model_mappings ?? []).map((mapping) => <span key={mapping.public_model_id} className="max-w-full break-words rounded border bg-background px-2 py-0.5 text-[11px] text-muted-foreground">{mapping.public_model_id} → {mapping.upstream_model_id}</span>)}
                      {(account.tags ?? []).length === 0 && (account.model_mappings ?? []).length === 0 ? <span className="text-xs text-muted-foreground">{t("aiRoutingGateway.accounts.noMappings")}</span> : null}
                    </div>
                  </button>
                </div>
                <div className="flex flex-wrap items-center gap-2 lg:max-w-sm lg:justify-end">
                  <button type="button" onClick={() => showDetail("edit", account.id)} disabled={busy} className="h-8 w-8 rounded-md border disabled:opacity-50" title={t("edit")} aria-label={t("edit")}><Pencil className="mx-auto h-4 w-4" /></button>
                  <button type="button" onClick={() => void move(account, -1)} disabled={busy || groupIndex <= 0} className="h-8 w-8 rounded-md border disabled:opacity-50" title={t("aiRoutingGateway.accounts.moveUp")} aria-label={t("aiRoutingGateway.accounts.moveUp")}><ChevronLeft className="mx-auto h-4 w-4 rotate-90" /></button>
                  <button type="button" onClick={() => void move(account, 1)} disabled={busy || groupIndex < 0 || groupIndex >= groupAccounts.length - 1} className="h-8 w-8 rounded-md border disabled:opacity-50" title={t("aiRoutingGateway.accounts.moveDown")} aria-label={t("aiRoutingGateway.accounts.moveDown")}><ChevronRight className="mx-auto h-4 w-4 rotate-90" /></button>
                  <button type="button" onClick={() => void toggle(account)} disabled={busy} className={`h-8 rounded-md border px-3 text-xs disabled:opacity-50 ${account.enabled ? "text-emerald-600" : "text-muted-foreground"}`}>{account.enabled ? t("aiRoutingGateway.common.enabled") : t("aiRoutingGateway.common.disabled")}</button>
                  <button type="button" onClick={() => void remove(account)} disabled={busy} className="h-8 w-8 rounded-md border text-destructive disabled:opacity-50" title={t("aiRoutingGateway.accounts.delete")} aria-label={t("aiRoutingGateway.accounts.delete")}><Trash2 className="mx-auto h-4 w-4" /></button>
                </div>
              </div>
            </article>;
          })}
        </div>
      )}
      <AccountGroupManagerDialog open={groupManagerOpen} groups={orderedGroups} busy={busy} error={error} onOpenChange={setGroupManagerOpen} onCreate={createGroup} onRename={renameGroup} onDelete={deleteGroup} />
    </div>
  );
}

function KeysTab({ data, reload }: { data: GatewayBootstrap; reload: () => Promise<void> }) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [groups, setGroups] = useState<string[]>(data.groups.map((group) => group.id));
  const [models, setModels] = useState<string[]>(data.models.map((model) => model.id));
  const [expiresAt, setExpiresAt] = useState("");
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => () => setPlaintext(null), []);

  const create = async () => {
    setBusy(true); setError(""); setPlaintext(null);
    try {
      const value = await aiRoutingGatewayKeyCreate({ name, groupIds: groups, modelIds: models, expiresAt: expiresAt ? new Date(`${expiresAt}T23:59:59`).toISOString() : null });
      setPlaintext(value.plaintext); setName(""); await reload();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const regenerate = async (key: GatewayKeyRecord) => {
    if (!window.confirm(t("aiRoutingGateway.keys.regenerateConfirm", { name: key.name }))) return;
    setBusy(true); setError(""); setPlaintext(null);
    try {
      const value = await aiRoutingGatewayKeyRegenerate(key.id); setPlaintext(value.plaintext); await reload();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const copyKey = async (key: GatewayKeyRecord) => {
    setBusy(true); setError("");
    try {
      await navigator.clipboard.writeText(await aiRoutingGatewayKeyCopy(key.id));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const updateGroups = async (key: GatewayKeyRecord, groupId: string, checked: boolean) => {
    const next = checked ? [...key.groupIds, groupId] : key.groupIds.filter((id) => id !== groupId);
    if (next.length === 0) return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayKeyGroupsUpdate(key.id, next);
      await reload();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-4" data-testid="ai-gateway-tab-keys">
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      {plaintext ? <div className="rounded-md border border-amber-500/30 bg-amber-500/10 p-4"><div className="flex items-center justify-between gap-2"><div><div className="text-sm font-semibold">{t("aiRoutingGateway.keys.oneTimeTitle")}</div><div className="text-xs text-muted-foreground">{t("aiRoutingGateway.keys.oneTimeHint")}</div></div><button type="button" onClick={() => setPlaintext(null)} className="h-8 w-8 rounded-md" title={t("aiRoutingGateway.common.close")}><X className="mx-auto h-4 w-4" /></button></div><div className="mt-3 flex gap-2"><code className="min-w-0 flex-1 overflow-x-auto rounded-md border bg-background px-3 py-2 text-xs select-text">{plaintext}</code><button type="button" onClick={async () => { await navigator.clipboard.writeText(plaintext); setCopied(true); window.setTimeout(() => setCopied(false), 1500); }} className="h-9 w-9 rounded-md border bg-background" title={t("aiRoutingGateway.common.copy")}>{copied ? <Check className="mx-auto h-4 w-4" /> : <Clipboard className="mx-auto h-4 w-4" />}</button></div></div> : null}
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.keys.create")}</h2><div className="mt-3 grid gap-3 md:grid-cols-2"><input value={name} onChange={(event) => setName(event.target.value)} placeholder={t("aiRoutingGateway.keys.name")} className="h-9 rounded-md border bg-background px-3 text-sm" /><input type="date" value={expiresAt} onChange={(event) => setExpiresAt(event.target.value)} aria-label={t("aiRoutingGateway.keys.expiresAt")} className="h-9 rounded-md border bg-background px-3 text-sm" /><fieldset className="rounded-md border p-3"><legend className="px-1 text-xs">{t("aiRoutingGateway.keys.groupPermissions")}</legend>{data.groups.map((group) => <label key={group.id} className="mr-4 inline-flex items-center gap-2 text-sm"><input type="checkbox" checked={groups.includes(group.id)} onChange={(event) => setGroups((current) => event.target.checked ? [...current, group.id] : current.filter((id) => id !== group.id))} />{group.name}</label>)}</fieldset><fieldset className="rounded-md border p-3"><legend className="px-1 text-xs">{t("aiRoutingGateway.keys.modelPermissions")}</legend>{data.models.map((model) => <label key={model.id} className="mr-4 inline-flex items-center gap-2 text-sm"><input type="checkbox" checked={models.includes(model.id)} onChange={(event) => setModels((current) => event.target.checked ? [...current, model.id] : current.filter((id) => id !== model.id))} />{model.displayName}</label>)}</fieldset></div><button type="button" onClick={() => void create()} disabled={busy || !name.trim() || groups.length === 0 || models.length === 0} className="mt-3 inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50"><Plus className="h-4 w-4" />{t("aiRoutingGateway.common.create")}</button></section>
      {data.keys.length === 0 ? <div className="rounded-md border border-dashed p-10 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.keys.empty")}</div> : <div className="space-y-2">{data.keys.map((key) => { const expired = !!key.expiresAt && new Date(key.expiresAt) <= new Date(); const revoked = !!key.revokedAt; const status = revoked ? t("aiRoutingGateway.keys.revoked") : expired ? t("aiRoutingGateway.keys.expired") : key.enabled ? t("aiRoutingGateway.common.enabled") : t("aiRoutingGateway.common.disabled"); return <section key={key.id} className="rounded-md border p-3"><div className="flex flex-wrap items-start gap-3"><div className="min-w-0 flex-1"><div className="truncate text-sm font-medium">{key.name}</div><div className="mt-0.5 font-mono text-xs text-muted-foreground">{key.maskedKey} · {status}</div><div className="mt-1 text-xs text-muted-foreground">{t("aiRoutingGateway.keys.createdAt")}: {new Date(key.createdAt).toLocaleString()} · {t("aiRoutingGateway.keys.expiresAt")}: {key.expiresAt ? new Date(key.expiresAt).toLocaleString() : "-"}</div></div><button type="button" onClick={() => void copyKey(key)} disabled={busy} className="h-8 w-8 rounded-md border disabled:opacity-50" title={t("aiRoutingGateway.common.copy")}>{copied ? <Check className="mx-auto h-4 w-4" /> : <Clipboard className="mx-auto h-4 w-4" />}</button><button type="button" disabled={revoked || busy} onClick={async () => { setBusy(true); setError(""); try { await aiRoutingGatewayKeySetEnabled(key.id, !key.enabled); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); } }} className="h-8 rounded-md border px-3 text-xs disabled:opacity-50">{key.enabled ? t("aiRoutingGateway.keys.disable") : t("aiRoutingGateway.keys.enable")}</button><button type="button" onClick={() => void regenerate(key)} disabled={busy} className="h-8 w-8 rounded-md border disabled:opacity-50" title={t("aiRoutingGateway.keys.regenerate")}><RotateCw className="mx-auto h-4 w-4" /></button><button type="button" disabled={revoked || busy} onClick={async () => { if (!window.confirm(t("aiRoutingGateway.keys.revokeConfirm", { name: key.name }))) return; setBusy(true); setError(""); try { await aiRoutingGatewayKeyRevoke(key.id); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); } }} className="h-8 w-8 rounded-md border text-destructive disabled:opacity-50" title={t("aiRoutingGateway.keys.revoke")}><Trash2 className="mx-auto h-4 w-4" /></button></div><fieldset className="mt-3 border-t pt-3"><legend className="text-xs text-muted-foreground">{t("aiRoutingGateway.keys.groupPermissions")}</legend>{data.groups.map((group) => <label key={group.id} className="mr-4 mt-2 inline-flex items-center gap-2 text-xs"><input type="checkbox" checked={key.groupIds.includes(group.id)} disabled={busy || revoked} onChange={(event) => void updateGroups(key, group.id, event.target.checked)} />{group.name}</label>)}</fieldset><div className="mt-3 grid grid-cols-2 gap-2 text-xs sm:grid-cols-4"><div><span className="text-muted-foreground">{t("aiRoutingGateway.keys.today")}</span><div>{key.today.requestCount} req · {compact(key.today.totalTokens)} tok · {key.today.estimatedCostUsd == null ? "-" : `$${key.today.estimatedCostUsd}`}</div></div><div><span className="text-muted-foreground">{t("aiRoutingGateway.keys.last30Days")}</span><div>{key.last30Days.requestCount} req · {compact(key.last30Days.totalTokens)} tok · {key.last30Days.estimatedCostUsd == null ? "-" : `$${key.last30Days.estimatedCostUsd}`}</div></div><div className="col-span-2 truncate text-muted-foreground">{key.modelIds.join(", ")}</div></div></section>; })}</div>}
    </div>
  );
}

function LogsTab({ data }: { data: GatewayBootstrap }) {
  const { t } = useTranslation();
  const [status, setStatus] = useState("");
  const [model, setModel] = useState("");
  const [accountId, setAccountId] = useState("");
  const [groupId, setGroupId] = useState("");
  const [upstreamModel, setUpstreamModel] = useState("");
  const [errorCode, setErrorCode] = useState("");
  const [apiKeyId, setApiKeyId] = useState("");
  const [startedAtOrAfter, setStartedAtOrAfter] = useState("");
  const [startedBefore, setStartedBefore] = useState("");
  const [pageSize, setPageSize] = useState(25);
  const [items, setItems] = useState<RequestLog[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [cursor, setCursor] = useState<string | undefined>();
  const [history, setHistory] = useState<Array<string | undefined>>([]);
  const [attempts, setAttempts] = useState<Record<string, RequestAttempt[]>>({});
  const [expanded, setExpanded] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = useCallback(async (nextCursorValue?: string) => {
    setLoading(true); setError("");
    try {
      const page = await aiRoutingGatewayLogsQuery({
        startedAtOrAfter: startedAtOrAfter ? `${startedAtOrAfter}T00:00:00.000Z` : undefined,
        startedBefore: startedBefore ? `${startedBefore}T23:59:59.999Z` : undefined,
        accountId: accountId || undefined,
        groupId: groupId || undefined,
        publicModelId: model || undefined,
        upstreamModelId: upstreamModel.trim() || undefined,
        status: status || undefined,
        errorCode: errorCode.trim() || undefined,
        apiKeyId: apiKeyId || undefined,
        cursor: nextCursorValue,
        pageSize,
      });
      setItems(page.items); setNextCursor(page.nextCursor ?? null);
    } catch (value) {
      setError(errorText(value));
    } finally {
      setLoading(false);
    }
  }, [accountId, apiKeyId, errorCode, groupId, model, pageSize, startedAtOrAfter, startedBefore, status, upstreamModel]);

  useEffect(() => {
    setCursor(undefined); setHistory([]); void load();
  }, [load]);

  const toggle = async (log: RequestLog) => {
    if (expanded === log.id) { setExpanded(null); return; }
    setExpanded(log.id);
    if (!attempts[log.id]) {
      try {
        const value = await aiRoutingGatewayLogAttempts(log.id);
        setAttempts((current) => ({ ...current, [log.id]: value }));
      } catch (value) {
        setError(errorText(value));
      }
    }
  };

  return (
    <div className="space-y-4" data-testid="ai-gateway-tab-logs">
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
        <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.logs.startDate")}</span><input type="date" aria-label={t("aiRoutingGateway.logs.startDate")} value={startedAtOrAfter} onChange={(event) => setStartedAtOrAfter(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.logs.endDate")}</span><input type="date" aria-label={t("aiRoutingGateway.logs.endDate")} value={startedBefore} onChange={(event) => setStartedBefore(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <select aria-label={t("aiRoutingGateway.logs.account")} value={accountId} onChange={(event) => setAccountId(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.logs.allAccounts")}</option>{data.accounts.map((account) => <option key={account.id} value={account.id}>{account.name}</option>)}</select>
        <select aria-label={t("aiRoutingGateway.logs.group")} value={groupId} onChange={(event) => setGroupId(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.logs.allGroups")}</option>{data.groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}</select>
        <select aria-label={t("aiRoutingGateway.filters.model")} value={model} onChange={(event) => setModel(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.filters.allModels")}</option>{data.models.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}</select>
        <input aria-label={t("aiRoutingGateway.logs.upstreamModel")} value={upstreamModel} onChange={(event) => setUpstreamModel(event.target.value)} placeholder={t("aiRoutingGateway.logs.upstreamModel")} className="h-9 rounded-md border bg-background px-3 text-sm" />
        <select aria-label={t("aiRoutingGateway.logs.status")} value={status} onChange={(event) => setStatus(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.logs.allStatuses")}</option>{["succeeded", "failed", "cancelled", "interrupted"].map((value) => <option key={value}>{value}</option>)}</select>
        <input aria-label={t("aiRoutingGateway.logs.errorCode")} value={errorCode} onChange={(event) => setErrorCode(event.target.value)} placeholder={t("aiRoutingGateway.logs.errorCode")} className="h-9 rounded-md border bg-background px-3 text-sm" />
        <select aria-label={t("aiRoutingGateway.logs.apiKey")} value={apiKeyId} onChange={(event) => setApiKeyId(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.logs.allApiKeys")}</option>{data.keys.map((key) => <option key={key.id} value={key.id}>{key.name}</option>)}</select>
        <select aria-label={t("aiRoutingGateway.logs.pageSize")} value={pageSize} onChange={(event) => setPageSize(Number(event.target.value))} className="h-9 rounded-md border bg-background px-3 text-sm">{[10, 25, 50, 100].map((size) => <option key={size} value={size}>{size}</option>)}</select>
        <button type="button" onClick={async () => { if (!window.confirm(t("aiRoutingGateway.logs.clearConfirm"))) return; try { await aiRoutingGatewayLogsClear(); setCursor(undefined); setHistory([]); await load(); } catch (value) { setError(errorText(value)); } }} className="inline-flex h-9 items-center justify-center gap-2 rounded-md border px-3 text-sm text-destructive"><Trash2 className="h-4 w-4" />{t("aiRoutingGateway.logs.clear")}</button>
      </div>
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      {loading ? <div className="flex items-center justify-center gap-2 rounded-md border border-dashed p-10 text-sm text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" />{t("aiRoutingGateway.common.loading")}</div> : items.length === 0 ? <div className="rounded-md border border-dashed p-10 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.logs.empty")}</div> : <div className="overflow-x-auto rounded-md border"><table className="w-full min-w-[1120px] text-left text-xs"><thead className="bg-muted/40 text-muted-foreground"><tr><th className="px-3 py-2">{t("aiRoutingGateway.logs.time")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.model")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.upstreamModel")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.account")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.group")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.apiKey")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.status")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.errorCode")}</th><th className="px-3 py-2 text-right">{t("aiRoutingGateway.logs.tokens")}</th><th className="px-3 py-2 text-right">{t("aiRoutingGateway.logs.cost")}</th></tr></thead><tbody>{items.map((log) => <Fragment key={log.id}><tr className="border-t align-top"><td colSpan={10} className="p-0"><button type="button" onClick={() => void toggle(log)} className="grid w-full grid-cols-[10rem_8rem_9rem_9rem_8rem_8rem_7rem_9rem_6rem_7rem] gap-2 px-3 py-2 text-left"><span>{new Date(log.started_at).toLocaleString()}</span><span className="truncate">{log.public_model_id}</span><span className="truncate">{log.upstream_model_id_snapshot ?? "-"}</span><span className="truncate">{log.account_name_snapshot ?? "-"}</span><span className="truncate">{log.group_name_snapshot ?? "-"}</span><span className="truncate">{log.api_key_name_snapshot ?? "-"}</span><span>{log.status}</span><span className="truncate">{log.error_code ?? t("aiRoutingGateway.logs.noError")}</span><span className="text-right">{compact(log.usage.total_tokens)}</span><span className="text-right">{log.cost_calculable ? `$${log.estimated_cost_usd ?? "-"}` : t("aiRoutingGateway.common.notCalculable")}</span></button>{expanded === log.id ? <div className="border-t bg-muted/15 px-4 py-3"><div className="mb-2 font-mono text-[11px] text-muted-foreground">{log.request_id} · {log.endpoint} · {log.error_code ?? t("aiRoutingGateway.logs.noError")}</div>{attempts[log.id]?.length ? <div className="space-y-1">{attempts[log.id].map((attempt) => <div key={attempt.id} className="flex flex-wrap gap-x-3 text-[11px]"><span>#{attempt.attempt_number}</span><span>{attempt.account_name_snapshot}</span><span>{attempt.upstream_model_id_snapshot ?? "-"}</span><span>{attempt.status}</span><span>{attempt.error_code ?? t("aiRoutingGateway.logs.noError")}</span><span>{attempt.emitted_client_bytes ? t("aiRoutingGateway.logs.streamStarted") : ""}</span></div>)}</div> : <div className="text-xs text-muted-foreground">{t("aiRoutingGateway.logs.noAttempts")}</div>}</div> : null}</td></tr></Fragment>)}</tbody></table></div>}
      <div className="flex justify-end gap-2"><button type="button" disabled={history.length === 0 || loading} onClick={() => { const previous = history.at(-1); setHistory((current) => current.slice(0, -1)); setCursor(previous); void load(previous); }} className="h-8 w-8 rounded-md border disabled:opacity-40" title={t("aiRoutingGateway.logs.previous")}><ChevronLeft className="mx-auto h-4 w-4" /></button><button type="button" disabled={!nextCursor || loading} onClick={() => { setHistory((current) => [...current, cursor]); setCursor(nextCursor ?? undefined); void load(nextCursor ?? undefined); }} className="h-8 w-8 rounded-md border disabled:opacity-40" title={t("aiRoutingGateway.logs.next")}><ChevronRight className="mx-auto h-4 w-4" /></button></div>
    </div>
  );
}

function SettingsTab({ data, reload }: { data: GatewayBootstrap; reload: () => Promise<void> }) {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<GatewaySettings>(data.settings);
  const [prices, setPrices] = useState<PriceRecord[]>([]);
  const [priceModel, setPriceModel] = useState(data.models[0]?.id ?? "");
  const [priceAccount, setPriceAccount] = useState("");
  const [priceInput, setPriceInput] = useState("");
  const [priceOutput, setPriceOutput] = useState("");
  const [priceCacheRead, setPriceCacheRead] = useState("");
  const [priceCacheWrite, setPriceCacheWrite] = useState("");
  const [startDate, setStartDate] = useState(dateInput(new Date(Date.now() - 30 * 86400000)));
  const [endDate, setEndDate] = useState(dateInput());
  const [maintenance, setMaintenance] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const apiKeyAccounts = data.accounts.filter((account) => account.account_type === "api_key");

  useEffect(() => { setSettings(data.settings); }, [data.settings]);
  useEffect(() => { void aiRoutingGatewayPricesList().then(setPrices).catch((value) => setError(errorText(value))); }, []);

  const save = async () => {
    setBusy(true); setError("");
    try { await aiRoutingGatewaySettingsSave(settings); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };

  const runMaintenance = async (operation: "optimize" | "cleanup" | "rebuild" | "validate") => {
    setBusy(true); setError(""); setMaintenance(t("aiRoutingGateway.settings.maintenanceRunning"));
    try { const value = await aiRoutingGatewayMaintenanceRun(operation, startDate, endDate); setMaintenance(t("aiRoutingGateway.settings.maintenanceDone", { rows: value.affectedRows, mismatches: value.mismatchedRows ?? 0 })); } catch (value) { setError(errorText(value)); setMaintenance(""); } finally { setBusy(false); }
  };

  const savePrice = async () => {
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayPriceSave({
        publicModelId: priceModel,
        accountId: priceAccount || null,
        effectiveAt: new Date().toISOString(),
        inputPerMillionUsd: priceInput || null,
        outputPerMillionUsd: priceOutput || null,
        cacheReadPerMillionUsd: priceCacheRead || null,
        cacheWritePerMillionUsd: priceCacheWrite || null,
      });
      setPrices(await aiRoutingGatewayPricesList());
      setPriceInput(""); setPriceOutput(""); setPriceCacheRead(""); setPriceCacheWrite("");
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-5" data-testid="ai-gateway-tab-settings">
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.settings.runtime")}</h2><div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-4"><label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.settings.port")}</span><input type="number" min={1} max={65535} value={settings.port} onChange={(event) => setSettings((current) => ({ ...current, port: Number(event.target.value) }))} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label><label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.settings.threshold")}</span><input type="number" min={0} max={100} value={settings.globalQuotaThresholdPercent} onChange={(event) => setSettings((current) => ({ ...current, globalQuotaThresholdPercent: Number(event.target.value) }))} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label><label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.settings.retention")}</span><select value={settings.logRetentionDays ?? "forever"} onChange={(event) => setSettings((current) => ({ ...current, logRetentionDays: event.target.value === "forever" ? null : Number(event.target.value) as 7 | 30 | 90 | 180 }))} className="h-9 w-full rounded-md border bg-background px-3 text-sm">{[7, 30, 90, 180].map((days) => <option key={days} value={days}>{days} {t("aiRoutingGateway.common.days")}</option>)}<option value="forever">{t("aiRoutingGateway.settings.forever")}</option></select></label><label className="flex items-end gap-2 pb-2 text-sm"><input type="checkbox" checked={settings.runEnabled} onChange={(event) => setSettings((current) => ({ ...current, runEnabled: event.target.checked }))} />{t("aiRoutingGateway.settings.runEnabled")}</label></div><button type="button" onClick={() => void save()} disabled={busy} className="mt-3 inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground"><Save className="h-4 w-4" />{t("aiRoutingGateway.common.save")}</button></section>
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.settings.pricing")}</h2><p className="mt-1 text-xs text-muted-foreground">{t("aiRoutingGateway.settings.pricingHint")}</p><div className="mt-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-3"><select aria-label={t("aiRoutingGateway.settings.model")} value={priceModel} onChange={(event) => setPriceModel(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm">{data.models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}</select><select aria-label={t("aiRoutingGateway.settings.priceAccount")} value={priceAccount} onChange={(event) => setPriceAccount(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.settings.officialPrice")}</option>{apiKeyAccounts.map((account) => <option key={account.id} value={account.id}>{t("aiRoutingGateway.settings.accountOverride")}: {account.name}</option>)}</select><input aria-label={t("aiRoutingGateway.settings.inputPrice")} value={priceInput} onChange={(event) => setPriceInput(event.target.value)} placeholder={t("aiRoutingGateway.settings.inputPrice")} className="h-9 rounded-md border bg-background px-3 text-sm" /><input aria-label={t("aiRoutingGateway.settings.outputPrice")} value={priceOutput} onChange={(event) => setPriceOutput(event.target.value)} placeholder={t("aiRoutingGateway.settings.outputPrice")} className="h-9 rounded-md border bg-background px-3 text-sm" /><input aria-label={t("aiRoutingGateway.settings.cacheReadPrice")} value={priceCacheRead} onChange={(event) => setPriceCacheRead(event.target.value)} placeholder={t("aiRoutingGateway.settings.cacheReadPrice")} className="h-9 rounded-md border bg-background px-3 text-sm" /><input aria-label={t("aiRoutingGateway.settings.cacheWritePrice")} value={priceCacheWrite} onChange={(event) => setPriceCacheWrite(event.target.value)} placeholder={t("aiRoutingGateway.settings.cacheWritePrice")} className="h-9 rounded-md border bg-background px-3 text-sm" /></div><button type="button" onClick={() => void savePrice()} disabled={!priceModel || ![priceInput, priceOutput, priceCacheRead, priceCacheWrite].some(Boolean) || busy} className="mt-3 h-9 rounded-md border px-3 text-sm disabled:opacity-50">{t("aiRoutingGateway.settings.addPrice")}</button><div className="mt-3 max-h-48 overflow-y-auto text-xs">{prices.map((price, index) => <div key={`${price.public_model_id}-${price.account_id ?? "official"}-${price.effective_at}-${index}`} className="flex flex-wrap justify-between gap-3 border-t py-2"><span>{price.public_model_id} · {price.account_id ?? t("aiRoutingGateway.settings.officialPrice")} · {price.source}</span><span>${price.input_per_million_usd ?? "-"} / ${price.output_per_million_usd ?? "-"} / ${price.cache_read_per_million_usd ?? "-"} / ${price.cache_write_per_million_usd ?? "-"}</span></div>)}</div></section>
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.settings.maintenance")}</h2><div className="mt-3 flex flex-wrap gap-2"><input type="date" value={startDate} onChange={(event) => setStartDate(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm" /><input type="date" value={endDate} onChange={(event) => setEndDate(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm" />{(["optimize", "cleanup", "rebuild", "validate"] as const).map((operation) => <button key={operation} type="button" onClick={() => void runMaintenance(operation)} disabled={busy} className="h-9 rounded-md border px-3 text-sm disabled:opacity-50">{t(`aiRoutingGateway.settings.${operation}`)}</button>)}</div>{maintenance ? <div className="mt-3 flex items-center gap-2 text-sm text-muted-foreground" role="status">{busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Check className="h-4 w-4 text-emerald-600" />}{maintenance}</div> : null}</section>
    </div>
  );
}

export function AiRoutingGateway({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<TabId>("home");
  const [data, setData] = useState<GatewayBootstrap | null>(null);
  const [homepage, setHomepage] = useState<GatewayHomepage | null>(null);
  const [filters, setFilters] = useState<HomepageFilterState>({ accountId: "", groupId: "", publicModelId: "" });
  const [days, setDays] = useState<TrendDays>(7);
  const [mode, setMode] = useState<TrendMode>("tokens");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const mounted = useRef(true);
  const load = useCallback(async () => {
    setLoading(true); setError("");
    try {
      const value = await aiRoutingGatewayBootstrap(days, toHomepageFilters(filters));
      if (!mounted.current) return;
      setData(value); setHomepage(value.homepage);
    } catch (value) {
      if (mounted.current) setError(errorText(value));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [days, filters]);

  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { if (isVisible) void load(); }, [isVisible, load]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeAiRoutingGatewayEvents({
      runtime: (runtime) => setData((current) => current ? { ...current, runtime } : current),
      account: () => void load(),
      maintenance: () => undefined,
    }).then((value) => { if (disposed) value(); else unlisten = value; }).catch((value) => { if (!disposed) setError(errorText(value)); });
    return () => { disposed = true; unlisten?.(); };
  }, [load]);

  const changeDays = async (value: TrendDays) => {
    setDays(value);
    try {
      setHomepage(await aiRoutingGatewayStatsHome(value, toHomepageFilters(filters)));
    } catch (nextError) {
      setError(errorText(nextError));
    }
  };

  const toggleRuntime = async () => {
    if (!data) return;
    setRuntimeBusy(true); setError("");
    try {
      const runtime = data.runtime.state === "running" ? await aiRoutingGatewayRuntimeStop() : await aiRoutingGatewayRuntimeStart();
      setData((current) => current ? { ...current, runtime } : current);
    } catch (value) {
      setError(errorText(value));
    } finally {
      setRuntimeBusy(false);
    }
  };

  return (
    <div className="h-full overflow-y-auto bg-background" data-testid="ai-routing-gateway">
      <div className="mx-auto max-w-7xl p-4 sm:p-6">
        <header className="flex flex-col gap-3 border-b pb-4 sm:flex-row sm:items-center sm:justify-between"><div><h1 className="text-2xl font-bold">{t("aiRoutingGateway.title")}</h1><p className="mt-1 text-sm text-muted-foreground">{t("aiRoutingGateway.description")}</p></div><div className="flex items-center gap-2"><span className={`inline-flex h-8 items-center gap-2 rounded-md border px-3 text-xs ${data?.runtime.state === "running" ? "border-emerald-500/30 text-emerald-600" : "text-muted-foreground"}`}><span className={`h-2 w-2 rounded-full ${data?.runtime.state === "running" ? "bg-emerald-500" : data?.runtime.state === "error" || data?.runtime.state === "locked" ? "bg-amber-500" : "bg-muted-foreground"}`} />{data ? t(`aiRoutingGateway.states.${data.runtime.state}`) : t("aiRoutingGateway.common.loading")}</span><button type="button" onClick={() => void load()} disabled={loading} className="h-9 w-9 rounded-md border" title={t("aiRoutingGateway.common.refresh")}><RefreshCw className={`mx-auto h-4 w-4 ${loading ? "animate-spin" : ""}`} /></button><button type="button" onClick={() => void toggleRuntime()} disabled={!data || runtimeBusy || data.runtime.state === "locked"} className="h-9 w-9 rounded-md border disabled:opacity-40" title={data?.runtime.state === "running" ? t("aiRoutingGateway.common.stop") : t("aiRoutingGateway.common.start")}>{runtimeBusy ? <Loader2 className="mx-auto h-4 w-4 animate-spin" /> : data?.runtime.state === "running" ? <Square className="mx-auto h-4 w-4" /> : <Play className="mx-auto h-4 w-4" />}</button></div></header>
        <nav className="my-4 flex overflow-x-auto border-b" aria-label={t("aiRoutingGateway.tabs.label")}>{TABS.map(({ id, icon: Icon }) => <button key={id} type="button" onClick={() => setTab(id)} className={`flex h-10 shrink-0 items-center gap-2 border-b-2 px-4 text-sm ${tab === id ? "border-primary font-medium text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"}`} aria-current={tab === id ? "page" : undefined}><Icon className="h-4 w-4" />{t(`aiRoutingGateway.tabs.${id}`)}</button>)}</nav>
        {error ? <div className="mb-4 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive" role="alert"><AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" /><div className="min-w-0 flex-1"><div className="font-medium">{t("aiRoutingGateway.common.loadFailed")}</div><div className="break-words text-xs">{error}</div></div><button type="button" onClick={() => void load()} className="h-8 rounded-md border px-2">{t("aiRoutingGateway.common.retry")}</button></div> : null}
        {loading && !data ? <div className="flex items-center justify-center gap-2 rounded-md border border-dashed p-16 text-sm text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" />{t("aiRoutingGateway.common.loading")}</div> : data && homepage ? <div className="space-y-4"><StatusBanner data={data} />{tab === "home" ? <HomeTab data={data} homepage={homepage} days={days} mode={mode} filters={filters} onDays={(value) => void changeDays(value)} onMode={setMode} onFilters={setFilters} /> : null}{tab === "accounts" ? <AccountsTab data={data} reload={load} /> : null}{tab === "keys" ? <KeysTab data={data} reload={load} /> : null}{tab === "logs" ? <LogsTab data={data} /> : null}{tab === "settings" ? <SettingsTab data={data} reload={load} /> : null}</div> : !error ? <div className="rounded-md border border-dashed p-12 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.common.empty")}</div> : null}
      </div>
    </div>
  );
}
