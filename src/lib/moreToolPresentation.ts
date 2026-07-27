import {
  Braces,
  Cloud,
  KeyRound,
  Route,
  ScanSearch,
  Server,
  Star,
  Waypoints,
  type LucideIcon,
} from "lucide-react";
import type { MoreToolsSection } from "./navigation";

export type PresentedMoreToolId = Exclude<
  MoreToolsSection,
  "backup" | "notes" | "snippets"
>;

type MoreToolPresentation = {
  icon: LucideIcon;
  iconClassName: string;
};

const MORE_TOOL_PRESENTATION: Record<PresentedMoreToolId, MoreToolPresentation> = {
  bookmarks: { icon: Star, iconClassName: "bg-amber-500/10 text-amber-600" },
  cloud: { icon: Cloud, iconClassName: "bg-sky-500/10 text-sky-600" },
  ssh: { icon: Server, iconClassName: "bg-blue-500/10 text-blue-600" },
  "ssh-tunnels": { icon: Waypoints, iconClassName: "bg-cyan-500/10 text-cyan-600" },
  "protocol-router": { icon: Route, iconClassName: "bg-orange-500/10 text-orange-600" },
  "random-password": { icon: KeyRound, iconClassName: "bg-emerald-500/10 text-emerald-600" },
  "json-parser": { icon: Braces, iconClassName: "bg-sky-500/10 text-sky-600" },
  "ai-request-capture": { icon: ScanSearch, iconClassName: "bg-cyan-500/10 text-cyan-600" },
};

export function getMoreToolPresentation(toolId: PresentedMoreToolId) {
  return MORE_TOOL_PRESENTATION[toolId];
}
