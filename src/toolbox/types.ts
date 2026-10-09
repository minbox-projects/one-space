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

/**
 * Navigation props shared by every toolbox tool rendered inside the hub.
 *
 * The shape is intentionally a superset with every field optional: tool
 * components accept it and may ignore fields they do not use. `initialTab`
 * carries a `JttParserTabId` for the JT/T parser and an opaque group id for SSH
 * tunnels, so it accepts the JT/T literals plus any other string.
 */
export type ToolboxNavigationProps = {
  isVisible?: boolean;
  initialTab?: JttParserTabId | string;
  navigationNonce?: number;
};

/**
 * Deferred import of a selected tool's implementation. Keeping a loader (or a
 * `component` reference) out of the descriptor keeps tool-exclusive modules out
 * of the startup dependency closure; callers resolve it on demand. The resolved
 * module exposes the renderable component as its default export, matching the
 * `React.lazy` module shape.
 */
export type ToolboxToolComponentLoader = () => Promise<{
  default: ComponentType<ToolboxNavigationProps>;
}>;

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
  loadComponent: ToolboxToolComponentLoader;
};

export type ResolvedToolboxTarget = {
  toolId: string;
  tab: string;
  moreToolsSection?: string;
  jttParserTab?: JttParserTabId;
  sshTunnelTab?: string;
};
