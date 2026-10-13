import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Search, Server, Trash2, X } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { errorToMessage } from "@/lib/messages";
import {
  MODEL_POOL_AVAILABILITY_LABEL_KEYS,
  modelPoolCandidateKey,
  newModelPoolId,
  resolveModelPoolCandidateState,
  type GatewayModelPool,
  type GatewayModelPoolCandidate,
  type GatewayModelPoolStatus,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { protocolBadgeClass, protocolShortLabel } from "./gatewayShared";

type DraftCandidate = GatewayModelPoolCandidate & { key: string };

type ModelPoolDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  providers: GatewayUpstreamProvider[];
  /** The saved pool being edited; `null`/absent creates a new pool. */
  pool?: GatewayModelPool | null;
  /** Latest nonsecret status for the edited pool, used only for reasons. */
  poolStatus?: GatewayModelPoolStatus | null;
  busy?: boolean;
  /** Persist the pool; a rejected promise keeps the draft and surfaces its message. */
  onSave: (pool: GatewayModelPool) => Promise<void>;
};

function toDraft(candidate: GatewayModelPoolCandidate): DraftCandidate {
  return { ...candidate, key: modelPoolCandidateKey(candidate) };
}

function parseWeight(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed < 1 || parsed > 100) return null;
  return parsed;
}

export function ModelPoolDialog({
  open,
  onOpenChange,
  providers,
  pool = null,
  poolStatus = null,
  busy = false,
  onSave,
}: ModelPoolDialogProps) {
  const { t } = useTranslation();

  // Seeded once per mount; the console remounts this dialog per open/target so
  // an in-flight runtime refresh can never overwrite an unsaved draft.
  const [name, setName] = useState(pool ? pool.name : "Free Modle");
  const [model, setModel] = useState(pool ? pool.model : "free-model");
  const [selected, setSelected] = useState<DraftCandidate[]>(() =>
    (pool?.candidates ?? []).map(toDraft),
  );
  const [weights, setWeights] = useState<Record<string, string>>(() => {
    const initial: Record<string, string> = {};
    for (const candidate of pool?.candidates ?? []) {
      initial[modelPoolCandidateKey(candidate)] = String(candidate.weight);
    }
    return initial;
  });
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [saving, setSaving] = useState(false);

  const statusByKey = useMemo(() => {
    const map = new Map<string, GatewayModelPoolStatus["candidates"][number]>();
    for (const candidate of poolStatus?.candidates ?? []) {
      map.set(modelPoolCandidateKey(candidate), candidate);
    }
    return map;
  }, [poolStatus]);

  const sourceGroups = useMemo(() => {
    const groups = providers.map((provider) => ({
      providerId: provider.id,
      providerName: provider.name,
      rows: (provider.mappings ?? [])
        .map((mapping) => {
          const localModel = mapping.local_model.trim();
          const upstreamModel = mapping.upstream_model.trim();
          if (!localModel || !upstreamModel) return null;
          const candidate: GatewayModelPoolCandidate = {
            provider_id: provider.id,
            local_model: localModel,
            upstream_model: upstreamModel,
            protocol:
              (mapping.protocol as GatewayUpstreamProtocol | undefined) ??
              provider.protocol ??
              "chat_completions",
            weight: 1,
          };
          return { key: modelPoolCandidateKey(candidate), candidate };
        })
        .filter((row): row is { key: string; candidate: GatewayModelPoolCandidate } =>
          row !== null,
        ),
    }));
    return groups;
  }, [providers]);

  const normalizedQuery = query.trim().toLowerCase();

  const selectedKeys = useMemo(
    () => new Set(selected.map((candidate) => candidate.key)),
    [selected],
  );

  const toggleCandidate = (candidate: GatewayModelPoolCandidate) => {
    const key = modelPoolCandidateKey(candidate);
    setSelected((previous) => {
      if (previous.some((item) => item.key === key)) {
        return previous.filter((item) => item.key !== key);
      }
      return [...previous, { ...candidate, weight: 1, key }];
    });
    setWeights((previous) => {
      if (previous[key] !== undefined) return previous;
      return { ...previous, [key]: "1" };
    });
    setError(null);
  };

  const removeCandidate = (key: string) => {
    setSelected((previous) => previous.filter((item) => item.key !== key));
    setError(null);
  };

  const handleSave = async () => {
    if (busy || saving) return;
    const trimmedName = name.trim();
    const trimmedModel = model.trim();
    if (!trimmedName) {
      setError(t("aiGatewayModelPoolErrorNameRequired", "Enter a pool name."));
      return;
    }
    if (!trimmedModel) {
      setError(
        t("aiGatewayModelPoolErrorModelRequired", "Enter a public model ID."),
      );
      return;
    }
    if (selected.length === 0) {
      setError(
        t(
          "aiGatewayModelPoolErrorSelectionRequired",
          "Select at least one source model.",
        ),
      );
      return;
    }
    const candidates: GatewayModelPoolCandidate[] = [];
    for (const candidate of selected) {
      const weight = parseWeight(weights[candidate.key] ?? "1");
      if (weight === null) {
        setError(
          t(
            "aiGatewayModelPoolErrorWeightInvalid",
            "Weight must be an integer between 1 and 100.",
          ),
        );
        return;
      }
      candidates.push({
        provider_id: candidate.provider_id,
        local_model: candidate.local_model,
        upstream_model: candidate.upstream_model,
        protocol: candidate.protocol,
        weight,
      });
    }
    setError(null);
    setSaving(true);
    try {
      await onSave({
        id: pool?.id || newModelPoolId(),
        name: trimmedName,
        model: trimmedModel,
        enabled: pool?.enabled ?? true,
        candidates,
      });
      onOpenChange(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : errorToMessage(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        data-testid="model-pool-dialog"
        className="w-full sm:max-w-2xl lg:max-w-3xl max-h-[88vh] overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
      >
        <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
              <Server className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0">
              <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                {pool
                  ? t("aiGatewayModelPoolDialogEditTitle", "Edit model pool")
                  : t("aiGatewayModelPoolDialogTitle", "Create model pool")}
              </DialogTitle>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                {t(
                  "aiGatewayModelPoolDialogDesc",
                  "Select source models and assign each candidate an integer weight.",
                )}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-5 p-6">
          {/* Basic fields */}
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
              <span>{t("aiGatewayModelPoolNameLabel", "Name")}</span>
              <input
                type="text"
                data-testid="model-pool-name-input"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("aiGatewayModelPoolNamePlaceholder", "Free Modle")}
                disabled={busy || saving}
                className="h-9 rounded-lg border border-border/80 bg-background px-3 text-sm text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
              />
            </label>
            <label className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
              <span>
                {t("aiGatewayModelPoolModelLabel", "Public model ID")}
              </span>
              <input
                type="text"
                data-testid="model-pool-model-input"
                value={model}
                onChange={(event) => setModel(event.target.value)}
                placeholder={t(
                  "aiGatewayModelPoolModelPlaceholder",
                  "free-model",
                )}
                disabled={busy || saving}
                className="h-9 rounded-lg border border-border/80 bg-background px-3 font-mono text-sm text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
              />
            </label>
          </div>

          {/* Source search */}
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
            <input
              type="search"
              data-testid="model-pool-source-search"
              aria-label={t("aiGatewayModelPoolSourceSearch", "Search source models")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t(
                "aiGatewayModelPoolSourceSearchPlaceholder",
                "Search by provider or model",
              )}
              className="h-9 w-full rounded-lg border border-border/80 bg-background pl-9 pr-9 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20"
            />
            {query ? (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label={t("aiGatewayModelListClearSearch", "Clear search")}
                className="absolute right-2 top-1/2 -translate-y-1/2 rounded-full p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            ) : null}
          </div>

          {/* Source groups */}
          <div className="space-y-3">
            <div className="text-xs font-semibold text-foreground">
              {t("aiGatewayModelPoolSourcesTitle", "Source models")}
            </div>
            {sourceGroups.length === 0 ? (
              <p className="rounded-lg border border-dashed bg-muted/20 px-3 py-4 text-center text-xs text-muted-foreground">
                {t("aiGatewayModelPoolNoSources", "No source models available.")}
              </p>
            ) : (
              sourceGroups.map((group) => {
                const visibleRows = group.rows.filter((row) =>
                  normalizedQuery
                    ? row.candidate.local_model
                        .toLowerCase()
                        .includes(normalizedQuery) ||
                      row.candidate.upstream_model
                        .toLowerCase()
                        .includes(normalizedQuery) ||
                      group.providerName.toLowerCase().includes(normalizedQuery)
                    : true,
                );
                const isCollapsed = Boolean(collapsed[group.providerId]);
                return (
                  <div
                    key={group.providerId}
                    className="rounded-xl border border-border/70 bg-card/60"
                  >
                    <button
                      type="button"
                      data-testid={`model-pool-source-group-${group.providerId}`}
                      aria-expanded={!isCollapsed}
                      onClick={() =>
                        setCollapsed((previous) => ({
                          ...previous,
                          [group.providerId]: !previous[group.providerId],
                        }))
                      }
                      className="flex w-full items-start justify-between gap-2 px-3 py-2 text-left"
                    >
                      <span className="inline-flex min-w-0 flex-1 items-start gap-2 text-xs font-semibold text-foreground">
                        <Server className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                        <span className="min-w-0 break-words">{group.providerName}</span>
                        <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                          {group.rows.length}
                        </span>
                      </span>
                      <ChevronDown
                        className={`mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform ${
                          isCollapsed ? "-rotate-90" : ""
                        }`}
                      />
                    </button>
                    {!isCollapsed ? (
                      visibleRows.length === 0 ? (
                        <p className="px-3 pb-3 text-[11px] text-muted-foreground">
                          {t(
                            "aiGatewayModelListNoMatch",
                            "No models match the current search.",
                          )}
                        </p>
                      ) : (
                        <div className="space-y-1 px-2 pb-2">
                          {visibleRows.map(({ key, candidate }) => {
                            const checked = selectedKeys.has(key);
                            const state = resolveModelPoolCandidateState(
                              candidate,
                              providers,
                              statusByKey.get(key) ?? null,
                            );
                            const labelKey =
                              MODEL_POOL_AVAILABILITY_LABEL_KEYS[
                                state.availability
                              ];
                            const label = labelKey
                              ? t(labelKey)
                              : state.availability;
                            return (
                              <label
                                key={key}
                                className="flex cursor-pointer items-start gap-2.5 rounded-lg px-2 py-1.5 transition hover:bg-muted/50"
                              >
                                <input
                                  type="checkbox"
                                  checked={checked}
                                  onChange={() => toggleCandidate(candidate)}
                                  aria-label={`${group.providerName} ${candidate.local_model} ${candidate.upstream_model}`}
                                  disabled={busy || saving}
                                  className="mt-0.5 h-3.5 w-3.5 shrink-0 rounded border-border accent-primary"
                                />
                                <span className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5 text-xs">
                                  <span className="break-all font-medium text-foreground">
                                    {candidate.local_model}
                                  </span>
                                  <span className="text-muted-foreground/60">
                                    →
                                  </span>
                                  <code className="break-all rounded border border-border/40 bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                                    {candidate.upstream_model}
                                  </code>
                                  <span
                                    className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${protocolBadgeClass(
                                      candidate.protocol,
                                    )}`}
                                  >
                                    {protocolShortLabel(candidate.protocol)}
                                  </span>
                                  <span
                                    className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${
                                      state.ready
                                        ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                                        : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
                                    }`}
                                  >
                                    {label}
                                  </span>
                                  {state.reason ? (
                                    <span className="break-words text-[10px] text-amber-600 dark:text-amber-400">
                                      {state.reason}
                                    </span>
                                  ) : null}
                                </span>
                              </label>
                            );
                          })}
                        </div>
                      )
                    ) : null}
                  </div>
                );
              })
            )}
          </div>

          {/* Selected candidates */}
          <div
            data-testid="model-pool-selected-list"
            className="space-y-2 rounded-xl border border-border/70 bg-muted/20 p-3"
          >
            <div className="text-xs font-semibold text-foreground">
              {t("aiGatewayModelPoolSelectedTitle", "Selected candidates")}
            </div>
            {selected.length === 0 ? (
              <p className="text-[11px] text-muted-foreground">
                {t(
                  "aiGatewayModelPoolSelectedEmpty",
                  "Select at least one source model.",
                )}
              </p>
            ) : (
              selected.map((candidate) => {
                const state = resolveModelPoolCandidateState(
                  candidate,
                  providers,
                  statusByKey.get(candidate.key) ?? null,
                );
                const labelKey =
                  MODEL_POOL_AVAILABILITY_LABEL_KEYS[state.availability];
                const label = labelKey ? t(labelKey) : state.availability;
                const providerName =
                  state.provider_name ?? candidate.provider_id;
                return (
                  <div
                    key={candidate.key}
                    className="flex flex-wrap items-center gap-2 rounded-lg border border-border/60 bg-background/70 px-2.5 py-1.5"
                  >
                    <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5 text-xs">
                      <span className="break-all font-medium text-foreground">
                        {candidate.local_model}
                      </span>
                      <span className="text-muted-foreground/60">→</span>
                      <code className="break-all rounded border border-border/40 bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                        {candidate.upstream_model}
                      </code>
                      <span className="break-words text-[10px] text-muted-foreground">
                        {providerName}
                      </span>
                      <span
                        className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${protocolBadgeClass(
                          candidate.protocol,
                        )}`}
                      >
                        {protocolShortLabel(candidate.protocol)}
                      </span>
                      <span
                        className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium ${
                          state.ready
                            ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                            : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
                        }`}
                      >
                        {label}
                      </span>
                      {state.reason ? (
                        <span className="break-words text-[10px] text-amber-600 dark:text-amber-400">
                          {state.reason}
                        </span>
                      ) : null}
                    </div>
                    <label className="flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground">
                      <input
                        type="number"
                        min={1}
                        max={100}
                        step={1}
                        value={weights[candidate.key] ?? "1"}
                        onChange={(event) =>
                          setWeights((previous) => ({
                            ...previous,
                            [candidate.key]: event.target.value,
                          }))
                        }
                        aria-label={t("aiGatewayModelPoolWeightAria", {
                          model: candidate.local_model,
                          defaultValue: "{{model}} weight",
                        })}
                        disabled={busy || saving}
                        className="h-7 w-16 rounded-md border border-border/80 bg-background px-2 text-right text-xs text-foreground shadow-xs outline-none focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
                      />
                    </label>
                    <button
                      type="button"
                      onClick={() => removeCandidate(candidate.key)}
                      disabled={busy || saving}
                      aria-label={t("aiGatewayModelPoolRemoveAria", {
                        model: candidate.local_model,
                        defaultValue: "Remove {{model}}",
                      })}
                      className="shrink-0 rounded p-1 text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                );
              })
            )}
          </div>

          {error ? (
            <div
              role="alert"
              data-testid="model-pool-error"
              className="rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive"
            >
              {error}
            </div>
          ) : null}
        </div>

        <DialogFooter className="flex flex-row items-center justify-end gap-2 border-t px-6 py-4 shrink-0">
          <button
            type="button"
            data-testid="model-pool-cancel"
            onClick={() => onOpenChange(false)}
            disabled={busy || saving}
            className="inline-flex h-8 items-center rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground transition hover:bg-muted disabled:opacity-50"
          >
            {t("aiGatewayModelPoolCancel", "Cancel")}
          </button>
          <button
            type="button"
            data-testid="model-pool-save"
            onClick={() => void handleSave()}
            disabled={busy || saving}
            className="inline-flex h-8 items-center rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-xs transition hover:bg-primary/90 disabled:opacity-50"
          >
            {t("aiGatewayModelPoolSave", "Save")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
