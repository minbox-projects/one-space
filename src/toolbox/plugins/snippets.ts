import { Code2 } from "lucide-react";

import { Snippets } from "@/components/Snippets";
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
  component: Snippets,
};
