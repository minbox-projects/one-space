import { Link } from "lucide-react";

import { ShortLinkTool } from "@/components/ShortLinkTool";
import type { ToolboxToolDescriptor } from "../types";

export const shortLinkTool: ToolboxToolDescriptor = {
  id: "short-link",
  icon: Link,
  iconClassName: "bg-teal-500/10 text-teal-600",
  labelKey: "shortLink",
  descriptionKey: "shortLinkLauncherDesc",
  aliases: [{ target: "short-link" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 8,
  component: ShortLinkTool,
};
