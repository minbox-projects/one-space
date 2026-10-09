import { Waypoints } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const sshTunnelsTool: ToolboxToolDescriptor = {
  id: "ssh-tunnels",
  icon: Waypoints,
  iconClassName: "bg-cyan-500/10 text-cyan-600",
  labelKey: "sshTunnels",
  descriptionKey: "launcherSshTunnelsDesc",
  aliases: [
    { target: "ssh-tunnels" },
    { target: "ssh-tunnels:connected", sshTunnelTab: "__connected__" },
  ],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 3,
  loadComponent: () =>
    import("@/components/SshTunnels").then((m) => ({ default: m.SshTunnels })),
};
