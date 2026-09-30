import type { ComponentType } from "react";
import type { LucideIcon } from "lucide-react";

export type ToolboxSurface =
  | "hub"
  | "launcher-quick"
  | "launcher-internal"
  | "sidebar"
  | "tray";

export type JttParserTabId = "jt808" | "jt809" | "jt1078" | "hex";

export type ToolboxNavAlias = {
  target: string;
  jttParserTab?: JttParserTabId;
  sshTunnelTab?: string;
};

export type ToolboxBilingualText = {
  zh: string;
  en: string;
};

export type ToolboxToolDescriptor = {
  id: string;
  icon: LucideIcon;
  iconClassName: string;
  labelKey: string;
  descriptionKey: string;
  labelText?: ToolboxBilingualText;
  descriptionText?: ToolboxBilingualText;
  aliases: readonly ToolboxNavAlias[];
  surfaces: readonly ToolboxSurface[];
  defaultVisible: boolean;
  defaultOrder: number;
  component: ComponentType<{ isVisible?: boolean }>;
};

export type ResolvedToolboxTarget = {
  toolId: string;
  tab: string;
  moreToolsSection?: string;
  jttParserTab?: JttParserTabId;
  sshTunnelTab?: string;
};
