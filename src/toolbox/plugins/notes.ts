import { NotebookPen } from "lucide-react";

import { Notes } from "@/components/Notes";
import type { ToolboxToolDescriptor } from "../types";

export const notesTool: ToolboxToolDescriptor = {
  id: "notes",
  icon: NotebookPen,
  iconClassName: "bg-amber-500/10 text-amber-600",
  labelKey: "notes",
  descriptionKey: "notes",
  aliases: [{ target: "notes" }],
  surfaces: ["sidebar", "tray", "launcher-internal"],
  defaultVisible: true,
  defaultOrder: 13,
  component: Notes,
};
