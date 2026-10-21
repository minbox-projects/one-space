import { Globe } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const portTunnelTool: ToolboxToolDescriptor = {
  id: "port-tunnel",
  icon: Globe,
  iconClassName: "bg-emerald-500/10 text-emerald-600",
  labelKey: "portTunnel",
  descriptionKey: "launcherPortTunnelDesc",
  aliases: [{ target: "port-tunnel" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 14,
  loadComponent: () =>
    import("@/components/PortTunnelTool").then((m) => ({
      default: m.PortTunnelTool,
    })),
};
