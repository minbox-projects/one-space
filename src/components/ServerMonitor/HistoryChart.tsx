import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  formatBytes,
  formatMonitorTimestamp,
  formatPercent,
  formatRate,
  latestGoodSample,
  sampleMetrics,
  sampleOutcome,
  selectNetworkInterface,
  type MetricSnapshot,
  type ServerMonitorHistoryRange,
  type ServerMonitorSample,
} from "@/lib/serverMonitor";

const WIDTH = 640;
const HEIGHT = 180;
const PADDING = 12;

type TrendPoint = {
  timestamp: number;
  value: number;
};

export type HistoryMetricType = "cpu" | "memory" | "disk" | "network";

interface ValidChartItem {
  sample: ServerMonitorSample;
  metrics: MetricSnapshot;
  timestamp: number;
  value: number;
  txValue?: number;
  cpu?: number;
  memory?: {
    usedPercent: number;
    usedBytes: number;
    totalBytes: number;
  };
  disk?: {
    mountPoint: string;
    usedPercent: number;
    usedBytes: number;
    totalBytes: number;
  };
  network?: {
    iface: string;
    rxBytesPerSec: number;
    txBytesPerSec: number;
  };
}

function rangeDuration(range: ServerMonitorHistoryRange): number {
  return range === "sevenDays" ? 7 * 24 * 60 * 60 * 1000 : 24 * 60 * 60 * 1000;
}

function computeCeilingRate(maxRate: number): number {
  if (maxRate <= 0) return 1024 * 1024;
  const target = maxRate * 1.2;
  const magnitude = Math.pow(10, Math.floor(Math.log10(target)));
  const ratio = target / magnitude;
  let factor = 10;
  if (ratio <= 1.2) factor = 1.2;
  else if (ratio <= 2) factor = 2;
  else if (ratio <= 2.5) factor = 2.5;
  else if (ratio <= 5) factor = 5;
  return Math.max(1024, factor * magnitude);
}

function buildSeries(
  ordered: ServerMonitorSample[],
  getValue: (sample: ServerMonitorSample, metrics: MetricSnapshot) => number | null,
  expectedIntervalSeconds?: number | null,
): {
  points: TrendPoint[];
  segments: TrendPoint[][];
} {
  const points: TrendPoint[] = [];
  const segments: TrendPoint[][] = [];
  let current: TrendPoint[] = [];
  let previous: ServerMonitorSample | null = null;

  const breakSegment = () => {
    if (current.length > 1) segments.push(current);
    current = [];
  };

  for (const sample of ordered) {
    const outcome = sampleOutcome(sample);
    const metrics = sampleMetrics(sample);
    const value = outcome !== "failed" && metrics ? getValue(sample, metrics) : null;
    if (value === null || !Number.isFinite(value)) {
      breakSegment();
      previous = sample;
      continue;
    }

    const point: TrendPoint = {
      timestamp: sample.collectedAtMs,
      value,
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

  return { points, segments };
}

export interface HistoryChartProps {
  samples: ServerMonitorSample[];
  range: ServerMonitorHistoryRange;
  expectedIntervalSeconds?: number | null;
  selectedInterface?: string | null;
  onSelectInterface?: (iface: string) => void;
  className?: string;
}

/**
 * Lightweight SVG trend for server metrics (CPU, Memory, Disk, Network).
 * Daily observations are discrete points; interval observations connect
 * only across successful samples whose spacing stays within twice the interval.
 */
export function HistoryChart({
  samples,
  range,
  expectedIntervalSeconds,
  selectedInterface,
  onSelectInterface,
  className,
}: HistoryChartProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";

  const [activeMetric, setActiveMetric] = useState<HistoryMetricType>("cpu");
  const [internalInterface, setInternalInterface] = useState<string | null>(null);
  const [selectedMountPoint, setSelectedMountPoint] = useState<string | null>(null);
  const [hoveredItem, setHoveredItem] = useState<ValidChartItem | null>(null);

  // Extract all available mount points across samples
  const availableMountPoints = useMemo(() => {
    const set = new Set<string>();
    for (const sample of samples) {
      const metrics = sampleMetrics(sample);
      if (Array.isArray(metrics?.disks)) {
        for (const disk of metrics.disks) {
          if (disk.mountPoint) set.add(disk.mountPoint);
        }
      }
    }
    return Array.from(set);
  }, [samples]);

  // Extract all available network interfaces across samples
  const availableInterfaces = useMemo(() => {
    const set = new Set<string>();
    for (const sample of samples) {
      const metrics = sampleMetrics(sample);
      const nets = Array.isArray(metrics?.networks) && metrics.networks.length > 0
        ? metrics.networks
        : Array.isArray(sample.networks)
          ? sample.networks
          : [];
      for (const net of nets) {
        if (net.interface) set.add(net.interface);
      }
      if (metrics?.network?.interface) {
        set.add(metrics.network.interface);
      } else if (sample.networkInterface) {
        set.add(sample.networkInterface);
      }
    }
    return Array.from(set);
  }, [samples]);

  const effectiveInterface =
    selectedInterface ?? internalInterface ?? availableInterfaces[0] ?? null;

  const handleInterfaceChange = (iface: string) => {
    setInternalInterface(iface);
    setHoveredItem(null);
    onSelectInterface?.(iface);
  };

  const effectiveMountPoint = useMemo(() => {
    if (selectedMountPoint && availableMountPoints.includes(selectedMountPoint)) {
      return selectedMountPoint;
    }
    const goodSample = latestGoodSample(samples);
    const goodMetrics = sampleMetrics(goodSample);
    if (Array.isArray(goodMetrics?.disks) && goodMetrics.disks.length > 0) {
      const busiest = goodMetrics.disks.reduce((max, d) =>
        d.usedPercent > max.usedPercent ? d : max,
      );
      return busiest.mountPoint;
    }
    return availableMountPoints[0] ?? null;
  }, [selectedMountPoint, availableMountPoints, samples]);

  // Chart computation for active metric
  const chartData = useMemo(() => {
    const ordered = [...samples]
      .filter((sample) => Number.isFinite(sample.collectedAtMs))
      .sort((a, b) => a.collectedAtMs - b.collectedAtMs);

    const duration = rangeDuration(range);
    const end = ordered.length > 0 ? ordered[ordered.length - 1].collectedAtMs : 0;
    const start = end - duration;
    const x = (timestamp: number) =>
      PADDING + ((timestamp - start) / duration) * (WIDTH - PADDING * 2);

    const pathFor = (segment: TrendPoint[], y: (v: number) => number) =>
      segment
        .map((point, index) => `${index === 0 ? "M" : "L"} ${x(point.timestamp).toFixed(1)} ${y(point.value).toFixed(1)}`)
        .join(" ");

    const areaFor = (segment: TrendPoint[], y: (v: number) => number) => {
      if (segment.length < 2) return "";
      const first = segment[0];
      const last = segment[segment.length - 1];
      const bottomY = HEIGHT - PADDING;
      const linePart = segment
        .map((point, index) => `${index === 0 ? "M" : "L"} ${x(point.timestamp).toFixed(1)} ${y(point.value).toFixed(1)}`)
        .join(" ");
      return `${linePart} L ${x(last.timestamp).toFixed(1)} ${bottomY} L ${x(first.timestamp).toFixed(1)} ${bottomY} Z`;
    };

    if (activeMetric === "network") {
      const rxSeries = buildSeries(
        ordered,
        (sample, metrics) => {
          const net = selectNetworkInterface(metrics ?? sample, effectiveInterface);
          return net ? net.rxBytesPerSec : null;
        },
        expectedIntervalSeconds,
      );

      const txSeries = buildSeries(
        ordered,
        (sample, metrics) => {
          const net = selectNetworkInterface(metrics ?? sample, effectiveInterface);
          return net ? net.txBytesPerSec : null;
        },
        expectedIntervalSeconds,
      );

      const maxVal = Math.max(
        ...rxSeries.points.map((p) => p.value),
        ...txSeries.points.map((p) => p.value),
        0,
      );
      const ceiling = computeCeilingRate(maxVal);

      const y = (rate: number) =>
        HEIGHT - PADDING - (Math.min(ceiling, Math.max(0, rate)) / ceiling) * (HEIGHT - PADDING * 2);

      const hasData = rxSeries.points.length > 0 || txSeries.points.length > 0;
      const lastRx = rxSeries.points.length > 0 ? rxSeries.points[rxSeries.points.length - 1].value : null;
      const lastTx = txSeries.points.length > 0 ? txSeries.points[txSeries.points.length - 1].value : null;

      // Build validItems for hover
      const validItems: ValidChartItem[] = [];
      for (const sample of ordered) {
        if (sampleOutcome(sample) === "failed") continue;
        const metrics = sampleMetrics(sample);
        if (!metrics) continue;
        const net = selectNetworkInterface(metrics ?? sample, effectiveInterface);
        if (!net) continue;
        validItems.push({
          sample,
          metrics,
          timestamp: sample.collectedAtMs,
          value: net.rxBytesPerSec,
          txValue: net.txBytesPerSec,
          network: {
            iface: net.interface,
            rxBytesPerSec: net.rxBytesPerSec,
            txBytesPerSec: net.txBytesPerSec,
          },
        });
      }

      return {
        isRate: true as const,
        hasData,
        x,
        y,
        ceiling,
        validItems,
        rx: {
          points: rxSeries.points,
          segments: rxSeries.segments.map((s) => pathFor(s, y)),
          areas: rxSeries.segments.map((s) => areaFor(s, y)).filter(Boolean),
          last: lastRx,
        },
        tx: {
          points: txSeries.points,
          segments: txSeries.segments.map((s) => pathFor(s, y)),
          last: lastTx,
        },
      };
    }

    // Percentage-based metrics (CPU, Memory, Disk)
    let getValue: (sample: ServerMonitorSample, metrics: MetricSnapshot) => number | null;
    if (activeMetric === "memory") {
      getValue = (_sample, metrics) => metrics.memory?.usedPercent ?? null;
    } else if (activeMetric === "disk") {
      getValue = (_sample, metrics) => {
        if (!Array.isArray(metrics.disks) || metrics.disks.length === 0) return null;
        if (effectiveMountPoint) {
          const match = metrics.disks.find((d) => d.mountPoint === effectiveMountPoint);
          if (match) return match.usedPercent;
        }
        return metrics.disks[0].usedPercent;
      };
    } else {
      getValue = (_sample, metrics) => metrics.cpu?.usagePercent ?? null;
    }

    const series = buildSeries(ordered, getValue, expectedIntervalSeconds);
    const y = (percent: number) =>
      HEIGHT - PADDING - (Math.min(100, Math.max(0, percent)) / 100) * (HEIGHT - PADDING * 2);

    // Build validItems for hover
    const validItems: ValidChartItem[] = [];
    for (const sample of ordered) {
      if (sampleOutcome(sample) === "failed") continue;
      const metrics = sampleMetrics(sample);
      if (!metrics) continue;
      const val = getValue(sample, metrics);
      if (val === null || !Number.isFinite(val)) continue;

      const item: ValidChartItem = {
        sample,
        metrics,
        timestamp: sample.collectedAtMs,
        value: val,
      };

      if (activeMetric === "cpu" && metrics.cpu) {
        item.cpu = metrics.cpu.usagePercent;
      } else if (activeMetric === "memory" && metrics.memory) {
        item.memory = {
          usedPercent: metrics.memory.usedPercent,
          usedBytes: metrics.memory.usedBytes,
          totalBytes: metrics.memory.totalBytes,
        };
      } else if (activeMetric === "disk" && Array.isArray(metrics.disks)) {
        const disk = effectiveMountPoint
          ? metrics.disks.find((d) => d.mountPoint === effectiveMountPoint) ?? metrics.disks[0]
          : metrics.disks[0];
        if (disk) {
          item.disk = {
            mountPoint: disk.mountPoint,
            usedPercent: disk.usedPercent,
            usedBytes: disk.usedBytes,
            totalBytes: disk.totalBytes,
          };
        }
      }
      validItems.push(item);
    }

    return {
      isRate: false as const,
      hasData: series.points.length > 0,
      x,
      y,
      validItems,
      points: series.points,
      segments: series.segments.map((s) => pathFor(s, y)),
      areas: series.segments.map((s) => areaFor(s, y)).filter(Boolean),
      last: series.points.length > 0 ? series.points[series.points.length - 1].value : null,
    };
  }, [samples, range, activeMetric, effectiveInterface, effectiveMountPoint, expectedIntervalSeconds]);

  const handlePointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!chartData.hasData || chartData.validItems.length === 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) return;
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const currentSvgX = PADDING + ratio * (WIDTH - PADDING * 2);

    let closest = chartData.validItems[0];
    let minDiff = Math.abs(chartData.x(closest.timestamp) - currentSvgX);
    for (let i = 1; i < chartData.validItems.length; i++) {
      const item = chartData.validItems[i];
      const diff = Math.abs(chartData.x(item.timestamp) - currentSvgX);
      if (diff < minDiff) {
        minDiff = diff;
        closest = item;
      }
    }
    setHoveredItem(closest);
  };

  const handlePointerLeave = () => {
    setHoveredItem(null);
  };

  const metricLabel = useMemo(() => {
    switch (activeMetric) {
      case "cpu":
        return t("serverMonitorCpu");
      case "memory":
        return t("serverMonitorMemory");
      case "disk":
        return t("serverMonitorDisk");
      case "network":
        return t("serverMonitorNetwork");
    }
  }, [activeMetric, t]);

  const title = `${t("serverMonitorHistory")} · ${metricLabel} · ${t(
    range === "sevenDays" ? "serverMonitorRange7d" : "serverMonitorRange24h",
  )} / ${i18n.getFixedT(alternate)("serverMonitorHistory")}`;

  const hoveredX = hoveredItem ? chartData.x(hoveredItem.timestamp) : 0;
  const hoveredLeftPercent = hoveredItem
    ? Math.max(0, Math.min(100, (hoveredX / WIDTH) * 100))
    : 0;
  const flipTooltip = hoveredLeftPercent > 60;

  return (
    <div className={className}>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center rounded-xl bg-slate-100 p-0.5 dark:bg-slate-800">
          {(["cpu", "memory", "disk", "network"] as const).map((tab) => (
            <button
              key={tab}
              type="button"
              onClick={() => {
                setActiveMetric(tab);
                setHoveredItem(null);
              }}
              className={`rounded-lg px-2.5 py-1 text-xs font-medium transition ${
                activeMetric === tab
                  ? "bg-white font-semibold text-slate-900 shadow-sm dark:bg-slate-700 dark:text-white"
                  : "text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
              }`}
            >
              {tab === "cpu" && "CPU"}
              {tab === "memory" && t("serverMonitorMemory")}
              {tab === "disk" && t("serverMonitorDisk")}
              {tab === "network" && t("serverMonitorNetwork")}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          {activeMetric === "disk" && availableMountPoints.length > 1 ? (
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] font-medium text-slate-500 dark:text-slate-400">
                {t("serverMonitorMountPoint")}:
              </span>
              <select
                aria-label={t("serverMonitorMountPoint")}
                value={effectiveMountPoint ?? ""}
                onChange={(e) => {
                  setSelectedMountPoint(e.target.value);
                  setHoveredItem(null);
                }}
                className="h-6 rounded-md border border-slate-200 bg-slate-50/50 px-2 text-[11px] font-mono font-medium text-slate-800 transition-all hover:border-slate-300 focus:border-emerald-500 focus:outline-none dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200"
              >
                {availableMountPoints.map((mp) => (
                  <option key={mp} value={mp} label={mp}>
                    {mp}
                  </option>
                ))}
              </select>
            </div>
          ) : null}

          {activeMetric === "network" && availableInterfaces.length > 1 ? (
            <div className="flex items-center gap-1.5">
              <span className="text-[11px] font-medium text-slate-500 dark:text-slate-400">
                {t("serverMonitorNetworkInterface")}:
              </span>
              <select
                aria-label={t("serverMonitorNetworkInterface")}
                value={effectiveInterface ?? ""}
                onChange={(e) => handleInterfaceChange(e.target.value)}
                className="h-6 rounded-md border border-slate-200 bg-slate-50/50 px-2 text-[11px] font-mono font-medium text-slate-800 transition-all hover:border-slate-300 focus:border-emerald-500 focus:outline-none dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200"
              >
                {availableInterfaces.map((iface) => (
                  <option key={iface} value={iface} label={iface}>
                    {iface}
                  </option>
                ))}
              </select>
            </div>
          ) : null}

          {activeMetric === "network" ? (
            <div className="flex items-center gap-3 font-mono text-[11px]">
              <span className="flex items-center gap-1 text-emerald-600 dark:text-emerald-400">
                <span className="h-2 w-2 rounded-full bg-emerald-500" />
                {t("serverMonitorRx")}
              </span>
              <span className="flex items-center gap-1 text-sky-600 dark:text-sky-400">
                <span className="h-2 w-2 rounded-full bg-sky-500" />
                {t("serverMonitorTx")}
              </span>
            </div>
          ) : null}
        </div>
      </div>

      {!chartData.hasData ? (
        <div className="flex h-36 flex-col items-center justify-center rounded-xl border border-dashed border-border/80 bg-muted/10 p-6 text-center">
          <p className="text-xs text-muted-foreground">
            {t("serverMonitorHistoryEmpty")}
            <span className="sr-only"> {i18n.getFixedT(alternate)("serverMonitorHistoryEmpty")}</span>
          </p>
        </div>
      ) : (
        <div className="relative w-full">
          <svg
            role="img"
            aria-label={title}
            viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
            className="h-44 w-full overflow-hidden cursor-crosshair touch-none"
            preserveAspectRatio="none"
            onPointerMove={handlePointerMove}
            onPointerLeave={handlePointerLeave}
          >
            <title>{title}</title>
            <defs>
              <linearGradient id="serverMonitorChartGradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="hsl(var(--primary))" stopOpacity="0.28" />
                <stop offset="100%" stopColor="hsl(var(--primary))" stopOpacity="0.0" />
              </linearGradient>
              <linearGradient id="serverMonitorNetworkRxGradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="#10b981" stopOpacity="0.25" />
                <stop offset="100%" stopColor="#10b981" stopOpacity="0.0" />
              </linearGradient>
            </defs>

            {chartData.isRate ? (
              <>
                {[0.25, 0.5, 0.75].map((ratio) => {
                  const val = chartData.ceiling * ratio;
                  return (
                    <g key={ratio}>
                      <line
                        x1={PADDING}
                        x2={WIDTH - PADDING}
                        y1={chartData.y(val)}
                        y2={chartData.y(val)}
                        className="stroke-border/50"
                        strokeWidth={1}
                        strokeDasharray="4 4"
                      />
                      <text
                        x={WIDTH - PADDING + 4}
                        y={chartData.y(val) + 3}
                        className="fill-muted-foreground/60 text-[9px] font-mono select-none"
                      >
                        {formatRate(val)}
                      </text>
                    </g>
                  );
                })}

                {chartData.rx.areas.map((path, index) => (
                  <path
                    key={`rx-area-${index}`}
                    d={path}
                    fill="url(#serverMonitorNetworkRxGradient)"
                    stroke="none"
                  />
                ))}

                {chartData.rx.segments.map((path, index) => (
                  <path
                    key={`rx-segment-${index}`}
                    d={path}
                    fill="none"
                    className="stroke-emerald-500"
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                ))}

                {chartData.tx.segments.map((path, index) => (
                  <path
                    key={`tx-segment-${index}`}
                    d={path}
                    fill="none"
                    className="stroke-sky-500"
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                ))}

                {chartData.rx.points.map((point, index) => (
                  <circle
                    key={`rx-pt-${point.timestamp}-${index}`}
                    cx={chartData.x(point.timestamp)}
                    cy={chartData.y(point.value)}
                    r={2.5}
                    className="fill-emerald-500"
                  />
                ))}

                {chartData.tx.points.map((point, index) => (
                  <circle
                    key={`tx-pt-${point.timestamp}-${index}`}
                    cx={chartData.x(point.timestamp)}
                    cy={chartData.y(point.value)}
                    r={2.5}
                    className="fill-sky-500"
                  />
                ))}

                {hoveredItem ? (
                  <>
                    <line
                      x1={hoveredX}
                      x2={hoveredX}
                      y1={PADDING}
                      y2={HEIGHT - PADDING}
                      className="stroke-slate-400/70 dark:stroke-slate-500/70"
                      strokeWidth={1}
                      strokeDasharray="3 3"
                    />
                    <circle
                      cx={hoveredX}
                      cy={chartData.y(hoveredItem.network?.rxBytesPerSec ?? 0)}
                      r={4.5}
                      className="fill-emerald-500 stroke-white dark:stroke-slate-900"
                      strokeWidth={2}
                    />
                    <circle
                      cx={hoveredX}
                      cy={chartData.y(hoveredItem.network?.txBytesPerSec ?? 0)}
                      r={4.5}
                      className="fill-sky-500 stroke-white dark:stroke-slate-900"
                      strokeWidth={2}
                    />
                  </>
                ) : null}
              </>
            ) : (
              <>
                {[25, 50, 75].map((percent) => (
                  <g key={percent}>
                    <line
                      x1={PADDING}
                      x2={WIDTH - PADDING}
                      y1={chartData.y(percent)}
                      y2={chartData.y(percent)}
                      className="stroke-border/50"
                      strokeWidth={1}
                      strokeDasharray="4 4"
                    />
                    <text
                      x={WIDTH - PADDING + 4}
                      y={chartData.y(percent) + 3}
                      className="fill-muted-foreground/60 text-[9px] font-mono select-none"
                    >
                      {percent}%
                    </text>
                  </g>
                ))}

                {chartData.areas.map((path, index) => (
                  <path
                    key={`area-${index}`}
                    d={path}
                    fill="url(#serverMonitorChartGradient)"
                    stroke="none"
                  />
                ))}

                {chartData.segments.map((path, index) => (
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

                {chartData.points.map((point, index) => (
                  <circle
                    key={`${point.timestamp}-${index}`}
                    cx={chartData.x(point.timestamp)}
                    cy={chartData.y(point.value)}
                    r={2.5}
                    className="fill-primary"
                  />
                ))}

                {hoveredItem ? (
                  <>
                    <line
                      x1={hoveredX}
                      x2={hoveredX}
                      y1={PADDING}
                      y2={HEIGHT - PADDING}
                      className="stroke-slate-400/70 dark:stroke-slate-500/70"
                      strokeWidth={1}
                      strokeDasharray="3 3"
                    />
                    <circle
                      cx={hoveredX}
                      cy={chartData.y(hoveredItem.value)}
                      r={4.5}
                      className="fill-primary stroke-white dark:stroke-slate-900"
                      strokeWidth={2}
                    />
                  </>
                ) : null}
              </>
            )}
          </svg>

          {hoveredItem ? (
            <div
              data-testid="history-chart-tooltip"
              className={`pointer-events-none absolute top-2 z-20 min-w-[140px] rounded-xl border border-slate-200/90 bg-white/95 p-2.5 text-xs shadow-lg backdrop-blur-sm transition-all duration-75 dark:border-slate-800 dark:bg-slate-900/95 ${
                flipTooltip ? "-translate-x-full -ml-3" : "ml-3"
              }`}
              style={{ left: `${hoveredLeftPercent}%` }}
            >
              <div className="border-b border-slate-100 pb-1 text-[11px] font-mono font-medium text-slate-500 dark:border-slate-800 dark:text-slate-400">
                {formatMonitorTimestamp(hoveredItem.timestamp)}
              </div>

              {activeMetric === "cpu" ? (
                <div className="mt-1.5 flex items-center justify-between gap-4">
                  <span className="font-medium text-slate-600 dark:text-slate-300">
                    {t("serverMonitorCpu")}
                  </span>
                  <span className="font-mono font-semibold text-slate-900 dark:text-white">
                    {formatPercent(hoveredItem.cpu)}
                  </span>
                </div>
              ) : null}

              {activeMetric === "memory" ? (
                <div className="mt-1.5 space-y-0.5">
                  <div className="flex items-center justify-between gap-4">
                    <span className="font-medium text-slate-600 dark:text-slate-300">
                      {t("serverMonitorMemory")}
                    </span>
                    <span className="font-mono font-semibold text-slate-900 dark:text-white">
                      {formatPercent(hoveredItem.memory?.usedPercent)}
                    </span>
                  </div>
                  {hoveredItem.memory ? (
                    <div className="font-mono text-[10px] text-slate-400 dark:text-slate-500">
                      {formatBytes(hoveredItem.memory.usedBytes)} / {formatBytes(hoveredItem.memory.totalBytes)}
                    </div>
                  ) : null}
                </div>
              ) : null}

              {activeMetric === "disk" ? (
                <div className="mt-1.5 space-y-0.5">
                  <div className="flex items-center justify-between gap-4">
                    <span className="font-medium text-slate-600 dark:text-slate-300">
                      {hoveredItem.disk?.mountPoint ?? t("serverMonitorDisk")}
                    </span>
                    <span className="font-mono font-semibold text-slate-900 dark:text-white">
                      {formatPercent(hoveredItem.disk?.usedPercent)}
                    </span>
                  </div>
                  {hoveredItem.disk ? (
                    <div className="font-mono text-[10px] text-slate-400 dark:text-slate-500">
                      {formatBytes(hoveredItem.disk.usedBytes)} / {formatBytes(hoveredItem.disk.totalBytes)}
                    </div>
                  ) : null}
                </div>
              ) : null}

              {activeMetric === "network" ? (
                <div className="mt-1.5 space-y-1">
                  <div className="font-mono text-[10px] font-medium text-slate-400 dark:text-slate-500">
                    {hoveredItem.network?.iface}
                  </div>
                  <div className="flex items-center justify-between gap-4 text-emerald-600 dark:text-emerald-400">
                    <span className="flex items-center gap-1">
                      <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                      {t("serverMonitorRx")}
                    </span>
                    <span className="font-mono font-semibold">
                      {formatRate(hoveredItem.network?.rxBytesPerSec)}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-4 text-sky-600 dark:text-sky-400">
                    <span className="flex items-center gap-1">
                      <span className="h-1.5 w-1.5 rounded-full bg-sky-500" />
                      {t("serverMonitorTx")}
                    </span>
                    <span className="font-mono font-semibold">
                      {formatRate(hoveredItem.network?.txBytesPerSec)}
                    </span>
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}
