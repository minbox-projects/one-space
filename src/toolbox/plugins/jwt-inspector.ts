import { ShieldCheck } from "lucide-react";
import type { ToolboxToolDescriptor } from "../types";

export const jwtInspectorTool: ToolboxToolDescriptor = {
  id: "jwt-inspector",
  icon: ShieldCheck,
  iconClassName: "bg-emerald-500/10 text-emerald-600",
  labelKey: "jwtInspector",
  descriptionKey: "jwtInspectorDesc",
  aliases: [{ target: "jwt-inspector" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 15,
  loadComponent: () =>
    import("@/components/JwtInspectorTool").then((m) => ({
      default: m.JwtInspectorTool,
    })),
};
