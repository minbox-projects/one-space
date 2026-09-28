import type { LucideIcon } from "lucide-react";

import { getToolboxTool } from "@/toolbox/registry";
import type { MoreToolsSection } from "./navigation";

export type PresentedMoreToolId = Exclude<
  MoreToolsSection,
  "backup" | "notes" | "snippets"
>;

type MoreToolPresentation = {
  icon: LucideIcon;
  iconClassName: string;
};

export function getMoreToolPresentation(toolId: PresentedMoreToolId): MoreToolPresentation {
  const descriptor = getToolboxTool(toolId);
  if (!descriptor) {
    throw new Error(`Unknown toolbox tool: ${toolId}`);
  }
  return { icon: descriptor.icon, iconClassName: descriptor.iconClassName };
}
