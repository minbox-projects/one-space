import type { ReactNode } from "react";
import { HardDrive, MemoryStick, Network, Cpu } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  formatBytes,
  formatPercent,
  formatRate,
  type MetricSnapshot,
  type NetworkMetric,
} from "@/lib/serverMonitor";

type BiTextProps = {
  translationKey: string;
  values?: Record<string, string | number>;
};

function BiText({ translationKey, values }: BiTextProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";
  const primary = t(translationKey, values);
  const secondary = i18n.getFixedT(alternate)(translationKey, values);
  return (
    <>
      {primary}
      {secondary && secondary !== primary ? (
        <span className="sr-only"> {secondary}</span>
      ) : null}
    </>
  );
}

function getProgressTone(percent: number): { bar: string; text: string } {
  if (percent >= 85) return { bar: "bg-rose-500", text: "text-rose-500" };
  if (percent >= 70) return { bar: "bg-amber-500", text: "text-amber-500" };
  return { bar: "bg-emerald-500", text: "text-emerald-600 dark:text-emerald-400" };
}

/**
 * 格式化网络接口名称：
 * - 绝对不折行（whitespace-nowrap）；
 * - 当名称过长（超过 maxLen 字符）时，隐藏前部内容，仅展示可容纳的尾部内容，并在最前端显示省略号；
 * - 常见短名称（如 eth0、lo、ens33）完整展示无截断。
 */
export function formatNetworkInterfaceTail(
  name: string,
  maxLen = 9,
): {
  display: string;
  isTruncated: boolean;
  tail: string;
} {
  if (!name || name.length <= maxLen) {
    return { display: name || "—", isTruncated: false, tail: name || "—" };
  }
  const tail = name.slice(-maxLen);
  return {
    display: `…${tail}`,
    isTruncated: true,
    tail,
  };
}

export function NetworkInterfaceDisplay({ name }: { name: string | null }) {
  if (!name) return <>—</>;

  const { isTruncated, tail } = formatNetworkInterfaceTail(name, 9);

  return (
    <span
      title={name}
      className={`inline-flex min-w-0 max-w-full items-baseline overflow-hidden whitespace-nowrap font-mono ${
        name.length > 8 ? "text-xl sm:text-2xl" : "text-2xl"
      }`}
    >
      {isTruncated ? (
        <>
          <span className="shrink-0 text-slate-400 select-none mr-0.5">…</span>
          <span className="min-w-0 truncate">{tail}</span>
          <span className="sr-only">{name}</span>
        </>
      ) : (
        <span className="min-w-0 truncate">{name}</span>
      )}
    </span>
  );
}

function MetricCard({
  icon,
  labelKey,
  value,
  percent,
  detail,
  unavailable,
}: {
  icon: ReactNode;
  labelKey: string;
  value: ReactNode;
  percent?: number | null;
  detail?: ReactNode;
  unavailable?: boolean;
}) {
  const tone = typeof percent === "number" ? getProgressTone(percent) : null;
  const clampedPercent = typeof percent === "number" ? Math.min(100, Math.max(0, percent)) : null;

  return (
    <div className="group min-w-0 overflow-hidden rounded-2xl border border-slate-200/80 bg-white p-4 shadow-sm transition-all hover:border-slate-300 hover:shadow dark:border-slate-800 dark:bg-slate-900 dark:hover:border-slate-700">
      <div className="flex items-center justify-between">
        <span className="truncate text-xs font-medium text-slate-500 dark:text-slate-400">
          <BiText translationKey={labelKey} />
        </span>
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
          {icon}
        </div>
      </div>

      <div className="mt-1.5 flex min-w-0 items-baseline justify-between gap-2 overflow-hidden">
        <div
          className={`min-w-0 max-w-full overflow-hidden text-2xl font-bold tracking-tight font-mono ${
            unavailable ? "text-slate-400" : tone ? tone.text : "text-slate-900 dark:text-white"
          }`}
        >
          {value}
        </div>
      </div>

      {clampedPercent !== null ? (
        <div className="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
          <div
            className={`h-full transition-all duration-300 ${tone?.bar || "bg-emerald-500"}`}
            style={{ width: `${clampedPercent}%` }}
          />
        </div>
      ) : null}

      {detail ? (
        <p className="mt-2 text-[11px] text-slate-500 dark:text-slate-400 break-words truncate">{detail}</p>
      ) : null}
    </div>
  );
}

export interface MetricCardsProps {
  metrics: MetricSnapshot | null;
  networks?: NetworkMetric[];
  selectedInterface?: string | null;
  selectedNetwork?: NetworkMetric | null;
  onSelectInterface?: (value: string) => void;
}

export function MetricCards({
  metrics,
  selectedNetwork,
}: MetricCardsProps) {
  const { t } = useTranslation();

  const cpu = metrics?.cpu ?? null;
  const memory = metrics?.memory ?? null;
  const disks = Array.isArray(metrics?.disks) ? metrics.disks : [];
  const busiestDisk = disks.reduce<(typeof disks)[number] | null>(
    (current, disk) =>
      !current || disk.usedPercent > current.usedPercent ? disk : current,
    null,
  );

  return (
    <div className="space-y-3 min-w-0 max-w-full">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4 min-w-0">
        <MetricCard
          icon={<Cpu className="h-4 w-4" />}
          labelKey="serverMonitorCpu"
          value={formatPercent(cpu?.usagePercent)}
          percent={cpu?.usagePercent}
          unavailable={!cpu}
          detail={
            cpu ? (
              <BiText
                translationKey="serverMonitorSamplingWindow"
                values={{ seconds: metrics?.samplingWindowSeconds.toFixed(1) ?? "1.0" }}
              />
            ) : null
          }
        />
        <MetricCard
          icon={<MemoryStick className="h-4 w-4" />}
          labelKey="serverMonitorMemory"
          value={formatPercent(memory?.usedPercent)}
          percent={memory?.usedPercent}
          unavailable={!memory}
          detail={
            memory
              ? `${formatBytes(memory.usedBytes)} / ${formatBytes(memory.totalBytes)}`
              : null
          }
        />
        <MetricCard
          icon={<HardDrive className="h-4 w-4" />}
          labelKey="serverMonitorDisk"
          value={formatPercent(busiestDisk?.usedPercent)}
          percent={busiestDisk?.usedPercent}
          unavailable={!busiestDisk}
          detail={
            busiestDisk
              ? `${busiestDisk.mountPoint} · ${formatBytes(busiestDisk.usedBytes)} / ${formatBytes(busiestDisk.totalBytes)}`
              : null
          }
        />
        <MetricCard
          icon={<Network className="h-4 w-4" />}
          labelKey="serverMonitorNetwork"
          value={<NetworkInterfaceDisplay name={selectedNetwork ? selectedNetwork.interface : null} />}
          unavailable={!selectedNetwork}
          detail={
            selectedNetwork
              ? `${t("serverMonitorRx")}: ${formatRate(selectedNetwork.rxBytesPerSec)} · ${t("serverMonitorTx")}: ${formatRate(selectedNetwork.txBytesPerSec)}`
              : null
          }
        />
      </div>

      {disks.length > 0 ? (
        <div className="min-w-0 max-w-full rounded-2xl border border-slate-200/80 bg-white p-4 shadow-sm dark:border-slate-800 dark:bg-slate-900">
          <div className="flex items-center justify-between pb-3 border-b border-slate-100 dark:border-slate-800">
            <div className="flex items-center gap-2">
              <HardDrive className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
              <span className="text-xs font-semibold text-slate-900 dark:text-white">
                <BiText translationKey="serverMonitorAllDisksTitle" />
              </span>
              <span className="rounded-full bg-slate-100 px-2 py-0.5 text-[10px] font-mono text-slate-500 dark:bg-slate-800 dark:text-slate-400">
                {disks.length}
              </span>
            </div>
          </div>
          <div className="mt-3 divide-y divide-slate-100 dark:divide-slate-800/80">
            {disks.map((disk) => {
              const tone = getProgressTone(disk.usedPercent);
              const clampedPercent = Math.min(100, Math.max(0, disk.usedPercent));
              return (
                <div
                  key={disk.mountPoint}
                  data-testid={`disk-row-${disk.mountPoint}`}
                  className="py-2.5 first:pt-0 last:pb-0 flex flex-col gap-1.5 sm:flex-row sm:items-center sm:justify-between text-xs"
                >
                  <div className="min-w-0 sm:w-1/3 flex items-baseline gap-2">
                    <span className="font-mono font-semibold text-slate-900 dark:text-white truncate" title={disk.mountPoint}>
                      {disk.mountPoint}
                    </span>
                    <span className="rounded bg-slate-100 px-1.5 py-0.5 text-[10px] font-mono text-slate-500 dark:bg-slate-800 dark:text-slate-400 shrink-0">
                      {disk.filesystem}
                    </span>
                  </div>

                  <div className="min-w-0 sm:w-1/3 text-[11px] text-slate-500 dark:text-slate-400">
                    <span>
                      {formatBytes(disk.usedBytes)} / {formatBytes(disk.totalBytes)}
                    </span>
                    <span className="ml-1.5 opacity-70">
                      ({t("serverMonitorDiskFree")}: {formatBytes(disk.availableBytes)})
                    </span>
                  </div>

                  <div className="flex items-center gap-3 sm:w-1/3 sm:justify-end">
                    <div className="h-1.5 w-24 sm:w-28 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800 shrink-0">
                      <div
                        className={`h-full transition-all duration-300 ${tone.bar}`}
                        style={{ width: `${clampedPercent}%` }}
                      />
                    </div>
                    <span className={`w-12 text-right font-mono font-bold text-xs ${tone.text}`}>
                      {formatPercent(disk.usedPercent)}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}
