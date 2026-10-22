import { Activity } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const uptimeMonitorTool: ToolboxToolDescriptor = {
  id: "uptime-monitor",
  icon: Activity,
  iconClassName: "bg-emerald-500/10 text-emerald-600",
  labelKey: "uptimeMonitor",
  descriptionKey: "launcherUptimeMonitorDesc",
  labelText: {
    zh: "服务状态监控",
    en: "Uptime Monitor",
  },
  descriptionText: {
    zh: "本地与远程服务心跳探活、TCP端口检测、延迟趋势分析与可用性告警",
    en: "Service heartbeat probing, TCP port check, latency trends, and uptime alerts",
  },
  aliases: [{ target: "uptime-monitor" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 19,
  loadComponent: () =>
    import("@/components/UptimeMonitor").then((m) => ({
      default: m.UptimeMonitorTool,
    })),
};
