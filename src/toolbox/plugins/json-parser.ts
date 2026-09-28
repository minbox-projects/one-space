import { Braces } from "lucide-react";

import { JsonParserTool } from "@/components/JsonParserTool";
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
  component: JsonParserTool,
};
