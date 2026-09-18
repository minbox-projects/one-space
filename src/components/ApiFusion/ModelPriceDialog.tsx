import { Fragment, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, ChevronUp, Moon, Plus, Server, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  apiFusionGetConfig,
  apiFusionModelPricesGet,
  apiFusionModelPricesSave,
  getProviderAvailableModels,
  type FusionUpstreamProvider,
  type ModelPrice,
} from "@/lib/apiFusion";
import { errorToMessage } from "@/lib/messages";

export type DraftOffPeakPrice = {
  id: string;
  start_time: string;
  end_time: string;
  input: string;
  cache_read: string;
  cache_write: string;
  output: string;
};

export type DraftPrice = {
  id: string;
  provider_id: string | null;
  upstream_model: string;
  input: string;
  cache_read: string;
  cache_write: string;
  output: string;
  enable_off_peak: boolean;
  off_peaks: DraftOffPeakPrice[];
  is_expanded: boolean;
};

export type ModelPriceDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved?: (prices: ModelPrice[]) => void;
  providers?: FusionUpstreamProvider[];
};

const priceInputClass =
  "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/40";

const modelSelectClass =
  "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/40 min-w-[11rem] font-mono";

function toDraft(price: ModelPrice, index: number): DraftPrice {
  const rawOffPeaks =
    price.off_peaks && price.off_peaks.length > 0
      ? price.off_peaks
      : price.off_peak
        ? [price.off_peak]
        : [];

  const draftOffPeaks: DraftOffPeakPrice[] = rawOffPeaks.map((op, opIdx) => ({
    id: `op-${index}-${opIdx}-${Date.now()}-${Math.random()}`,
    start_time: op.start_time ?? "00:30",
    end_time: op.end_time ?? "08:30",
    input: op.input !== undefined && op.input !== null ? String(op.input) : "",
    cache_read:
      op.cache_read !== undefined && op.cache_read !== null ? String(op.cache_read) : "",
    cache_write:
      op.cache_write !== undefined && op.cache_write !== null ? String(op.cache_write) : "",
    output:
      op.output !== undefined && op.output !== null ? String(op.output) : "",
  }));

  const hasOffPeak = draftOffPeaks.length > 0;

  return {
    id: `price-${index}-${price.upstream_model}-${price.provider_id ?? "global"}`,
    provider_id: price.provider_id ?? null,
    upstream_model: price.upstream_model,
    input: String(price.input),
    cache_read: String(price.cache_read),
    cache_write: String(price.cache_write),
    output: String(price.output),
    enable_off_peak: hasOffPeak,
    off_peaks: hasOffPeak
      ? draftOffPeaks
      : [
          {
            id: `op-${index}-0-${Date.now()}-${Math.random()}`,
            start_time: "00:30",
            end_time: "08:30",
            input: "",
            cache_read: "",
            cache_write: "",
            output: "",
          },
        ],
    is_expanded: false,
  };
}

function parsePriceNumber(value: string): number {
  const parsed = Number(value.trim());
  return Number.isFinite(parsed) ? parsed : 0;
}

function OffPeakConfigPanel({
  draft,
  originalIndex,
  updateDraft,
}: {
  draft: DraftPrice;
  originalIndex: number;
  updateDraft: (index: number, patch: Partial<DraftPrice>) => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="rounded-lg border border-border/80 bg-background/80 p-3 space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <label className="inline-flex items-center gap-2 cursor-pointer font-medium text-xs text-foreground select-none">
          <input
            type="checkbox"
            checked={draft.enable_off_peak}
            onChange={(event) => {
              const checked = event.target.checked;
              updateDraft(originalIndex, {
                enable_off_peak: checked,
                off_peaks:
                  checked && draft.off_peaks.length === 0
                    ? [
                        {
                          id: `op-${Date.now()}-${Math.random()}`,
                          start_time: "00:30",
                          end_time: "08:30",
                          input: "",
                          cache_read: "",
                          cache_write: "",
                          output: "",
                        },
                      ]
                    : draft.off_peaks,
              });
            }}
            className="h-3.5 w-3.5 rounded border-border text-primary focus:ring-primary"
          />
          <span>
            {t("apiFusionOffPeakEnable", "Enable off-peak pricing")}
          </span>
        </label>

        {draft.enable_off_peak ? (
          <button
            type="button"
            onClick={() => {
              const newOp: DraftOffPeakPrice = {
                id: `op-${Date.now()}-${Math.random()}`,
                start_time: "00:30",
                end_time: "08:30",
                input: "",
                cache_read: "",
                cache_write: "",
                output: "",
              };
              updateDraft(originalIndex, {
                off_peaks: [...draft.off_peaks, newOp],
              });
            }}
            className="inline-flex h-7 items-center gap-1 rounded-md border border-border bg-background px-2 text-xs font-medium transition hover:bg-muted"
          >
            <Plus className="h-3 w-3" />
            <span>{t("apiFusionOffPeakAdd", "Add off-peak window")}</span>
          </button>
        ) : null}
      </div>

      {draft.enable_off_peak ? (
        <>
          <p className="text-[11px] text-muted-foreground">
            {t(
              "apiFusionOffPeakHint",
              "Calls during this window in UTC+8 use these discounted rates; standard rates apply otherwise.",
            )}
          </p>

          <div className="space-y-3">
            {draft.off_peaks.map((op, opIndex) => (
              <div
                key={op.id}
                className="rounded-md border border-border/70 bg-card/60 p-2.5 space-y-2"
              >
                <div className="flex items-center justify-between gap-2 border-b border-border/40 pb-2">
                  <div className="flex items-center gap-2">
                    <span className="text-[11px] font-semibold text-foreground">
                      {t("apiFusionOffPeakWindowIndex", {
                        index: opIndex + 1,
                        defaultValue: `Off-peak window #${opIndex + 1}`,
                      })}
                    </span>
                    <div className="flex items-center gap-1.5 text-xs">
                      <span className="text-muted-foreground font-medium">
                        {t(
                          "apiFusionOffPeakTimeRange",
                          "Off-peak window (UTC+8)",
                        )}
                        :
                      </span>
                      <input
                        type="text"
                        value={op.start_time}
                        onChange={(event) => {
                          const val = event.target.value;
                          const newOps = draft.off_peaks.map((item, idx) =>
                            idx === opIndex ? { ...item, start_time: val } : item,
                          );
                          updateDraft(originalIndex, { off_peaks: newOps });
                        }}
                        placeholder="00:30"
                        aria-label={t("apiFusionOffPeakStartTime", "Start")}
                        className="w-16 rounded border border-border bg-background px-2 py-1 text-center font-mono text-xs focus:border-primary focus:outline-none"
                      />
                      <span className="text-muted-foreground">-</span>
                      <input
                        type="text"
                        value={op.end_time}
                        onChange={(event) => {
                          const val = event.target.value;
                          const newOps = draft.off_peaks.map((item, idx) =>
                            idx === opIndex ? { ...item, end_time: val } : item,
                          );
                          updateDraft(originalIndex, { off_peaks: newOps });
                        }}
                        placeholder="08:30"
                        aria-label={t("apiFusionOffPeakEndTime", "End")}
                        className="w-16 rounded border border-border bg-background px-2 py-1 text-center font-mono text-xs focus:border-primary focus:outline-none"
                      />
                    </div>
                  </div>

                  {draft.off_peaks.length > 1 ? (
                    <button
                      type="button"
                      onClick={() => {
                        const newOps = draft.off_peaks.filter(
                          (_, idx) => idx !== opIndex,
                        );
                        updateDraft(originalIndex, { off_peaks: newOps });
                      }}
                      aria-label={t(
                        "apiFusionOffPeakDeleteAria",
                        "Delete this off-peak window",
                      )}
                      className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  ) : null}
                </div>

                <div>
                  <div className="text-[11px] font-medium text-muted-foreground mb-1.5">
                    {t(
                      "apiFusionOffPeakRates",
                      "Off-peak rates ($/1M tokens)",
                    )}
                  </div>
                  <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                    <div>
                      <label className="mb-1 block text-[10px] text-muted-foreground">
                        {t("apiFusionPriceInput", "Input")}
                      </label>
                      <input
                        type="number"
                        step="any"
                        value={op.input}
                        placeholder={draft.input || "0"}
                        onChange={(event) => {
                          const val = event.target.value;
                          const newOps = draft.off_peaks.map((item, idx) =>
                            idx === opIndex ? { ...item, input: val } : item,
                          );
                          updateDraft(originalIndex, { off_peaks: newOps });
                        }}
                        aria-label={`${t("apiFusionPriceInput", "Input")} (${t("apiFusionOffPeakBadge", "Off-peak")})`}
                        className={`${priceInputClass} min-w-[5rem]`}
                      />
                    </div>
                    <div>
                      <label className="mb-1 block text-[10px] text-muted-foreground">
                        {t("apiFusionPriceCacheRead", "Cache read")}
                      </label>
                      <input
                        type="number"
                        step="any"
                        value={op.cache_read}
                        placeholder={draft.cache_read || "0"}
                        onChange={(event) => {
                          const val = event.target.value;
                          const newOps = draft.off_peaks.map((item, idx) =>
                            idx === opIndex ? { ...item, cache_read: val } : item,
                          );
                          updateDraft(originalIndex, { off_peaks: newOps });
                        }}
                        aria-label={`${t("apiFusionPriceCacheRead", "Cache read")} (${t("apiFusionOffPeakBadge", "Off-peak")})`}
                        className={`${priceInputClass} min-w-[5rem]`}
                      />
                    </div>
                    <div>
                      <label className="mb-1 block text-[10px] text-muted-foreground">
                        {t("apiFusionPriceCacheWrite", "Cache write")}
                      </label>
                      <input
                        type="number"
                        step="any"
                        value={op.cache_write}
                        placeholder={draft.cache_write || "0"}
                        onChange={(event) => {
                          const val = event.target.value;
                          const newOps = draft.off_peaks.map((item, idx) =>
                            idx === opIndex ? { ...item, cache_write: val } : item,
                          );
                          updateDraft(originalIndex, { off_peaks: newOps });
                        }}
                        aria-label={`${t("apiFusionPriceCacheWrite", "Cache write")} (${t("apiFusionOffPeakBadge", "Off-peak")})`}
                        className={`${priceInputClass} min-w-[5rem]`}
                      />
                    </div>
                    <div>
                      <label className="mb-1 block text-[10px] text-muted-foreground">
                        {t("apiFusionPriceOutput", "Output")}
                      </label>
                      <input
                        type="number"
                        step="any"
                        value={op.output}
                        placeholder={draft.output || "0"}
                        onChange={(event) => {
                          const val = event.target.value;
                          const newOps = draft.off_peaks.map((item, idx) =>
                            idx === opIndex ? { ...item, output: val } : item,
                          );
                          updateDraft(originalIndex, { off_peaks: newOps });
                        }}
                        aria-label={`${t("apiFusionPriceOutput", "Output")} (${t("apiFusionOffPeakBadge", "Off-peak")})`}
                        className={`${priceInputClass} min-w-[5rem]`}
                      />
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </>
      ) : null}
    </div>
  );
}

export function ModelPriceDialog({
  open,
  onOpenChange,
  onSaved,
  providers: externalProviders,
}: ModelPriceDialogProps) {
  const { t } = useTranslation();
  const [drafts, setDrafts] = useState<DraftPrice[]>([]);
  const [providers, setProviders] = useState<FusionUpstreamProvider[]>(
    externalProviders ?? [],
  );
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setError("");

    const loadProviders = externalProviders
      ? Promise.resolve(externalProviders)
      : apiFusionGetConfig()
          .then((cfg) => cfg.providers ?? [])
          .catch(() => [] as FusionUpstreamProvider[]);

    Promise.all([apiFusionModelPricesGet(), loadProviders])
      .then(([prices, loadedProviders]) => {
        if (cancelled) return;
        setProviders(loadedProviders);

        // Normalize loaded prices: if provider_id is missing, try to associate with a matching provider
        const initialDrafts = (prices ?? []).map((p, idx) => {
          let pid = p.provider_id ?? null;
          if (!pid && loadedProviders.length > 0) {
            const matched = loadedProviders.find((prov) => {
              const available = getProviderAvailableModels(prov);
              return available.some((m) => m.upstream_model === p.upstream_model);
            });
            if (matched) {
              pid = matched.id;
            }
          }
          return toDraft({ ...p, provider_id: pid }, idx);
        });

        setDrafts(initialDrafts);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(errorToMessage(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [open, externalProviders]);

  const updateDraft = (index: number, patch: Partial<DraftPrice>) => {
    setDrafts((prev) =>
      prev.map((draft, draftIndex) =>
        draftIndex === index ? { ...draft, ...patch } : draft,
      ),
    );
  };

  const addDraftForProvider = (providerId: string | null) => {
    let defaultModel = "";
    if (providerId) {
      const prov = providers.find((p) => p.id === providerId);
      if (prov) {
        const available = getProviderAvailableModels(prov);
        // Pick the first model not yet priced for this provider
        const existingModels = new Set(
          drafts
            .filter((d) => d.provider_id === providerId)
            .map((d) => d.upstream_model),
        );
        const unpriced = available.find(
          (m) => !existingModels.has(m.upstream_model),
        );
        defaultModel = unpriced?.upstream_model ?? available[0]?.upstream_model ?? "";
      }
    }

    setDrafts((prev) => [
      ...prev,
      {
        id: `draft-${Date.now()}-${Math.random()}`,
        provider_id: providerId,
        upstream_model: defaultModel,
        input: "0",
        cache_read: "0",
        cache_write: "0",
        output: "0",
        enable_off_peak: false,
        off_peaks: [
          {
            id: `op-${Date.now()}-${Math.random()}`,
            start_time: "00:30",
            end_time: "08:30",
            input: "",
            cache_read: "",
            cache_write: "",
            output: "",
          },
        ],
        is_expanded: false,
      },
    ]);
  };

  const removeDraft = (index: number) => {
    setDrafts((prev) => prev.filter((_draft, draftIndex) => draftIndex !== index));
  };

  const handleSave = async () => {
    setSaving(true);
    setError("");
    try {
      const prices: ModelPrice[] = drafts
        .map((draft) => {
          const item: ModelPrice = {
            ...(draft.provider_id ? { provider_id: draft.provider_id } : {}),
            upstream_model: draft.upstream_model.trim(),
            input: parsePriceNumber(draft.input),
            cache_read: parsePriceNumber(draft.cache_read),
            cache_write: parsePriceNumber(draft.cache_write),
            output: parsePriceNumber(draft.output),
          };
          if (draft.enable_off_peak && draft.off_peaks.length > 0) {
            item.off_peaks = draft.off_peaks.map((op) => ({
              start_time: op.start_time.trim() || "00:30",
              end_time: op.end_time.trim() || "08:30",
              input: parsePriceNumber(op.input || draft.input),
              cache_read: parsePriceNumber(op.cache_read || draft.cache_read),
              cache_write: parsePriceNumber(op.cache_write || draft.cache_write),
              output: parsePriceNumber(op.output || draft.output),
            }));
            item.off_peak = item.off_peaks[0] ?? null;
          }
          return item;
        })
        .filter((price) => price.upstream_model !== "");

      const saved = await apiFusionModelPricesSave(prices);
      onSaved?.(saved ?? prices);
      onOpenChange(false);
    } catch (err) {
      setError(errorToMessage(err));
    } finally {
      setSaving(false);
    }
  };

  type PriceTierKey = "input" | "cache_read" | "cache_write" | "output";

  const columns: Array<{
    key: PriceTierKey;
    label: string;
  }> = [
    { key: "input", label: t("apiFusionPriceInput", "Input") },
    { key: "cache_read", label: t("apiFusionPriceCacheRead", "Cache read") },
    { key: "cache_write", label: t("apiFusionPriceCacheWrite", "Cache write") },
    { key: "output", label: t("apiFusionPriceOutput", "Output") },
  ];

  // Group drafts by provider
  const knownProviderIds = new Set(providers.map((p) => p.id));
  const unassignedDrafts: { draft: DraftPrice; originalIndex: number }[] = [];

  drafts.forEach((draft, idx) => {
    if (!draft.provider_id || !knownProviderIds.has(draft.provider_id)) {
      unassignedDrafts.push({ draft, originalIndex: idx });
    }
  });

  const providerGroups = providers.map((provider) => {
    const groupDrafts: { draft: DraftPrice; originalIndex: number }[] = [];
    drafts.forEach((draft, idx) => {
      if (draft.provider_id === provider.id) {
        groupDrafts.push({ draft, originalIndex: idx });
      }
    });
    return {
      provider,
      availableModels: getProviderAvailableModels(provider),
      drafts: groupDrafts,
    };
  });

  // Check if there are no drafts at all
  const hasAnyDrafts = drafts.length > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[90vh] w-full overflow-y-auto p-5 sm:max-w-6xl sm:rounded-xl"
        data-testid="api-fusion-model-price-dialog"
      >
        <DialogHeader className="space-y-1">
          <DialogTitle className="text-base font-semibold">
            {t("apiFusionModelPriceDialogTitle", "Model prices")}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "apiFusionModelPriceDialogDesc",
              "Maintain four price tiers per upstream model grouped by provider.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="flex items-center justify-between gap-3">
            <span className="text-xs text-muted-foreground">
              {t("apiFusionPricePerMillion", "USD / million tokens")}
            </span>
            <button
              type="button"
              onClick={() => addDraftForProvider(providers[0]?.id ?? null)}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("apiFusionAddPrice", "Add price")}
            </button>
          </div>

          {error ? (
            <div
              role="alert"
              className="rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive"
            >
              {error}
            </div>
          ) : null}

          {loading ? (
            <p className="rounded-lg border border-dashed bg-muted/20 px-3 py-4 text-center text-xs text-muted-foreground">
              {t("loading", "Loading...")}
            </p>
          ) : providers.length === 0 && !hasAnyDrafts ? (
            <p className="rounded-lg border border-dashed bg-muted/20 px-3 py-6 text-center text-xs text-muted-foreground">
              {t("apiFusionModelPricesEmpty", "No model prices configured yet.")}
            </p>
          ) : (
            <div className="space-y-4">
              {/* Groups per provider */}
              {providerGroups.map(({ provider, availableModels, drafts: pDrafts }) => (
                <div
                  key={provider.id}
                  data-testid={`api-fusion-price-group-${provider.id}`}
                  className="rounded-xl border border-border/80 bg-card p-4 shadow-sm"
                >
                  <div className="mb-3 flex items-center justify-between gap-2 border-b border-border/60 pb-2.5">
                    <div className="flex items-center gap-2">
                      <Server className="h-4 w-4 text-muted-foreground" />
                      <span className="text-sm font-semibold text-foreground">
                        {provider.name}
                      </span>
                      <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                        {t("apiFusionPriceModelCount", {
                          count: pDrafts.length,
                          defaultValue: `${pDrafts.length} models priced`,
                        })}
                      </span>
                    </div>

                    <button
                      type="button"
                      onClick={() => addDraftForProvider(provider.id)}
                      disabled={availableModels.length === 0}
                      title={
                        availableModels.length === 0
                          ? t(
                              "apiFusionPriceProviderNoModels",
                              "No models configured for this provider. Please add mappings in provider settings first.",
                            )
                          : t("apiFusionAddPrice", "Add price")
                      }
                      className="inline-flex h-7 items-center gap-1 rounded-md border bg-background px-2 text-xs font-medium transition hover:bg-muted disabled:opacity-50"
                    >
                      <Plus className="h-3 w-3" />
                      <span>{t("apiFusionAddPrice", "Add price")}</span>
                    </button>
                  </div>

                  {availableModels.length === 0 ? (
                    <p className="rounded-lg border border-dashed bg-muted/10 px-3 py-3 text-center text-xs text-muted-foreground">
                      {t(
                        "apiFusionPriceProviderNoModels",
                        "No models configured for this provider. Please add mappings in provider settings first.",
                      )}
                    </p>
                  ) : pDrafts.length === 0 ? (
                    <p className="rounded-lg border border-dashed bg-muted/10 px-3 py-3 text-center text-xs text-muted-foreground">
                      {t(
                        "apiFusionModelPricesEmpty",
                        "No model prices configured yet.",
                      )}
                    </p>
                  ) : (
                    <div className="overflow-x-auto">
                      <table className="w-full text-left text-xs">
                        <thead className="text-muted-foreground">
                          <tr>
                            <th className="px-2 py-1.5 font-medium">
                              {t("apiFusionPriceModel", "Upstream model")}
                            </th>
                            {columns.map((column) => (
                              <th
                                key={column.key}
                                className="px-2 py-1.5 font-medium"
                              >
                                {column.label}
                              </th>
                            ))}
                            <th className="px-2 py-1.5 font-medium text-center whitespace-nowrap">
                              {t("apiFusionOffPeakBadge", "Off-peak")}
                            </th>
                            <th className="w-8 px-2 py-1.5" />
                          </tr>
                        </thead>
                        <tbody>
                          {pDrafts.map(({ draft, originalIndex }) => (
                            <Fragment key={draft.id}>
                              <tr className="border-t">
                                <td className="px-2 py-1.5">
                                  <select
                                    value={draft.upstream_model}
                                    onChange={(event) =>
                                      updateDraft(originalIndex, {
                                        upstream_model: event.target.value,
                                      })
                                    }
                                    aria-label={t(
                                      "apiFusionPriceModel",
                                      "Upstream model",
                                    )}
                                    className={modelSelectClass}
                                  >
                                    {!draft.upstream_model ? (
                                      <option value="" disabled>
                                        {t(
                                          "apiFusionPriceSelectModel",
                                          "Select model",
                                        )}
                                      </option>
                                    ) : null}
                                    {availableModels.map((m) => (
                                      <option
                                        key={m.upstream_model}
                                        value={m.upstream_model}
                                      >
                                        {m.upstream_model}
                                        {m.display_name &&
                                        m.display_name !== m.upstream_model
                                          ? ` (${m.display_name})`
                                          : ""}
                                      </option>
                                    ))}
                                    {/* In case the model is not in the provider's current mappings */}
                                    {draft.upstream_model &&
                                    !availableModels.some(
                                      (m) => m.upstream_model === draft.upstream_model,
                                    ) ? (
                                      <option value={draft.upstream_model}>
                                        {draft.upstream_model}
                                      </option>
                                    ) : null}
                                  </select>
                                </td>
                                {columns.map((column) => (
                                  <td key={column.key} className="px-2 py-1.5">
                                    <input
                                      type="number"
                                      step="any"
                                      value={draft[column.key]}
                                      onChange={(event) =>
                                        updateDraft(originalIndex, {
                                          [column.key]: event.target.value,
                                        })
                                      }
                                      aria-label={column.label}
                                      className={`${priceInputClass} min-w-[5.5rem]`}
                                    />
                                  </td>
                                ))}
                                <td className="px-2 py-1.5 text-center whitespace-nowrap">
                                  <button
                                    type="button"
                                    onClick={() =>
                                      updateDraft(originalIndex, {
                                        is_expanded: !draft.is_expanded,
                                      })
                                    }
                                    title={
                                      !draft.enable_off_peak || draft.off_peaks.length === 0
                                        ? t("apiFusionOffPeakConfigure", "Off-peak discount")
                                        : draft.off_peaks
                                            .map(
                                              (op) =>
                                                t("apiFusionOffPeakActive", {
                                                  start: op.start_time,
                                                  end: op.end_time,
                                                  defaultValue: `Off-peak (${op.start_time} - ${op.end_time})`,
                                                }),
                                            )
                                            .join(", ")
                                    }
                                    className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition ${
                                      draft.enable_off_peak && draft.off_peaks.length > 0
                                        ? "bg-amber-500/15 text-amber-700 dark:text-amber-400 border border-amber-500/30 hover:bg-amber-500/25"
                                        : "border border-dashed border-border text-muted-foreground hover:bg-muted hover:text-foreground"
                                    }`}
                                  >
                                    <Moon className="h-3 w-3" />
                                    <span>
                                      {!draft.enable_off_peak || draft.off_peaks.length === 0
                                        ? t(
                                            "apiFusionOffPeakConfigure",
                                            "Off-peak discount",
                                          )
                                        : draft.off_peaks.length === 1
                                          ? `${draft.off_peaks[0].start_time}-${draft.off_peaks[0].end_time}`
                                          : `${draft.off_peaks[0].start_time}-${draft.off_peaks[0].end_time} (+${draft.off_peaks.length - 1})`}
                                    </span>
                                    {draft.is_expanded ? (
                                      <ChevronUp className="h-3 w-3 opacity-70" />
                                    ) : (
                                      <ChevronDown className="h-3 w-3 opacity-70" />
                                    )}
                                  </button>
                                </td>
                                <td className="px-2 py-1.5">
                                  <button
                                    type="button"
                                    onClick={() => removeDraft(originalIndex)}
                                    aria-label={t("apiFusionDeletePriceAria", {
                                      model: draft.upstream_model || "—",
                                      defaultValue: `Delete price for ${draft.upstream_model || "—"}`,
                                    })}
                                    className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                                  >
                                    <Trash2 className="h-3.5 w-3.5" />
                                  </button>
                                </td>
                              </tr>

                              {draft.is_expanded ? (
                                <tr className="bg-muted/15 border-b">
                                  <td colSpan={7} className="px-3 py-3">
                                    <OffPeakConfigPanel
                                      draft={draft}
                                      originalIndex={originalIndex}
                                      updateDraft={updateDraft}
                                    />
                                  </td>
                                </tr>
                              ) : null}
                            </Fragment>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              ))}

              {/* Unassigned / Legacy prices */}
              {unassignedDrafts.length > 0 ? (
                <div
                  data-testid="api-fusion-price-group-unassigned"
                  className="rounded-xl border border-dashed border-border bg-card p-4 shadow-sm"
                >
                  <div className="mb-3 flex items-center justify-between gap-2 border-b border-border/60 pb-2.5">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-semibold text-muted-foreground">
                        {t(
                          "apiFusionPriceUnassignedProvider",
                          "Other / Unassigned",
                        )}
                      </span>
                      <span className="rounded-full bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                        {t("apiFusionPriceModelCount", {
                          count: unassignedDrafts.length,
                          defaultValue: `${unassignedDrafts.length} models priced`,
                        })}
                      </span>
                    </div>
                  </div>

                  <div className="overflow-x-auto">
                    <table className="w-full text-left text-xs">
                      <thead className="text-muted-foreground">
                        <tr>
                          <th className="px-2 py-1.5 font-medium">
                            {t("apiFusionPriceModel", "Upstream model")}
                          </th>
                          {columns.map((column) => (
                            <th
                              key={column.key}
                              className="px-2 py-1.5 font-medium"
                            >
                              {column.label}
                            </th>
                          ))}
                          <th className="px-2 py-1.5 font-medium text-center whitespace-nowrap">
                            {t("apiFusionOffPeakBadge", "Off-peak")}
                          </th>
                          <th className="w-8 px-2 py-1.5" />
                        </tr>
                      </thead>
                      <tbody>
                        {unassignedDrafts.map(({ draft, originalIndex }) => (
                          <Fragment key={draft.id}>
                            <tr className="border-t">
                              <td className="px-2 py-1.5">
                                <input
                                  type="text"
                                  value={draft.upstream_model}
                                  onChange={(event) =>
                                    updateDraft(originalIndex, {
                                      upstream_model: event.target.value,
                                    })
                                  }
                                  placeholder={t(
                                    "apiFusionPriceModelPlaceholder",
                                    "e.g. gpt-4o",
                                  )}
                                  aria-label={t(
                                    "apiFusionPriceModel",
                                    "Upstream model",
                                  )}
                                  className={`${priceInputClass} min-w-[10rem] font-mono`}
                                />
                              </td>
                              {columns.map((column) => (
                                <td key={column.key} className="px-2 py-1.5">
                                  <input
                                    type="number"
                                    step="any"
                                    value={draft[column.key]}
                                    onChange={(event) =>
                                      updateDraft(originalIndex, {
                                        [column.key]: event.target.value,
                                      })
                                    }
                                    aria-label={column.label}
                                    className={`${priceInputClass} min-w-[5.5rem]`}
                                  />
                                </td>
                              ))}
                              <td className="px-2 py-1.5 text-center whitespace-nowrap">
                                <button
                                  type="button"
                                  onClick={() =>
                                    updateDraft(originalIndex, {
                                      is_expanded: !draft.is_expanded,
                                    })
                                  }
                                  title={
                                    !draft.enable_off_peak || draft.off_peaks.length === 0
                                      ? t("apiFusionOffPeakConfigure", "Off-peak discount")
                                      : draft.off_peaks
                                          .map(
                                            (op) =>
                                              t("apiFusionOffPeakActive", {
                                                start: op.start_time,
                                                end: op.end_time,
                                                defaultValue: `Off-peak (${op.start_time} - ${op.end_time})`,
                                              }),
                                          )
                                          .join(", ")
                                  }
                                  className={`inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition ${
                                    draft.enable_off_peak && draft.off_peaks.length > 0
                                      ? "bg-amber-500/15 text-amber-700 dark:text-amber-400 border border-amber-500/30 hover:bg-amber-500/25"
                                      : "border border-dashed border-border text-muted-foreground hover:bg-muted hover:text-foreground"
                                  }`}
                                >
                                  <Moon className="h-3 w-3" />
                                  <span>
                                    {!draft.enable_off_peak || draft.off_peaks.length === 0
                                      ? t(
                                          "apiFusionOffPeakConfigure",
                                          "Off-peak discount",
                                        )
                                      : draft.off_peaks.length === 1
                                        ? `${draft.off_peaks[0].start_time}-${draft.off_peaks[0].end_time}`
                                        : `${draft.off_peaks[0].start_time}-${draft.off_peaks[0].end_time} (+${draft.off_peaks.length - 1})`}
                                  </span>
                                  {draft.is_expanded ? (
                                    <ChevronUp className="h-3 w-3 opacity-70" />
                                  ) : (
                                    <ChevronDown className="h-3 w-3 opacity-70" />
                                  )}
                                </button>
                              </td>
                              <td className="px-2 py-1.5">
                                <button
                                  type="button"
                                  onClick={() => removeDraft(originalIndex)}
                                  aria-label={t("apiFusionDeletePriceAria", {
                                    model: draft.upstream_model || "—",
                                    defaultValue: `Delete price for ${draft.upstream_model || "—"}`,
                                  })}
                                  className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                                >
                                  <Trash2 className="h-3.5 w-3.5" />
                                </button>
                              </td>
                            </tr>

                            {draft.is_expanded ? (
                              <tr className="bg-muted/15 border-b">
                                <td colSpan={7} className="px-3 py-3">
                                  <OffPeakConfigPanel
                                    draft={draft}
                                    originalIndex={originalIndex}
                                    updateDraft={updateDraft}
                                  />
                                </td>
                              </tr>
                            ) : null}
                          </Fragment>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              ) : null}
            </div>
          )}
        </div>

        <DialogFooter className="flex flex-row items-center justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            disabled={saving}
            className="rounded-lg border bg-background px-4 py-2 text-sm font-medium transition hover:bg-muted disabled:opacity-50"
          >
            {t("cancel", "Cancel")}
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={saving || loading}
            className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
          >
            {t("apiFusionSave", "Save")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
