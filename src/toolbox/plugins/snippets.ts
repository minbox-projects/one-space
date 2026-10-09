import { Code2 } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const snippetsTool: ToolboxToolDescriptor = {
  id: "snippets",
  icon: Code2,
  iconClassName: "bg-indigo-500/10 text-indigo-600",
  labelKey: "snippets",
  descriptionKey: "snippets",
  aliases: [{ target: "snippets" }],
  surfaces: ["sidebar", "tray", "launcher-internal"],
  defaultVisible: true,
  defaultOrder: 12,
  loadComponent: () =>
    import("@/components/Snippets").then((m) => ({ default: m.Snippets })),
};
