import { Share2 } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const fileSharingTool: ToolboxToolDescriptor = {
  id: "file-sharing",
  icon: Share2,
  iconClassName: "bg-rose-500/10 text-rose-600",
  labelKey: "fileSharing",
  descriptionKey: "fileSharingLauncherDesc",
  aliases: [{ target: "file-sharing" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 9,
  loadComponent: () =>
    import("@/components/FileSharingTool").then((m) => ({
      default: m.FileSharingTool,
    })),
};
