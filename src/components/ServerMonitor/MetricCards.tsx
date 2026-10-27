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

function MetricCard({
  icon,
  labelKey,
  value,
  detail,
  unavailable,
}: {
  icon: ReactNode;
  labelKey: string;
  value: string;
  detail?: ReactNode;
  unavailable?: boolean;
}) {
  return (
    <div className="rounded-xl border border-border/70 bg-card p-4 shadow-sm">
      <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        {icon}
        <span>
          <BiText translationKey={labelKey} />
        </span>
      </div>
      <div
        className={`mt-2 text-2xl font-bold tracking-tight ${
          unavailable ? "text-muted-foreground" : "text-foreground"
        }`}
      >
        {value}
      </div>
      {detail ? (
        <p className="mt-1 text-xs text-muted-foreground break-words">{detail}</p>
      ) : null}
    </div>
  );
}

export interface MetricCardsProps {
  metrics: MetricSnapshot | null;
  networks: NetworkMetric[];
  selectedInterface: string | null;
  selectedNetwork: NetworkMetric | null;
  onSelectInterface: (value: string) => void;
}

export function MetricCards({
  metrics,
  networks,
  selectedInterface,
  selectedNetwork,
  onSelectInterface,
}: MetricCardsProps) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage || i18n.language || "zh";
  const alternate = language.startsWith("zh") ? "en" : "zh";

  const cpu = metrics?.cpu ?? null;
  const memory = metrics?.memory ?? null;
  const disks = Array.isArray(metrics?.disks) ? metrics.disks : [];
  const busiestDisk = disks.reduce<(typeof disks)[number] | null>(
    (current, disk) =>
      !current || disk.usedPercent > current.usedPercent ? disk : current,
    null,
  );

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <MetricCard
          icon={<Cpu className="h-3.5 w-3.5" />}
          labelKey="serverMonitorCpu"
          value={formatPercent(cpu?.usagePercent)}
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
          icon={<MemoryStick className="h-3.5 w-3.5" />}
          labelKey="serverMonitorMemory"
          value={formatPercent(memory?.usedPercent)}
          unavailable={!memory}
          detail={
            memory
              ? `${formatBytes(memory.usedBytes)} / ${formatBytes(memory.totalBytes)}`
              : null
          }
        />
        <MetricCard
          icon={<HardDrive className="h-3.5 w-3.5" />}
          labelKey="serverMonitorDisk"
          value={formatPercent(busiestDisk?.usedPercent)}
          unavailable={!busiestDisk}
          detail={
            busiestDisk
              ? `${busiestDisk.mountPoint} · ${formatBytes(busiestDisk.usedBytes)} / ${formatBytes(busiestDisk.totalBytes)}`
              : null
          }
        />
        <MetricCard
          icon={<Network className="h-3.5 w-3.5" />}
          labelKey="serverMonitorNetwork"
          value={selectedNetwork ? selectedNetwork.interface : "—"}
          unavailable={!selectedNetwork}
          detail={
            selectedNetwork
              ? `${t("serverMonitorRx")}: ${formatRate(selectedNetwork.rxBytesPerSec)} · ${t("serverMonitorTx")}: ${formatRate(selectedNetwork.txBytesPerSec)}`
              : null
          }
        />
      </div>

      {networks.length > 0 ? (
        <label className="flex flex-col gap-1.5 text-xs font-medium text-muted-foreground sm:w-64">
          <span>
            <BiText translationKey="serverMonitorNetworkInterface" />
          </span>
          <select
            aria-label={`${t("serverMonitorNetworkInterface")} ${i18n.getFixedT(alternate)("serverMonitorNetworkInterface")}`}
            value={selectedInterface ?? selectedNetwork?.interface ?? networks[0].interface}
            onChange={(event) => onSelectInterface(event.target.value)}
            className="h-[38px] w-full rounded-lg border border-border bg-background px-3 text-sm font-medium text-foreground transition-all hover:border-primary/60 focus:border-primary focus:outline-none focus:ring-2 focus:ring-primary/50"
          >
            {networks.map((item) => (
              <option
                key={item.interface}
                value={item.interface}
                label={item.interface}
                aria-label={item.interface}
              />
            ))}
          </select>
        </label>
      ) : null}
    </div>
  );
}
