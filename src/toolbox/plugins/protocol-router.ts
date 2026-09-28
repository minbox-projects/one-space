import { Route } from "lucide-react";

import { ProtocolRouterTool } from "@/components/ProtocolRouterTool";
import type { ToolboxToolDescriptor } from "../types";

export const protocolRouterTool: ToolboxToolDescriptor = {
  id: "protocol-router",
  icon: Route,
  iconClassName: "bg-orange-500/10 text-orange-600",
  labelKey: "protocolRouter",
  descriptionKey: "launcherProtocolRouterDesc",
  aliases: [{ target: "protocol-router" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 4,
  component: ProtocolRouterTool,
};
