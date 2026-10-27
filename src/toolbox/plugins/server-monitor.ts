import { ServerCog } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const serverMonitorTool: ToolboxToolDescriptor = {
  id: "server-monitor",
  icon: ServerCog,
  iconClassName: "bg-sky-500/10 text-sky-600",
  labelKey: "serverMonitor",
  descriptionKey: "serverMonitorDesc",
  labelText: {
    zh: "服务器监控",
    en: "Server Monitor",
  },
  descriptionText: {
    zh: "通过 SSH 采集 Linux 主机的 CPU、内存、磁盘与网络使用情况。",
    en: "Collect CPU, memory, disk, and network usage from Linux hosts over SSH.",
  },
  aliases: [{ target: "server-monitor" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 21,
  loadComponent: () =>
    import("@/components/ServerMonitor").then((module) => ({
      default: module.ServerMonitorTool,
    })),
};
