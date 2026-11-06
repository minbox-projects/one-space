import { Link2 } from "lucide-react";
import type { ToolboxToolDescriptor } from "../types";

export const urlInspectorTool: ToolboxToolDescriptor = {
  id: "url-inspector",
  icon: Link2,
  iconClassName: "bg-violet-500/10 text-violet-600",
  labelKey: "urlInspector",
  descriptionKey: "urlInspectorDesc",
  labelText: {
    zh: "URL 嵌套解构器",
    en: "URL Inspector",
  },
  descriptionText: {
    zh: "超长 URL 剖析、嵌套回调自动解包、参数表格双向同步与代码导出。",
    en: "Inspect deep nested URLs, recursive callback unwrapping, query grid sync & code export.",
  },
  aliases: [{ target: "url-inspector" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 22,
  loadComponent: () =>
    import("@/components/UrlInspector").then((module) => ({
      default: module.UrlInspectorTool,
    })),
};
