import { FolderSync } from "lucide-react";
import type { ToolboxToolDescriptor } from "../types";

export const quickSftpTool: ToolboxToolDescriptor = {
  id: "quick-sftp",
  icon: FolderSync,
  iconClassName: "bg-purple-500/10 text-purple-600",
  labelKey: "quickSftp",
  descriptionKey: "quickSftpDesc",
  aliases: [{ target: "quick-sftp" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 17,
  loadComponent: () =>
    import("@/components/QuickSftpTool").then((m) => ({
      default: m.QuickSftpTool,
    })),
};
