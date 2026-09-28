import { Cloud } from "lucide-react";

import { CloudDrive } from "@/components/CloudDrive";
import type { ToolboxToolDescriptor } from "../types";

export const cloudTool: ToolboxToolDescriptor = {
  id: "cloud",
  icon: Cloud,
  iconClassName: "bg-sky-500/10 text-sky-600",
  labelKey: "cloudDrive",
  descriptionKey: "Browse and organize synced cloud files.",
  descriptionText: {
    en: "Browse and organize synced cloud files",
    zh: "查看和整理云端文件内容",
  },
  aliases: [{ target: "cloud" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 1,
  component: CloudDrive,
};
