import { Bluetooth } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const bleLockTool: ToolboxToolDescriptor = {
  id: "ble-lock",
  icon: Bluetooth,
  iconClassName: "bg-sky-500/10 text-sky-600",
  labelKey: "bleLock",
  descriptionKey: "bleLockLauncherDesc",
  aliases: [{ target: "ble-lock" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 1,
  loadComponent: () =>
    import("@/components/BleLockTool").then((m) => ({
      default: m.BleLockTool,
    })),
};
