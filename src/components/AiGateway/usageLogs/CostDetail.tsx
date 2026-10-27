import { useId, useLayoutEffect, useRef, useState } from "react";
import { Info } from "lucide-react";
import type { TFunction } from "i18next";
import {
  formatUsageAmount,
  formatUsageCurrency,
  type UsageLogRecord,
} from "@/lib/aiGateway";
import { formatCostRate } from "./formatting";

export function CostDetail({
  item,
  t,
}: {
  item: UsageLogRecord;
  t: TFunction;
}) {
  const detailId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const detailRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const breakdown = item.amount == null ? null : item.cost_breakdown;

  const rows = [
    {
      label: t("aiGatewayLogsTokensInput", "Input"),
      cost: formatUsageCurrency(breakdown?.input_cost),
      rate: formatCostRate(breakdown?.input_price),
    },
    {
      label: t("aiGatewayLogsTokensOutput", "Output"),
      cost: formatUsageCurrency(breakdown?.output_cost),
      rate: formatCostRate(breakdown?.output_price),
    },
    {
      label: t("aiGatewayLogsTokensCacheRead", "Cache read"),
      cost: formatUsageCurrency(breakdown?.cache_read_cost),
      rate: formatCostRate(breakdown?.cache_read_price),
    },
    ...(item.cache_write_tokens > 0
      ? [{
          label: t("aiGatewayLogsTokensCacheWrite", "Cache write"),
          cost: formatUsageCurrency(breakdown?.cache_write_cost),
          rate: formatCostRate(breakdown?.cache_write_price),
        }]
      : []),
  ];

  useLayoutEffect(() => {
    if (!open) return;
    const positionDetail = () => {
      const trigger = triggerRef.current;
      const detail = detailRef.current;
      if (!trigger || !detail) return;
      const anchor = trigger.getBoundingClientRect();
      const { width, height } = detail.getBoundingClientRect();
      const showAbove = anchor.bottom + height > window.innerHeight - 8;
      detail.style.left = `${Math.max(8, Math.min(anchor.right - width, window.innerWidth - width - 8))}px`;
      detail.style.top = `${Math.max(8, showAbove ? anchor.top - height : anchor.bottom)}px`;
    };
    positionDetail();
    window.addEventListener("resize", positionDetail);
    document.addEventListener("scroll", positionDetail, true);
    return () => {
      window.removeEventListener("resize", positionDetail);
      document.removeEventListener("scroll", positionDetail, true);
    };
  }, [open, item, t]);

  return (
    <div
      className="inline-flex items-center gap-1.5"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={(event) => {
        if (!event.currentTarget.contains(document.activeElement)) setOpen(false);
      }}
      onFocus={() => setOpen(true)}
      onBlur={(event) => {
        if (!event.currentTarget.matches(":hover")) setOpen(false);
      }}
      onKeyDown={(event) => {
        if (event.key === "Escape") setOpen(false);
      }}
    >
      <span>{item.amount === null ? "—" : formatUsageAmount(item.amount)}</span>
      <button
        ref={triggerRef}
        type="button"
        aria-label={t("aiGatewayLogsCostDetail", "Cost details")}
        aria-describedby={detailId}
        className="inline-flex items-center justify-center rounded p-0.5 text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground focus:outline-none focus:ring-1 focus:ring-primary/40"
      >
        <Info className="h-3 w-3" aria-hidden="true" />
      </button>
      <div
        ref={detailRef}
        id={detailId}
        role="tooltip"
        className={`fixed z-50 w-72 max-w-[calc(100vw-1rem)] max-h-[calc(100vh-1rem)] overflow-y-auto flex-col gap-1 rounded-md border bg-popover p-2.5 text-left text-xs whitespace-normal text-popover-foreground shadow-lg ${open ? "flex" : "hidden"}`}
      >
        <div className="border-b pb-1 text-[11px] font-semibold text-muted-foreground">
          {t("aiGatewayLogsCostDetail", "Cost details")}
        </div>
        <div className="space-y-0.5 pt-0.5 text-[11px]">
          <div className="text-muted-foreground">{t("aiGatewayLogsCostFees", "Fees (USD)")}</div>
          <dl className="space-y-0.5">
            {rows.map((row) => (
              <div className="flex items-center justify-between gap-3" key={row.label}>
                <dt className="text-muted-foreground">{row.label}</dt>
                <dd className="text-right font-mono font-medium">{row.cost}</dd>
              </div>
            ))}
          </dl>
          <div className="flex items-center justify-between gap-4 font-semibold">
            <span>{t("aiGatewayLogsCostDetailTotal", "Total fee")}</span>
            <span className="font-mono">{formatUsageCurrency(item.amount)}</span>
          </div>
        </div>
        <div className="border-t pt-1 text-[11px]">
          <div className="text-muted-foreground">{t("aiGatewayLogsCostUnitRates", "Unit rates")}</div>
          <div className="mb-1 text-[10px] text-muted-foreground">
            {t("aiGatewayPricePerMillion", "USD / million tokens")}
          </div>
          <dl className="space-y-0.5">
            {rows.map((row) => (
              <div className="flex items-center justify-between gap-3" key={row.label}>
                <dt className="text-muted-foreground">{row.label}</dt>
                <dd className="text-right font-mono font-medium">{row.rate}</dd>
              </div>
            ))}
          </dl>
        </div>
        {!breakdown || rows.some((row) => row.cost === "—" || row.rate === "—") ? (
          <p className="border-t pt-1 text-[10px] text-muted-foreground">
            {t(
              "aiGatewayLogsCostDetailUnavailable",
              "Recorded pricing details are unavailable; unknown fees and rates are shown as —.",
            )}
          </p>
        ) : null}
      </div>
    </div>
  );
}
