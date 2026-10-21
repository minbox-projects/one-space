import { Send } from "lucide-react";
import type { ToolboxToolDescriptor } from "../types";

export const curlPlaygroundTool: ToolboxToolDescriptor = {
  id: "curl-playground",
  icon: Send,
  iconClassName: "bg-blue-500/10 text-blue-600",
  labelKey: "curlPlayground",
  descriptionKey: "curlPlaygroundDesc",
  aliases: [{ target: "curl-playground" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 16,
  loadComponent: () =>
    import("@/components/CurlPlaygroundTool").then((m) => ({
      default: m.CurlPlaygroundTool,
    })),
};
