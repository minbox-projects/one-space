import { KeyRound } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const randomPasswordTool: ToolboxToolDescriptor = {
  id: "random-password",
  icon: KeyRound,
  iconClassName: "bg-emerald-500/10 text-emerald-600",
  labelKey: "randomPassword",
  descriptionKey: "randomPasswordToolDesc",
  aliases: [{ target: "random-password" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 5,
  loadComponent: () =>
    import("@/components/RandomPasswordTool").then((m) => ({
      default: m.RandomPasswordTool,
    })),
};
