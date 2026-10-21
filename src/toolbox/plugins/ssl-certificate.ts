import { ShieldCheck } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const sslCertificateTool: ToolboxToolDescriptor = {
  id: "ssl-certificate",
  icon: ShieldCheck,
  iconClassName: "bg-emerald-500/10 text-emerald-600",
  labelKey: "sslCertificate",
  descriptionKey: "launcherSslCertificateDesc",
  labelText: {
    zh: "SSL 证书",
    en: "SSL Certificates",
  },
  descriptionText: {
    zh: "免费申请域名证书，TLS握手健康检测、证书链分析与到期监控",
    en: "Free SSL certificate issuance, TLS health check, chain inspection, and expiration alerts",
  },
  aliases: [{ target: "ssl-certificate" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 15,
  loadComponent: () =>
    import("@/components/SslCertificateTool").then((m) => ({
      default: m.SslCertificateTool,
    })),
};
