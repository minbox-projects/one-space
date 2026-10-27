import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import {
  sampleMetrics,
  sampleOutcome,
  type ServerMonitorHistoryRange,
  type ServerMonitorSample,
} from "@/lib/serverMonitor";

const WIDTH = 640;
const HEIGHT = 180;
const PADDING = 12;

type Point = {
  timestamp: number;
  percent: number;
};

function rangeDuration(range: ServerMonitorHistoryRange): number {
  return range === "sevenDays" ? 7 * 24 * 60 * 60 * 1000 : 24 * 60 * 60 * 1000;
}

export interface HistoryChartProps {
  samples: ServerMonitorSample[];
  range: ServerMonitorHistoryRange;
  expectedIntervalSeconds?: number | null;
  className?: string;
}

/**
 * Lightweight SVG trend. Daily observations are discrete points; interval
 * observations connect only across successful samples whose spacing stays
 * within twice the interval recorded with that sample.
 */
export function HistoryChart({
  samples,
  range,
  expectedIntervalSeconds,
  className,
}: HistoryChartProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";

  const chart = useMemo(() => {
    const ordered = [...samples]
      .filter((sample) => Number.isFinite(sample.collectedAtMs))
      .sort((a, b) => a.collectedAtMs - b.collectedAtMs);
    const duration = rangeDuration(range);
    const end = ordered.length > 0 ? ordered[ordered.length - 1].collectedAtMs : 0;
    const start = end - duration;
    const x = (timestamp: number) =>
      PADDING + ((timestamp - start) / duration) * (WIDTH - PADDING * 2);
    const y = (percent: number) =>
      HEIGHT - PADDING - (Math.min(100, Math.max(0, percent)) / 100) * (HEIGHT - PADDING * 2);

    const points: Point[] = [];
    const segments: Point[][] = [];
    let current: Point[] = [];
    let previous: ServerMonitorSample | null = null;

    const breakSegment = () => {
      if (current.length > 1) segments.push(current);
      current = [];
    };

    for (const sample of ordered) {
      const before = sampleOutcome(sample);
      const metrics = sampleMetrics(sample);
      if (before === "failed" || !metrics?.cpu) {
        breakSegment();
        previous = sample;
        continue;
      }
      const point = {
        timestamp: sample.collectedAtMs,
        percent: metrics.cpu.usagePercent,
      };
      points.push(point);

      const gapLimitSeconds = sample.expectedIntervalSeconds ?? expectedIntervalSeconds ?? 0;
      const gapLimitMs = gapLimitSeconds > 0 ? gapLimitSeconds * 2 * 1000 : 0;
      const sameIntervalStream =
        sample.sourceMode === "interval" && previous?.sourceMode === "interval";
      const withinGap =
        gapLimitMs > 0 &&
        previous !== null &&
        sample.collectedAtMs - previous.collectedAtMs <= gapLimitMs;

      if (current.length === 0) {
        current.push(point);
      } else if (sameIntervalStream && withinGap && sampleOutcome(previous!) !== "failed") {
        current.push(point);
      } else {
        breakSegment();
        current.push(point);
      }
      previous = sample;
    }
    breakSegment();

    const pathFor = (segment: Point[]) =>
      segment
        .map((point, index) => `${index === 0 ? "M" : "L"} ${x(point.timestamp).toFixed(1)} ${y(point.percent).toFixed(1)}`)
        .join(" ");

    return {
      points,
      segments: segments.map(pathFor),
      x,
      y,
      lastPercent: points.length > 0 ? points[points.length - 1].percent : null,
    };
  }, [samples, range, expectedIntervalSeconds]);

  const title = `${t("serverMonitorHistory")} · ${t(
    range === "sevenDays" ? "serverMonitorRange7d" : "serverMonitorRange24h",
  )} / ${i18n.getFixedT(alternate)("serverMonitorHistory")}`;

  return (
    <div className={className}>
      {chart.points.length === 0 ? (
        <p className="rounded-lg border border-dashed border-border p-4 text-xs text-muted-foreground">
          {t("serverMonitorHistoryEmpty")}
          <span className="sr-only"> {i18n.getFixedT(alternate)("serverMonitorHistoryEmpty")}</span>
        </p>
      ) : (
        <>
          <div className="mb-1 flex items-center justify-between text-xs text-muted-foreground">
            <span>
              {t("serverMonitorCpu")}
              <span className="sr-only"> {i18n.getFixedT(alternate)("serverMonitorCpu")}</span>
            </span>
          </div>
          <svg
            role="img"
            aria-label={title}
            viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
            className="h-44 w-full overflow-visible"
            preserveAspectRatio="none"
          >
            <title>{title}</title>
            {[25, 50, 75].map((percent) => (
              <line
                key={percent}
                x1={PADDING}
                x2={WIDTH - PADDING}
                y1={chart.y(percent)}
                y2={chart.y(percent)}
                className="stroke-border/60"
                strokeWidth={1}
              />
            ))}
            {chart.segments.map((path, index) => (
              <path
                key={`segment-${index}`}
                d={path}
                fill="none"
                className="stroke-primary"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            ))}
            {chart.points.map((point, index) => (
              <circle
                key={`${point.timestamp}-${index}`}
                cx={chart.x(point.timestamp)}
                cy={chart.y(point.percent)}
                r={2.5}
                className="fill-primary"
              />
            ))}
          </svg>
        </>
      )}
    </div>
  );
}
