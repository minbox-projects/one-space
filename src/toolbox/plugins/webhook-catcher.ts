import { Webhook } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const webhookCatcherTool: ToolboxToolDescriptor = {
  id: "webhook-catcher",
  icon: Webhook,
  iconClassName: "bg-indigo-500/10 text-indigo-600",
  labelKey: "webhookCatcher",
  descriptionKey: "launcherWebhookCatcherDesc",
  labelText: {
    zh: "Webhook 捕获与重放",
    en: "Webhook Catcher & Replay",
  },
  descriptionText: {
    zh: "本地私密接收第三方回调，Payload与签名深度检视，一键重放到本地端口",
    en: "Private local webhook receiver, signature inspector, and instant replay to local dev ports",
  },
  aliases: [{ target: "webhook-catcher" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 20,
  loadComponent: () =>
    import("@/components/WebhookCatcher").then((m) => ({
      default: m.WebhookCatcherTool,
    })),
};
