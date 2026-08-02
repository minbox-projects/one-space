import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertTriangle,
  BarChart3,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Clipboard,
  KeyRound,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  RotateCw,
  Save,
  Settings,
  ShieldAlert,
  Square,
  Trash2,
  Users,
  X,
} from "lucide-react";
import {
  aiRoutingGatewayAccountCreateApiKey,
  aiRoutingGatewayAccountDelete,
  aiRoutingGatewayAccountDeleteConfirmation,
  aiRoutingGatewayAccountUpdate,
  aiRoutingGatewayBootstrap,
  aiRoutingGatewayGroupCreate,
  aiRoutingGatewayKeyCreate,
  aiRoutingGatewayKeyRegenerate,
  aiRoutingGatewayKeyRevoke,
  aiRoutingGatewayKeySetEnabled,
  aiRoutingGatewayLogAttempts,
  aiRoutingGatewayLogsClear,
  aiRoutingGatewayLogsQuery,
  aiRoutingGatewayMaintenanceRun,
  aiRoutingGatewayMappingList,
  aiRoutingGatewayMappingSave,
  aiRoutingGatewayOAuthBegin,
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
  type ModelMapping,
  type PriceRecord,
  type QuotaWindow,
  type RequestAttempt,
  type RequestLog,
} from "@/lib/aiRoutingGateway";

type TabId = "home" | "accounts" | "keys" | "logs" | "settings";
type TrendDays = 7 | 15 | 30;
type TrendMode = "tokens" | "cost";

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
  return new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value ?? 0);
}

function dateInput(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
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
  onDays,
  onMode,
}: {
  data: GatewayBootstrap;
  homepage: GatewayHomepage;
  days: TrendDays;
  mode: TrendMode;
  onDays: (days: TrendDays) => void;
  onMode: (mode: TrendMode) => void;
}) {
  const { t } = useTranslation();
  const [account, setAccount] = useState("");
  const [group, setGroup] = useState("");
  const [model, setModel] = useState("");
  const max = Math.max(
    1,
    ...homepage.trend.map((point) =>
      mode === "tokens" ? point.usage.totalTokens ?? 0 : Number(point.estimatedCostUsd ?? 0),
    ),
  );
  return (
    <div className="space-y-5" data-testid="ai-gateway-tab-home">
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
        <select aria-label={t("aiRoutingGateway.filters.account")} value={account} onChange={(event) => setAccount(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm">
          <option value="">{t("aiRoutingGateway.filters.allAccounts")}</option>
          {data.accounts.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
        <select aria-label={t("aiRoutingGateway.filters.group")} value={group} onChange={(event) => setGroup(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm">
          <option value="">{t("aiRoutingGateway.filters.allGroups")}</option>
          {data.groups.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
        <select aria-label={t("aiRoutingGateway.filters.model")} value={model} onChange={(event) => setModel(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm">
          <option value="">{t("aiRoutingGateway.filters.allModels")}</option>
          {data.models.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}
        </select>
      </div>
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Metric label={t("aiRoutingGateway.home.accounts")} value={`${homepage.availableCount}/${homepage.accountCount}`} detail={t("aiRoutingGateway.home.available")} />
        <Metric label={t("aiRoutingGateway.home.quotaWindows")} value={homepage.staleCount ? t("aiRoutingGateway.home.staleCount", { count: homepage.staleCount }) : t("aiRoutingGateway.home.fresh")} detail={t("aiRoutingGateway.home.windowTypes")} />
        <Metric label={t("aiRoutingGateway.home.todayTokens")} value={compact(homepage.today.usage.totalTokens)} detail={`${t("aiRoutingGateway.home.input")} ${compact(homepage.today.usage.inputTokens)} · ${t("aiRoutingGateway.home.output")} ${compact(homepage.today.usage.outputTokens)}`} />
        <Metric label={t("aiRoutingGateway.home.estimatedCost")} value={homepage.today.costCalculable ? `$${homepage.today.estimatedCostUsd ?? "0"}` : t("aiRoutingGateway.common.notCalculable")} detail={t("aiRoutingGateway.home.costDisclaimer")} />
      </div>
      <section className="rounded-md border">
        <div className="flex flex-col gap-3 border-b px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <h2 className="text-sm font-semibold">{t("aiRoutingGateway.home.trend")}</h2>
          <div className="flex flex-wrap gap-2">
            <div className="flex rounded-md border p-0.5">
              {([7, 15, 30] as TrendDays[]).map((value) => <button key={value} type="button" onClick={() => onDays(value)} className={`h-7 px-2 text-xs ${days === value ? "rounded bg-muted font-medium" : "text-muted-foreground"}`}>{value}{t("aiRoutingGateway.common.daysSuffix")}</button>)}
            </div>
            <div className="flex rounded-md border p-0.5">
              {(["tokens", "cost"] as TrendMode[]).map((value) => <button key={value} type="button" onClick={() => onMode(value)} className={`h-7 px-2 text-xs ${mode === value ? "rounded bg-muted font-medium" : "text-muted-foreground"}`}>{t(`aiRoutingGateway.home.${value}`)}</button>)}
            </div>
          </div>
        </div>
        {homepage.trend.every((point) => point.requestCount === 0) ? (
          <div className="p-8 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.home.emptyTrend")}</div>
        ) : (
          <div className="flex h-44 items-end gap-1 px-4 pb-4 pt-8" aria-label={t("aiRoutingGateway.home.trend")}>
            {homepage.trend.map((point) => {
              const value = mode === "tokens" ? point.usage.totalTokens ?? 0 : Number(point.estimatedCostUsd ?? 0);
              return <div key={point.localDate} className="group flex min-w-0 flex-1 flex-col items-center justify-end gap-1" title={`${point.localDate}: ${value}`}><div className="w-full max-w-7 rounded-t-sm bg-primary/70" style={{ height: `${Math.max(2, (value / max) * 112)}px` }} /><span className="hidden text-[10px] text-muted-foreground sm:block">{point.localDate.slice(5)}</span></div>;
            })}
          </div>
        )}
      </section>
    </div>
  );
}

function AccountDetail({ account, data, onChanged }: { account: GatewayAccount; data: GatewayBootstrap; onChanged: () => Promise<void> }) {
  const { t } = useTranslation();
  const [name, setName] = useState(account.name);
  const [groupId, setGroupId] = useState(account.group_id);
  const [note, setNote] = useState(account.note);
  const [tags, setTags] = useState(account.tags.join(", "));
  const [threshold, setThreshold] = useState(account.quota_threshold_override_percent?.toString() ?? "");
  const [quotas, setQuotas] = useState<QuotaWindow[]>([]);
  const [mappings, setMappings] = useState<ModelMapping[]>([]);
  const [mappingModel, setMappingModel] = useState(data.models[0]?.id ?? "");
  const [upstreamModel, setUpstreamModel] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    void Promise.all([aiRoutingGatewayQuotaList(account.id), aiRoutingGatewayMappingList(account.id)])
      .then(([nextQuotas, nextMappings]) => { setQuotas(nextQuotas); setMappings(nextMappings); })
      .catch((value) => setError(errorText(value)));
  }, [account.id]);
  const save = async () => {
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayAccountUpdate({
        accountId: account.id, name, groupId, sortOrder: account.sort_order, note,
        enabled: account.enabled,
        quotaThresholdOverridePercent: threshold === "" ? null : Number(threshold),
        tags: tags.split(",").map((value) => value.trim()).filter(Boolean),
      });
      await onChanged();
    } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  const saveMapping = async () => {
    if (!mappingModel || !upstreamModel.trim()) return;
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayMappingSave({ accountId: account.id, publicModelId: mappingModel, upstreamModelId: upstreamModel.trim(), enabled: true });
      setMappings(await aiRoutingGatewayMappingList(account.id)); setUpstreamModel("");
    } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  return (
    <div className="space-y-4 border-t bg-muted/15 p-4">
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      <div className="grid gap-3 md:grid-cols-2">
        <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.name")}</span><input value={name} onChange={(event) => setName(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.filters.group")}</span><select value={groupId} onChange={(event) => setGroupId(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm">{data.groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}</select></label>
        <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.tags")}</span><input value={tags} onChange={(event) => setTags(event.target.value)} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.accounts.threshold")}</span><input type="number" min={0} max={100} value={threshold} onChange={(event) => setThreshold(event.target.value)} placeholder={t("aiRoutingGateway.accounts.inherit")} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label>
        <label className="space-y-1 text-xs md:col-span-2"><span>{t("aiRoutingGateway.accounts.note")}</span><textarea value={note} onChange={(event) => setNote(event.target.value)} className="min-h-20 w-full rounded-md border bg-background p-3 text-sm" /></label>
      </div>
      <button type="button" onClick={save} disabled={busy || !name.trim()} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50"><Save className="h-4 w-4" />{t("aiRoutingGateway.common.save")}</button>
      <div className="grid gap-4 lg:grid-cols-2">
        <section className="rounded-md border bg-background p-3">
          <h3 className="text-sm font-semibold">{t("aiRoutingGateway.accounts.quota")}</h3>
          {account.account_type !== "oauth" ? <p className="mt-3 text-sm text-muted-foreground">{t("aiRoutingGateway.accounts.quotaOAuthOnly")}</p> : quotas.length === 0 ? <p className="mt-3 text-sm text-muted-foreground">{t("aiRoutingGateway.accounts.noQuota")}</p> : <div className="mt-3 space-y-2">{quotas.map((quota) => <div key={quota.id} className="flex items-center justify-between gap-2 text-sm"><span className="truncate">{quota.name} {quota.is_stale ? `· ${t("aiRoutingGateway.accounts.stale")}` : ""}</span><span className="font-medium">{quota.remaining_percent == null ? "-" : `${quota.remaining_percent}%`}</span></div>)}</div>}
        </section>
        <section className="rounded-md border bg-background p-3">
          <h3 className="text-sm font-semibold">{t("aiRoutingGateway.accounts.mappings")}</h3>
          <div className="mt-3 flex gap-2"><select value={mappingModel} onChange={(event) => setMappingModel(event.target.value)} className="h-9 min-w-0 flex-1 rounded-md border bg-background px-2 text-sm">{data.models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}</select><input value={upstreamModel} onChange={(event) => setUpstreamModel(event.target.value)} placeholder={t("aiRoutingGateway.accounts.upstreamModel")} className="h-9 min-w-0 flex-1 rounded-md border bg-background px-2 text-sm" /><button type="button" onClick={saveMapping} className="h-9 w-9 rounded-md border" title={t("aiRoutingGateway.common.add")}><Plus className="mx-auto h-4 w-4" /></button></div>
          <div className="mt-3 space-y-1">{mappings.length === 0 ? <p className="text-sm text-muted-foreground">{t("aiRoutingGateway.accounts.noMappings")}</p> : mappings.map((mapping) => <div key={mapping.public_model_id} className="flex justify-between gap-2 text-xs"><span>{mapping.public_model_id}</span><span className="truncate text-muted-foreground">{mapping.upstream_model_id}</span></div>)}</div>
        </section>
      </div>
    </div>
  );
}

function AccountsTab({ data, reload }: { data: GatewayBootstrap; reload: () => Promise<void> }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState<string | null>(null);
  const [tagFilter, setTagFilter] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [protocol, setProtocol] = useState<"responses" | "chat_completions">("responses");
  const [groupName, setGroupName] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const tags = [...new Set(data.accounts.flatMap((account) => account.tags))].sort();
  const visible = tagFilter ? data.accounts.filter((account) => account.tags.includes(tagFilter)) : data.accounts;
  const create = async () => {
    setBusy(true); setError("");
    try {
      await aiRoutingGatewayAccountCreateApiKey({ name, baseUrl, apiKey, authMethod: "bearer", upstreamProtocol: protocol, note: "" });
      setApiKey(""); setName(""); setBaseUrl(""); setShowCreate(false); await reload();
    } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  const oauth = async (method: "loopback" | "manual" | "device_code") => {
    setBusy(true); setError("");
    try { await aiRoutingGatewayOAuthBegin(method); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  const toggle = async (account: GatewayAccount) => {
    setBusy(true); setError("");
    try { await aiRoutingGatewayAccountUpdate({ accountId: account.id, name: account.name, groupId: account.group_id, sortOrder: account.sort_order, note: account.note, enabled: !account.enabled, quotaThresholdOverridePercent: account.quota_threshold_override_percent, tags: account.tags }); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  const move = async (account: GatewayAccount, delta: number) => {
    setBusy(true); setError("");
    try { await aiRoutingGatewayAccountUpdate({ accountId: account.id, name: account.name, groupId: account.group_id, sortOrder: account.sort_order + delta, note: account.note, enabled: account.enabled, quotaThresholdOverridePercent: account.quota_threshold_override_percent, tags: account.tags }); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  const remove = async (account: GatewayAccount) => {
    if (!window.confirm(t("aiRoutingGateway.accounts.deleteConfirm", { name: account.name }))) return;
    setBusy(true); setError("");
    try { const token = await aiRoutingGatewayAccountDeleteConfirmation(account.id); await aiRoutingGatewayAccountDelete(account.id, token); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  return (
    <div className="space-y-4" data-testid="ai-gateway-tab-accounts">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <select aria-label={t("aiRoutingGateway.accounts.filterTag")} value={tagFilter} onChange={(event) => setTagFilter(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.accounts.allTags")}</option>{tags.map((tag) => <option key={tag}>{tag}</option>)}</select>
        <div className="flex flex-wrap gap-2"><button type="button" onClick={() => setShowCreate((value) => !value)} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground"><Plus className="h-4 w-4" />{t("aiRoutingGateway.accounts.addThirdParty")}</button>{(["loopback", "manual", "device_code"] as const).map((method) => <button key={method} type="button" onClick={() => oauth(method)} disabled={busy} className="h-9 rounded-md border px-3 text-sm">{t(`aiRoutingGateway.accounts.oauth.${method}`)}</button>)}</div>
      </div>
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      {data.oauthReleaseBlockReason ? <div className="rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-700 dark:text-amber-300">{t("aiRoutingGateway.accounts.oauthBlocked")}</div> : null}
      {showCreate ? <div className="grid gap-3 rounded-md border p-4 md:grid-cols-2"><input aria-label={t("aiRoutingGateway.accounts.name")} value={name} onChange={(event) => setName(event.target.value)} placeholder={t("aiRoutingGateway.accounts.name")} className="h-9 rounded-md border bg-background px-3 text-sm" /><input aria-label={t("aiRoutingGateway.accounts.baseUrl")} value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://api.example.com/v1" className="h-9 rounded-md border bg-background px-3 text-sm" /><input aria-label={t("aiRoutingGateway.accounts.apiKey")} type="password" autoComplete="off" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={t("aiRoutingGateway.accounts.apiKey")} className="h-9 rounded-md border bg-background px-3 text-sm" /><select value={protocol} onChange={(event) => setProtocol(event.target.value as typeof protocol)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="responses">Responses</option><option value="chat_completions">Chat Completions</option></select><div className="flex gap-2 md:col-span-2"><button type="button" onClick={create} disabled={busy || !name.trim() || !baseUrl.trim() || !apiKey} className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50">{t("aiRoutingGateway.common.create")}</button><button type="button" onClick={() => { setShowCreate(false); setApiKey(""); }} className="h-9 rounded-md border px-3 text-sm">{t("aiRoutingGateway.common.cancel")}</button></div></div> : null}
      <div className="flex gap-2 rounded-md border p-3"><input value={groupName} onChange={(event) => setGroupName(event.target.value)} placeholder={t("aiRoutingGateway.accounts.newGroup")} className="h-9 min-w-0 flex-1 rounded-md border bg-background px-3 text-sm" /><button type="button" onClick={async () => { if (!groupName.trim()) return; setBusy(true); try { await aiRoutingGatewayGroupCreate({ name: groupName.trim(), sortOrder: data.groups.length }); setGroupName(""); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); } }} className="h-9 rounded-md border px-3 text-sm">{t("aiRoutingGateway.accounts.addGroup")}</button></div>
      {visible.length === 0 ? <div className="rounded-md border border-dashed p-10 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.accounts.empty")}</div> : <div className="overflow-hidden rounded-md border">{visible.map((account) => <div key={account.id} className="border-b last:border-0"><div className="flex flex-wrap items-center gap-3 p-3"><button type="button" onClick={() => setExpanded(expanded === account.id ? null : account.id)} className="flex min-w-0 flex-1 items-center gap-3 text-left"><ChevronDown className={`h-4 w-4 shrink-0 transition-transform ${expanded === account.id ? "rotate-180" : ""}`} /><div className="min-w-0"><div className="truncate text-sm font-medium">{account.name}</div><div className="truncate text-xs text-muted-foreground">{account.account_type === "oauth" ? "OAuth" : "API Key"} · {account.health_status} · {account.tags.join(", ") || t("aiRoutingGateway.accounts.noTags")}</div></div></button><button type="button" onClick={() => move(account, -1)} className="h-8 w-8 rounded-md border" title={t("aiRoutingGateway.accounts.moveUp")}><ChevronLeft className="mx-auto h-4 w-4 rotate-90" /></button><button type="button" onClick={() => move(account, 1)} className="h-8 w-8 rounded-md border" title={t("aiRoutingGateway.accounts.moveDown")}><ChevronRight className="mx-auto h-4 w-4 rotate-90" /></button><button type="button" onClick={() => toggle(account)} className={`h-8 rounded-md border px-3 text-xs ${account.enabled ? "text-emerald-600" : "text-muted-foreground"}`}>{account.enabled ? t("aiRoutingGateway.common.enabled") : t("aiRoutingGateway.common.disabled")}</button><button type="button" onClick={() => remove(account)} className="h-8 w-8 rounded-md border text-destructive" title={t("aiRoutingGateway.accounts.delete")}><Trash2 className="mx-auto h-4 w-4" /></button></div>{expanded === account.id ? <AccountDetail account={account} data={data} onChanged={reload} /> : null}</div>)}</div>}
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
    try { const value = await aiRoutingGatewayKeyCreate({ name, groupIds: groups, modelIds: models, expiresAt: expiresAt ? new Date(`${expiresAt}T23:59:59`).toISOString() : null }); setPlaintext(value.plaintext); setName(""); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  const regenerate = async (key: GatewayKeyRecord) => {
    if (!window.confirm(t("aiRoutingGateway.keys.regenerateConfirm", { name: key.name }))) return;
    setBusy(true); setError(""); setPlaintext(null);
    try { const value = await aiRoutingGatewayKeyRegenerate(key.id); setPlaintext(value.plaintext); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  return (
    <div className="space-y-4" data-testid="ai-gateway-tab-keys">
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      {plaintext ? <div className="rounded-md border border-amber-500/30 bg-amber-500/10 p-4"><div className="flex items-center justify-between gap-2"><div><div className="text-sm font-semibold">{t("aiRoutingGateway.keys.oneTimeTitle")}</div><div className="text-xs text-muted-foreground">{t("aiRoutingGateway.keys.oneTimeHint")}</div></div><button type="button" onClick={() => setPlaintext(null)} className="h-8 w-8 rounded-md" title={t("aiRoutingGateway.common.close")}><X className="mx-auto h-4 w-4" /></button></div><div className="mt-3 flex gap-2"><code className="min-w-0 flex-1 overflow-x-auto rounded-md border bg-background px-3 py-2 text-xs select-text">{plaintext}</code><button type="button" onClick={async () => { await navigator.clipboard.writeText(plaintext); setCopied(true); window.setTimeout(() => setCopied(false), 1500); }} className="h-9 w-9 rounded-md border bg-background" title={t("aiRoutingGateway.common.copy")}>{copied ? <Check className="mx-auto h-4 w-4" /> : <Clipboard className="mx-auto h-4 w-4" />}</button></div></div> : null}
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.keys.create")}</h2><div className="mt-3 grid gap-3 md:grid-cols-2"><input value={name} onChange={(event) => setName(event.target.value)} placeholder={t("aiRoutingGateway.keys.name")} className="h-9 rounded-md border bg-background px-3 text-sm" /><input type="date" value={expiresAt} onChange={(event) => setExpiresAt(event.target.value)} aria-label={t("aiRoutingGateway.keys.expiresAt")} className="h-9 rounded-md border bg-background px-3 text-sm" /><fieldset className="rounded-md border p-3"><legend className="px-1 text-xs">{t("aiRoutingGateway.keys.groupPermissions")}</legend>{data.groups.map((group) => <label key={group.id} className="mr-4 inline-flex items-center gap-2 text-sm"><input type="checkbox" checked={groups.includes(group.id)} onChange={(event) => setGroups((current) => event.target.checked ? [...current, group.id] : current.filter((id) => id !== group.id))} />{group.name}</label>)}</fieldset><fieldset className="rounded-md border p-3"><legend className="px-1 text-xs">{t("aiRoutingGateway.keys.modelPermissions")}</legend>{data.models.map((model) => <label key={model.id} className="mr-4 inline-flex items-center gap-2 text-sm"><input type="checkbox" checked={models.includes(model.id)} onChange={(event) => setModels((current) => event.target.checked ? [...current, model.id] : current.filter((id) => id !== model.id))} />{model.displayName}</label>)}</fieldset></div><button type="button" onClick={create} disabled={busy || !name.trim() || groups.length === 0 || models.length === 0} className="mt-3 inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50"><Plus className="h-4 w-4" />{t("aiRoutingGateway.common.create")}</button></section>
      {data.keys.length === 0 ? <div className="rounded-md border border-dashed p-10 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.keys.empty")}</div> : <div className="overflow-hidden rounded-md border">{data.keys.map((key) => { const expired = !!key.expiresAt && new Date(key.expiresAt) <= new Date(); const revoked = !!key.revokedAt; return <div key={key.id} className="flex flex-wrap items-center gap-3 border-b p-3 last:border-0"><div className="min-w-0 flex-1"><div className="truncate text-sm font-medium">{key.name}</div><div className="truncate font-mono text-xs text-muted-foreground">{key.keyPrefix}… · {revoked ? t("aiRoutingGateway.keys.revoked") : expired ? t("aiRoutingGateway.keys.expired") : key.enabled ? t("aiRoutingGateway.common.enabled") : t("aiRoutingGateway.common.disabled")}</div><div className="mt-1 truncate text-xs text-muted-foreground">{key.groupIds.join(", ")} · {key.modelIds.join(", ")}</div></div><button type="button" disabled={revoked} onClick={async () => { setBusy(true); try { await aiRoutingGatewayKeySetEnabled(key.id, !key.enabled); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); } }} className="h-8 rounded-md border px-3 text-xs">{key.enabled ? t("aiRoutingGateway.keys.disable") : t("aiRoutingGateway.keys.enable")}</button><button type="button" onClick={() => regenerate(key)} className="h-8 w-8 rounded-md border" title={t("aiRoutingGateway.keys.regenerate")}><RotateCw className="mx-auto h-4 w-4" /></button><button type="button" disabled={revoked} onClick={async () => { if (!window.confirm(t("aiRoutingGateway.keys.revokeConfirm", { name: key.name }))) return; setBusy(true); try { await aiRoutingGatewayKeyRevoke(key.id); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); } }} className="h-8 w-8 rounded-md border text-destructive" title={t("aiRoutingGateway.keys.revoke")}><Trash2 className="mx-auto h-4 w-4" /></button></div>; })}</div>}
    </div>
  );
}

function LogsTab({ data }: { data: GatewayBootstrap }) {
  const { t } = useTranslation();
  const [status, setStatus] = useState("");
  const [model, setModel] = useState("");
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
    try { const page = await aiRoutingGatewayLogsQuery({ status: status || undefined, publicModelId: model || undefined, cursor: nextCursorValue, pageSize: 25 }); setItems(page.items); setNextCursor(page.nextCursor ?? null); } catch (value) { setError(errorText(value)); } finally { setLoading(false); }
  }, [model, status]);
  useEffect(() => { setCursor(undefined); setHistory([]); void load(); }, [load]);
  const toggle = async (log: RequestLog) => {
    if (expanded === log.id) { setExpanded(null); return; }
    setExpanded(log.id);
    if (!attempts[log.id]) try { const value = await aiRoutingGatewayLogAttempts(log.id); setAttempts((current) => ({ ...current, [log.id]: value })); } catch (value) { setError(errorText(value)); }
  };
  return (
    <div className="space-y-4" data-testid="ai-gateway-tab-logs">
      <div className="flex flex-wrap gap-2"><select aria-label={t("aiRoutingGateway.logs.status")} value={status} onChange={(event) => setStatus(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.logs.allStatuses")}</option>{["succeeded", "failed", "cancelled", "interrupted"].map((value) => <option key={value}>{value}</option>)}</select><select aria-label={t("aiRoutingGateway.filters.model")} value={model} onChange={(event) => setModel(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm"><option value="">{t("aiRoutingGateway.filters.allModels")}</option>{data.models.map((item) => <option key={item.id} value={item.id}>{item.displayName}</option>)}</select><button type="button" onClick={async () => { if (!window.confirm(t("aiRoutingGateway.logs.clearConfirm"))) return; try { await aiRoutingGatewayLogsClear(); await load(); } catch (value) { setError(errorText(value)); } }} className="ml-auto inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm text-destructive"><Trash2 className="h-4 w-4" />{t("aiRoutingGateway.logs.clear")}</button></div>
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      {loading ? <div className="flex items-center justify-center gap-2 rounded-md border border-dashed p-10 text-sm text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" />{t("aiRoutingGateway.common.loading")}</div> : items.length === 0 ? <div className="rounded-md border border-dashed p-10 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.logs.empty")}</div> : <div className="overflow-x-auto rounded-md border"><table className="w-full min-w-[760px] text-left text-xs"><thead className="bg-muted/40 text-muted-foreground"><tr><th className="px-3 py-2">{t("aiRoutingGateway.logs.time")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.model")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.account")}</th><th className="px-3 py-2">{t("aiRoutingGateway.logs.status")}</th><th className="px-3 py-2 text-right">{t("aiRoutingGateway.logs.tokens")}</th><th className="px-3 py-2 text-right">{t("aiRoutingGateway.logs.cost")}</th></tr></thead><tbody>{items.map((log) => <tr key={log.id} className="border-t align-top"><td colSpan={6} className="p-0"><button type="button" onClick={() => toggle(log)} className="grid w-full grid-cols-[9rem_1fr_1fr_7rem_6rem_6rem] gap-2 px-3 py-2 text-left"><span>{new Date(log.started_at).toLocaleString()}</span><span className="truncate">{log.public_model_id}</span><span className="truncate">{log.account_name_snapshot ?? "-"}</span><span>{log.status}</span><span className="text-right">{compact(log.usage.total_tokens)}</span><span className="text-right">{log.cost_calculable ? `$${log.estimated_cost_usd ?? "0"}` : t("aiRoutingGateway.common.notCalculable")}</span></button>{expanded === log.id ? <div className="border-t bg-muted/15 px-4 py-3"><div className="mb-2 font-mono text-[11px] text-muted-foreground">{log.request_id} · {log.endpoint} · {log.error_code ?? t("aiRoutingGateway.logs.noError")}</div>{attempts[log.id]?.length ? <div className="space-y-1">{attempts[log.id].map((attempt) => <div key={attempt.id} className="flex flex-wrap justify-between gap-2 rounded border bg-background px-3 py-2"><span>#{attempt.attempt_number} · {attempt.account_name_snapshot} · {attempt.upstream_model_id_snapshot ?? "-"}</span><span>{attempt.status}{attempt.error_code ? ` · ${attempt.error_code}` : ""}{attempt.emitted_client_bytes ? ` · ${t("aiRoutingGateway.logs.streamStarted")}` : ""}</span></div>)}</div> : <div className="text-muted-foreground">{t("aiRoutingGateway.logs.noAttempts")}</div>}</div> : null}</td></tr>)}</tbody></table></div>}
      <div className="flex justify-end gap-2"><button type="button" disabled={history.length === 0 || loading} onClick={() => { const previous = history.at(-1); setHistory((current) => current.slice(0, -1)); setCursor(previous); void load(previous); }} className="h-8 w-8 rounded-md border disabled:opacity-40" title={t("aiRoutingGateway.logs.previous")}><ChevronLeft className="mx-auto h-4 w-4" /></button><button type="button" disabled={!nextCursor || loading} onClick={() => { setHistory((current) => [...current, cursor]); setCursor(nextCursor ?? undefined); void load(nextCursor ?? undefined); }} className="h-8 w-8 rounded-md border disabled:opacity-40" title={t("aiRoutingGateway.logs.next")}><ChevronRight className="mx-auto h-4 w-4" /></button></div>
    </div>
  );
}

function SettingsTab({ data, reload }: { data: GatewayBootstrap; reload: () => Promise<void> }) {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<GatewaySettings>(data.settings);
  const [prices, setPrices] = useState<PriceRecord[]>([]);
  const [priceModel, setPriceModel] = useState(data.models[0]?.id ?? "");
  const [priceInput, setPriceInput] = useState("");
  const [priceOutput, setPriceOutput] = useState("");
  const [startDate, setStartDate] = useState(dateInput(new Date(Date.now() - 30 * 86400000)));
  const [endDate, setEndDate] = useState(dateInput());
  const [maintenance, setMaintenance] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => { void aiRoutingGatewayPricesList().then(setPrices).catch((value) => setError(errorText(value))); }, []);
  const save = async () => {
    setBusy(true); setError("");
    try { await aiRoutingGatewaySettingsSave(settings); await reload(); } catch (value) { setError(errorText(value)); } finally { setBusy(false); }
  };
  const runMaintenance = async (operation: "optimize" | "cleanup" | "rebuild" | "validate") => {
    setBusy(true); setError(""); setMaintenance(t("aiRoutingGateway.settings.maintenanceRunning"));
    try { const value = await aiRoutingGatewayMaintenanceRun(operation, startDate, endDate); setMaintenance(t("aiRoutingGateway.settings.maintenanceDone", { rows: value.affectedRows, mismatches: value.mismatchedRows ?? 0 })); } catch (value) { setError(errorText(value)); setMaintenance(""); } finally { setBusy(false); }
  };
  return (
    <div className="space-y-5" data-testid="ai-gateway-tab-settings">
      {error ? <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">{error}</div> : null}
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.settings.runtime")}</h2><div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-4"><label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.settings.port")}</span><input type="number" min={1} max={65535} value={settings.port} onChange={(event) => setSettings((current) => ({ ...current, port: Number(event.target.value) }))} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label><label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.settings.threshold")}</span><input type="number" min={0} max={100} value={settings.globalQuotaThresholdPercent} onChange={(event) => setSettings((current) => ({ ...current, globalQuotaThresholdPercent: Number(event.target.value) }))} className="h-9 w-full rounded-md border bg-background px-3 text-sm" /></label><label className="space-y-1 text-xs"><span>{t("aiRoutingGateway.settings.retention")}</span><select value={settings.logRetentionDays ?? "forever"} onChange={(event) => setSettings((current) => ({ ...current, logRetentionDays: event.target.value === "forever" ? null : Number(event.target.value) as 7 | 30 | 90 | 180 }))} className="h-9 w-full rounded-md border bg-background px-3 text-sm">{[7, 30, 90, 180].map((days) => <option key={days} value={days}>{days} {t("aiRoutingGateway.common.days")}</option>)}<option value="forever">{t("aiRoutingGateway.settings.forever")}</option></select></label><label className="flex items-end gap-2 pb-2 text-sm"><input type="checkbox" checked={settings.runEnabled} onChange={(event) => setSettings((current) => ({ ...current, runEnabled: event.target.checked }))} />{t("aiRoutingGateway.settings.runEnabled")}</label></div><button type="button" onClick={save} disabled={busy} className="mt-3 inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground"><Save className="h-4 w-4" />{t("aiRoutingGateway.common.save")}</button></section>
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.settings.pricing")}</h2><p className="mt-1 text-xs text-muted-foreground">{t("aiRoutingGateway.settings.pricingHint")}</p><div className="mt-3 grid gap-2 sm:grid-cols-3"><select value={priceModel} onChange={(event) => setPriceModel(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm">{data.models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}</select><input value={priceInput} onChange={(event) => setPriceInput(event.target.value)} placeholder={t("aiRoutingGateway.settings.inputPrice")} className="h-9 rounded-md border bg-background px-3 text-sm" /><input value={priceOutput} onChange={(event) => setPriceOutput(event.target.value)} placeholder={t("aiRoutingGateway.settings.outputPrice")} className="h-9 rounded-md border bg-background px-3 text-sm" /></div><button type="button" onClick={async () => { setBusy(true); setError(""); try { await aiRoutingGatewayPriceSave({ publicModelId: priceModel, effectiveAt: new Date().toISOString(), inputPerMillionUsd: priceInput || null, outputPerMillionUsd: priceOutput || null }); setPrices(await aiRoutingGatewayPricesList()); setPriceInput(""); setPriceOutput(""); } catch (value) { setError(errorText(value)); } finally { setBusy(false); } }} disabled={!priceModel || !priceInput || !priceOutput || busy} className="mt-3 h-9 rounded-md border px-3 text-sm disabled:opacity-50">{t("aiRoutingGateway.settings.addPrice")}</button><div className="mt-3 max-h-36 overflow-y-auto text-xs">{prices.map((price, index) => <div key={`${price.public_model_id}-${price.account_id ?? "official"}-${price.effective_at}-${index}`} className="flex justify-between gap-3 border-t py-2"><span>{price.public_model_id} · {price.source}</span><span>${price.input_per_million_usd ?? "-"} / ${price.output_per_million_usd ?? "-"}</span></div>)}</div></section>
      <section className="rounded-md border p-4"><h2 className="text-sm font-semibold">{t("aiRoutingGateway.settings.maintenance")}</h2><div className="mt-3 flex flex-wrap gap-2"><input type="date" value={startDate} onChange={(event) => setStartDate(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm" /><input type="date" value={endDate} onChange={(event) => setEndDate(event.target.value)} className="h-9 rounded-md border bg-background px-3 text-sm" />{(["optimize", "cleanup", "rebuild", "validate"] as const).map((operation) => <button key={operation} type="button" onClick={() => runMaintenance(operation)} disabled={busy} className="h-9 rounded-md border px-3 text-sm disabled:opacity-50">{t(`aiRoutingGateway.settings.${operation}`)}</button>)}</div>{maintenance ? <div className="mt-3 flex items-center gap-2 text-sm text-muted-foreground" role="status">{busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Check className="h-4 w-4 text-emerald-600" />}{maintenance}</div> : null}</section>
    </div>
  );
}

export function AiRoutingGateway({ isVisible = true }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<TabId>("home");
  const [data, setData] = useState<GatewayBootstrap | null>(null);
  const [homepage, setHomepage] = useState<GatewayHomepage | null>(null);
  const [days, setDays] = useState<TrendDays>(7);
  const [mode, setMode] = useState<TrendMode>("tokens");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const mounted = useRef(true);
  const load = useCallback(async () => {
    setLoading(true); setError("");
    try {
      const value = await aiRoutingGatewayBootstrap(days);
      if (!mounted.current) return;
      setData(value); setHomepage(value.homepage);
    } catch (value) { if (mounted.current) setError(errorText(value)); } finally { if (mounted.current) setLoading(false); }
  }, [days]);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { if (isVisible) void load(); }, [isVisible, load]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeAiRoutingGatewayEvents({
      runtime: (runtime) => setData((current) => current ? { ...current, runtime } : current),
      account: () => void load(),
      maintenance: () => undefined,
    }).then((value) => { if (disposed) value(); else unlisten = value; });
    return () => { disposed = true; unlisten?.(); };
  }, [load]);
  const changeDays = async (value: TrendDays) => {
    setDays(value);
    try { setHomepage(await aiRoutingGatewayStatsHome(value)); } catch (nextError) { setError(errorText(nextError)); }
  };
  const toggleRuntime = async () => {
    if (!data) return;
    setRuntimeBusy(true); setError("");
    try { const runtime = data.runtime.state === "running" ? await aiRoutingGatewayRuntimeStop() : await aiRoutingGatewayRuntimeStart(); setData((current) => current ? { ...current, runtime } : current); } catch (value) { setError(errorText(value)); } finally { setRuntimeBusy(false); }
  };
  return (
    <div className="h-full overflow-y-auto bg-background" data-testid="ai-routing-gateway">
      <div className="mx-auto max-w-7xl p-4 sm:p-6">
        <header className="flex flex-col gap-3 border-b pb-4 sm:flex-row sm:items-center sm:justify-between"><div><h1 className="text-2xl font-bold">{t("aiRoutingGateway.title")}</h1><p className="mt-1 text-sm text-muted-foreground">{t("aiRoutingGateway.description")}</p></div><div className="flex items-center gap-2"><span className={`inline-flex h-8 items-center gap-2 rounded-md border px-3 text-xs ${data?.runtime.state === "running" ? "border-emerald-500/30 text-emerald-600" : "text-muted-foreground"}`}><span className={`h-2 w-2 rounded-full ${data?.runtime.state === "running" ? "bg-emerald-500" : data?.runtime.state === "error" || data?.runtime.state === "locked" ? "bg-amber-500" : "bg-muted-foreground"}`} />{data ? t(`aiRoutingGateway.states.${data.runtime.state}`) : t("aiRoutingGateway.common.loading")}</span><button type="button" onClick={() => void load()} disabled={loading} className="h-9 w-9 rounded-md border" title={t("aiRoutingGateway.common.refresh")}><RefreshCw className={`mx-auto h-4 w-4 ${loading ? "animate-spin" : ""}`} /></button><button type="button" onClick={toggleRuntime} disabled={!data || runtimeBusy || data.runtime.state === "locked"} className="h-9 w-9 rounded-md border disabled:opacity-40" title={data?.runtime.state === "running" ? t("aiRoutingGateway.common.stop") : t("aiRoutingGateway.common.start")}>{runtimeBusy ? <Loader2 className="mx-auto h-4 w-4 animate-spin" /> : data?.runtime.state === "running" ? <Square className="mx-auto h-4 w-4" /> : <Play className="mx-auto h-4 w-4" />}</button></div></header>
        <nav className="my-4 flex overflow-x-auto border-b" aria-label={t("aiRoutingGateway.tabs.label")}>{TABS.map(({ id, icon: Icon }) => <button key={id} type="button" onClick={() => setTab(id)} className={`flex h-10 shrink-0 items-center gap-2 border-b-2 px-4 text-sm ${tab === id ? "border-primary font-medium text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"}`} aria-current={tab === id ? "page" : undefined}><Icon className="h-4 w-4" />{t(`aiRoutingGateway.tabs.${id}`)}</button>)}</nav>
        {error ? <div className="mb-4 flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive" role="alert"><AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" /><div className="min-w-0 flex-1"><div className="font-medium">{t("aiRoutingGateway.common.loadFailed")}</div><div className="break-words text-xs">{error}</div></div><button type="button" onClick={() => void load()} className="h-8 rounded-md border px-2">{t("aiRoutingGateway.common.retry")}</button></div> : null}
        {loading && !data ? <div className="flex items-center justify-center gap-2 rounded-md border border-dashed p-16 text-sm text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" />{t("aiRoutingGateway.common.loading")}</div> : data && homepage ? <div className="space-y-4"><StatusBanner data={data} />{tab === "home" ? <HomeTab data={data} homepage={homepage} days={days} mode={mode} onDays={changeDays} onMode={setMode} /> : null}{tab === "accounts" ? <AccountsTab data={data} reload={load} /> : null}{tab === "keys" ? <KeysTab data={data} reload={load} /> : null}{tab === "logs" ? <LogsTab data={data} /> : null}{tab === "settings" ? <SettingsTab data={data} reload={load} /> : null}</div> : !error ? <div className="rounded-md border border-dashed p-12 text-center text-sm text-muted-foreground">{t("aiRoutingGateway.common.empty")}</div> : null}
      </div>
    </div>
  );
}
