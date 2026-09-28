import { Binary } from "lucide-react";
import type { ComponentType } from "react";

import { JttDataParserTool } from "@/components/JttDataParserTool";
import type { ToolboxToolDescriptor } from "../types";

export const jttDataParserTool: ToolboxToolDescriptor = {
  id: "jtt-data-parser",
  icon: Binary,
  iconClassName: "bg-emerald-500/10 text-emerald-600",
  labelKey: "JT/T Data Parser",
  descriptionKey: "Parse JT/T 808, 809, 1078 packets and convert hex locally.",
  labelText: {
    en: "JT/T Data Parser",
    zh: "JT/T 数据解析",
  },
  descriptionText: {
    en: "Parse JT/T 808, 809, 1078 packets and convert hex locally.",
    zh: "本地解析 JT/T 808、809、1078 报文并转换十六进制。",
  },
  aliases: [
    { target: "jtt-data-parser" },
    { target: "808", jttParserTab: "jt808" },
    { target: "809", jttParserTab: "jt809" },
    { target: "1078", jttParserTab: "jt1078" },
    { target: "hex", jttParserTab: "hex" },
  ],
  surfaces: ["hub", "launcher-quick"],
  defaultVisible: true,
  defaultOrder: 10,
  component: JttDataParserTool as unknown as ComponentType<{
    isVisible?: boolean;
  }>,
};
