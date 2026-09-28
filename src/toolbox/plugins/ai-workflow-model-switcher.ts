import { SlidersHorizontal } from "lucide-react";
import type { ComponentType } from "react";

import { AiWorkflowModelSwitcher } from "@/components/AiWorkflowModelSwitcher";
import type { ToolboxToolDescriptor } from "../types";

export const aiWorkflowModelSwitcherTool: ToolboxToolDescriptor = {
  id: "ai-workflow-model-switcher",
  icon: SlidersHorizontal,
  iconClassName: "bg-purple-500/10 text-purple-600",
  labelKey: "aiWorkflowModelSwitcher",
  descriptionKey: "aiWorkflowModelSwitcherDesc",
  aliases: [{ target: "ai-workflow-model-switcher" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 11,
  component: AiWorkflowModelSwitcher as unknown as ComponentType<{
    isVisible?: boolean;
  }>,
};
