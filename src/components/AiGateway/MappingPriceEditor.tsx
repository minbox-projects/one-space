import { useTranslation } from "react-i18next";
import { Moon, Plus, Trash2 } from "lucide-react";
import {
  GATEWAY_WEEKDAY_ORDER,
  gatewayWeekdayTranslationKey,
  normalizeDraftDays,
  type GatewayPriceDraft,
  type GatewayPriceDraftOffPeak,
} from "@/lib/aiGateway";

export type MappingPriceEditorProps = {
  draft: GatewayPriceDraft;
  index: number;
  onChange: (patch: Partial<GatewayPriceDraft>) => void;
};

type TierSource = {
  input: string;
  cache_read: string;
  cache_write: string;
  output: string;
};

const tierInputClass =
  "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/40";

const TIERS: Array<{
  suffix: string;
  labelKey: string;
  get: (source: TierSource) => string;
  patch: (value: string) => Partial<GatewayPriceDraft>;
}> = [
  {
    suffix: "input",
    labelKey: "aiGatewayPriceInput",
    get: (source) => source.input,
    patch: (value) => ({ input: value }),
  },
  {
    suffix: "cache-read",
    labelKey: "aiGatewayPriceCacheRead",
    get: (source) => source.cache_read,
    patch: (value) => ({ cache_read: value }),
  },
  {
    suffix: "cache-write",
    labelKey: "aiGatewayPriceCacheWrite",
    get: (source) => source.cache_write,
    patch: (value) => ({ cache_write: value }),
  },
  {
    suffix: "output",
    labelKey: "aiGatewayPriceOutput",
    get: (source) => source.output,
    patch: (value) => ({ output: value }),
  },
];

function blankOffPeak(): GatewayPriceDraftOffPeak {
  return {
    id: `op-${Date.now()}-${Math.random()}`,
    start_time: "00:30",
    end_time: "08:30",
    input: "",
    cache_read: "",
    cache_write: "",
    output: "",
    days: [],
  };
}

/**
 * Price editor for a single upstream model, rendered on demand by the mapping
 * row's expandable section when it is expanded: four standard tiers plus an
 * optional UTC+8 weekday-aware off-peak configuration.
 */
export function MappingPriceEditor({
  draft,
  index,
  onChange,
}: MappingPriceEditorProps) {
  const { t } = useTranslation();

  const updateOffPeak = (
    windowIndex: number,
    patch: Partial<GatewayPriceDraftOffPeak>,
  ) => {
    onChange({
      off_peaks: draft.off_peaks.map((op, opIndex) =>
        opIndex === windowIndex ? { ...op, ...patch } : op,
      ),
    });
  };

  return (
    <div
      data-testid={`ai-gateway-mapping-price-${index}`}
      className="space-y-2 rounded-lg border border-border/70 bg-background/70 p-2.5"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] font-semibold text-foreground">
          {t("aiGatewayMappingPrice", "Price (USD / million tokens)")}
        </span>
        <span className="text-[10px] text-muted-foreground">
          {t("aiGatewayPricePerMillion", "USD / million tokens")}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {TIERS.map((tier) => (
          <div key={tier.suffix}>
            <label className="mb-1 block text-[10px] text-muted-foreground">
              {t(tier.labelKey, tier.suffix)}
            </label>
            <input
              type="number"
              step="any"
              value={tier.get(draft)}
              data-testid={`ai-gateway-price-${index}-${tier.suffix}`}
              aria-label={t(tier.labelKey, tier.suffix)}
              onChange={(event) => onChange(tier.patch(event.target.value))}
              className={`${tierInputClass} min-w-[5rem]`}
            />
          </div>
        ))}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <label className="inline-flex cursor-pointer select-none items-center gap-2 text-xs font-medium text-foreground">
          <input
            type="checkbox"
            checked={draft.enable_off_peak}
            data-testid={`ai-gateway-price-${index}-off-peak`}
            onChange={(event) => {
              const checked = event.target.checked;
              onChange({
                enable_off_peak: checked,
                off_peaks:
                  checked && draft.off_peaks.length === 0
                    ? [blankOffPeak()]
                    : draft.off_peaks,
              });
            }}
            className="h-3.5 w-3.5 rounded border-border text-primary focus:ring-primary"
          />
          <Moon className="h-3 w-3 text-muted-foreground" />
          <span>{t("aiGatewayOffPeakEnable", "Enable off-peak pricing")}</span>
        </label>

        {draft.enable_off_peak ? (
          <button
            type="button"
            data-testid={`ai-gateway-off-peak-${index}-add`}
            onClick={() =>
              onChange({ off_peaks: [...draft.off_peaks, blankOffPeak()] })
            }
            className="inline-flex h-7 items-center gap-1 rounded-md border border-border bg-background px-2 text-xs font-medium transition hover:bg-muted"
          >
            <Plus className="h-3 w-3" />
            <span>{t("aiGatewayOffPeakAdd", "Add off-peak window")}</span>
          </button>
        ) : null}
      </div>

      {draft.enable_off_peak ? (
        <div className="space-y-2">
          <p className="text-[11px] text-muted-foreground">
            {t(
              "aiGatewayOffPeakHint",
              "Calls during this window in UTC+8 use these discounted rates; standard rates apply otherwise.",
            )}
          </p>

          {draft.off_peaks.map((op, windowIndex) => (
            <div
              key={op.id}
              className="space-y-2 rounded-md border border-border/70 bg-card/60 p-2.5"
            >
              <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/40 pb-2">
                <div className="flex flex-wrap items-center gap-1.5 text-xs">
                  <span className="text-[11px] font-semibold text-foreground">
                    {t("aiGatewayOffPeakWindowIndex", {
                      index: windowIndex + 1,
                      defaultValue: `Off-peak window #${windowIndex + 1}`,
                    })}
                  </span>
                  <span className="font-medium text-muted-foreground">
                    {t("aiGatewayOffPeakTimeRange", "Off-peak window (UTC+8)")}:
                  </span>
                  <input
                    type="text"
                    value={op.start_time}
                    data-testid={`ai-gateway-off-peak-${index}-start-${windowIndex}`}
                    onChange={(event) =>
                      updateOffPeak(windowIndex, { start_time: event.target.value })
                    }
                    placeholder="00:30"
                    aria-label={t("aiGatewayOffPeakStartTime", "Start")}
                    className="w-16 rounded border border-border bg-background px-2 py-1 text-center font-mono text-xs focus:border-primary focus:outline-none"
                  />
                  <span className="text-muted-foreground">-</span>
                  <input
                    type="text"
                    value={op.end_time}
                    data-testid={`ai-gateway-off-peak-${index}-end-${windowIndex}`}
                    onChange={(event) =>
                      updateOffPeak(windowIndex, { end_time: event.target.value })
                    }
                    placeholder="08:30"
                    aria-label={t("aiGatewayOffPeakEndTime", "End")}
                    className="w-16 rounded border border-border bg-background px-2 py-1 text-center font-mono text-xs focus:border-primary focus:outline-none"
                  />
                  <button
                    type="button"
                    data-testid={`ai-gateway-off-peak-${index}-remove-${windowIndex}`}
                    onClick={() =>
                      onChange({
                        off_peaks: draft.off_peaks.filter(
                          (_, opIndex) => opIndex !== windowIndex,
                        ),
                      })
                    }
                    aria-label={t(
                      "aiGatewayOffPeakDeleteAria",
                      "Delete this off-peak window",
                    )}
                    className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
              </div>

              <div>
                <div className="mb-1.5 text-[11px] font-medium text-muted-foreground">
                  {t("aiGatewayOffPeakRates", "Off-peak rates ($/1M tokens)")}
                </div>
                <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                  {TIERS.map((tier) => (
                    <div key={tier.suffix}>
                      <label className="mb-1 block text-[10px] text-muted-foreground">
                        {t(tier.labelKey, tier.suffix)}
                      </label>
                      <input
                        type="number"
                        step="any"
                        value={tier.get(op)}
                        data-testid={`ai-gateway-off-peak-${index}-${tier.suffix}-${windowIndex}`}
                        aria-label={`${t(tier.labelKey, tier.suffix)} (${t(
                          "aiGatewayOffPeakConfigure",
                          "Off-peak discount",
                        )})`}
                        onChange={(event) =>
                          updateOffPeak(windowIndex, tier.patch(event.target.value))
                        }
                        className={`${tierInputClass} min-w-[5rem]`}
                      />
                    </div>
                  ))}
                </div>
              </div>

              <div className="space-y-1.5">
                <div className="text-[11px] font-medium text-muted-foreground">
                  {t("aiGatewayWeekdaySelect", "Apply to weekdays")}
                </div>
                <div className="flex flex-wrap items-center gap-1">
                  {GATEWAY_WEEKDAY_ORDER.map((day) => {
                    const selected = op.days.includes(day);
                    return (
                      <button
                        key={day}
                        type="button"
                        data-testid={`ai-gateway-off-peak-${index}-day-${windowIndex}-${day}`}
                        aria-pressed={selected}
                        aria-label={t(gatewayWeekdayTranslationKey(day))}
                        onClick={() => {
                          const next = selected
                            ? op.days.filter((value) => value !== day)
                            : [...op.days, day];
                          updateOffPeak(windowIndex, {
                            days: normalizeDraftDays(next),
                          });
                        }}
                        className={`inline-flex h-6 min-w-[2rem] items-center justify-center rounded-full border px-1.5 text-[11px] font-medium transition ${
                          selected
                            ? "border-primary bg-primary text-primary-foreground"
                            : "border-border bg-background text-muted-foreground hover:bg-muted hover:text-foreground"
                        }`}
                      >
                        {t(gatewayWeekdayTranslationKey(day))}
                      </button>
                    );
                  })}
                  <button
                    type="button"
                    data-testid={`ai-gateway-off-peak-${index}-everyday-${windowIndex}`}
                    onClick={() => updateOffPeak(windowIndex, { days: [] })}
                    className="inline-flex h-6 items-center rounded-full border border-dashed border-border px-2 text-[11px] font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground"
                  >
                    {t("aiGatewayEveryDay", "Every day")}
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
