import { Hash } from "lucide-react";

import type { ToolboxToolDescriptor } from "../types";

export const md5EncryptionTool: ToolboxToolDescriptor = {
  id: "md5-encryption",
  icon: Hash,
  iconClassName: "bg-teal-500/10 text-teal-600",
  labelKey: "md5Encryption.title",
  descriptionKey: "md5Encryption.description",
  aliases: [{ target: "md5-encryption" }],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 7,
  loadComponent: () =>
    import("@/components/Md5EncryptionTool").then((m) => ({
      default: m.Md5EncryptionTool,
    })),
};
