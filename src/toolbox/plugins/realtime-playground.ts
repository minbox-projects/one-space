import { Activity } from "lucide-react";
import type { ToolboxToolDescriptor } from "../types";

export const realtimePlaygroundTool: ToolboxToolDescriptor = {
  id: "realtime-playground",
  icon: Activity,
  iconClassName: "bg-sky-500/10 text-sky-600",
  labelKey: "realtimePlayground",
  descriptionKey: "realtimePlaygroundDesc",
  labelText: {
    zh: "WebSocket & SSE",
    en: "WebSocket & SSE",
  },
  descriptionText: {
    zh: "大模型流式事件监听 (SSE)、WebSocket 双向交互、心跳保活与时间轴排查。",
    en: "Inspect SSE streams (TTFT), interactive WebSocket sessions, heartbeats & timeline flow.",
  },
  aliases: [{ target: "realtime-playground" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 23,
  loadComponent: () =>
    import("@/components/RealtimePlayground").then((module) => ({
      default: module.RealtimePlaygroundTool,
    })),
};
