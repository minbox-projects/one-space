/**
 * One aligned latency line: a muted short label on the left and a
 * right-aligned tabular value (plus a muted, separate unit) on the right. Only
 * the numeric value carries the performance color; an unavailable value renders
 * a single muted em dash with no unit.
 */
export function LatencyMetricRow({
  labelTestId,
  valueTestId,
  unitTestId,
  label,
  value,
  unit,
  colorClass,
}: {
  labelTestId: string;
  valueTestId: string;
  unitTestId: string;
  label: string;
  value: string;
  unit: string;
  colorClass: string;
}) {
  const available = value !== "—";
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground" data-testid={labelTestId}>
        {label}
      </span>
      <span className="inline-flex items-baseline justify-end gap-1">
        <span
          className={`tabular-nums font-medium ${colorClass}`}
          data-testid={valueTestId}
        >
          {value}
        </span>
        {available ? (
          <span className="text-muted-foreground" data-testid={unitTestId}>
            {unit}
          </span>
        ) : null}
      </span>
    </div>
  );
}
