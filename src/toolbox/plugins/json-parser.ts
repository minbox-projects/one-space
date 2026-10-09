import { Braces } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const jsonParserTool: ToolboxToolDescriptor = {
  id: "json-parser",
  icon: Braces,
  iconClassName: "bg-sky-500/10 text-sky-600",
  labelKey: "jsonParser",
  descriptionKey: "jsonParserToolDesc",
  aliases: [{ target: "json-parser" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 6,
  loadComponent: () =>
    import("@/components/JsonParserTool").then((m) => ({
      default: m.JsonParserTool,
    })),
};
