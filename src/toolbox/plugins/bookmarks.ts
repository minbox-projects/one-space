import { Star } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const bookmarksTool: ToolboxToolDescriptor = {
  id: "bookmarks",
  icon: Star,
  iconClassName: "bg-amber-500/10 text-amber-600",
  labelKey: "tray.bookmarks",
  descriptionKey: "Save the links and resources you revisit often.",
  descriptionText: {
    en: "Save the links and resources you revisit often",
    zh: "沉淀常用链接和资源入口",
  },
  aliases: [{ target: "bookmarks" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 0,
  loadComponent: () =>
    import("@/components/Bookmarks").then((m) => ({ default: m.Bookmarks })),
};
