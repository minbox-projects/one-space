import { Server } from "lucide-react";

import { SshServers } from "@/components/SshServers";
import type { ToolboxToolDescriptor } from "../types";

export const sshServersTool: ToolboxToolDescriptor = {
  id: "ssh",
  icon: Server,
  iconClassName: "bg-blue-500/10 text-blue-600",
  labelKey: "sshServers",
  descriptionKey: "launcherSshServersDesc",
  aliases: [{ target: "ssh" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 2,
  component: SshServers,
};
