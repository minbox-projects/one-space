import { Globe } from "lucide-react";
import type { ToolboxToolDescriptor } from "../types";

export const dnsInspectorTool: ToolboxToolDescriptor = {
  id: "dns-inspector",
  icon: Globe,
  iconClassName: "bg-indigo-500/10 text-indigo-600",
  labelKey: "dnsInspector",
  descriptionKey: "dnsInspectorDesc",
  labelText: {
    zh: "DNS 解析与网络探测",
    en: "DNS & Network Inspector",
  },
  descriptionText: {
    zh: "多公共 DNS 实时比对、全记录解析、IP 地理与 ASN 情报及 TCP 握手时延。",
    en: "Multi-provider DNS comparison, all record types, IP intelligence and TCP ping latency.",
  },
  aliases: [{ target: "dns-inspector" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 24,
  loadComponent: () =>
    import("@/components/DnsInspector").then((module) => ({
      default: module.DnsInspectorTool,
    })),
};
