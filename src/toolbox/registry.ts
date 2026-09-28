import i18n from "@/i18n";

import { aiWorkflowModelSwitcherTool } from "./plugins/ai-workflow-model-switcher";
import { bookmarksTool } from "./plugins/bookmarks";
import { cloudTool } from "./plugins/cloud";
import { fileSharingTool } from "./plugins/file-sharing";
import { jsonParserTool } from "./plugins/json-parser";
import { jttDataParserTool } from "./plugins/jtt-data-parser";
import { md5EncryptionTool } from "./plugins/md5-encryption";
import { notesTool } from "./plugins/notes";
import { protocolRouterTool } from "./plugins/protocol-router";
import { randomPasswordTool } from "./plugins/random-password";
import { shortLinkTool } from "./plugins/short-link";
import { snippetsTool } from "./plugins/snippets";
import { sshServersTool } from "./plugins/ssh-servers";
import { sshTunnelsTool } from "./plugins/ssh-tunnels";
import type {
  ResolvedToolboxTarget,
  ToolboxBilingualText,
  ToolboxSurface,
  ToolboxToolDescriptor,
} from "./types";

export const TOOLBOX_TOOLS: readonly ToolboxToolDescriptor[] = [
  bookmarksTool,
  cloudTool,
  sshServersTool,
  sshTunnelsTool,
  protocolRouterTool,
  randomPasswordTool,
  jsonParserTool,
  md5EncryptionTool,
  shortLinkTool,
  fileSharingTool,
  jttDataParserTool,
  aiWorkflowModelSwitcherTool,
  snippetsTool,
  notesTool,
];

export function listToolboxTools(surface: ToolboxSurface): ToolboxToolDescriptor[] {
  return TOOLBOX_TOOLS.filter((tool) => tool.surfaces.includes(surface));
}

export function getToolboxTool(id: string): ToolboxToolDescriptor | undefined {
  return TOOLBOX_TOOLS.find((tool) => tool.id === id);
}

export type ToolboxTextTranslate = (key: string) => string;

/**
 * Resolves descriptor copy for the active UI language. Descriptors whose i18n
 * keys do not exist supply explicit bilingual text, which takes precedence over
 * the `t(key)` lookup used by every fully-translated descriptor.
 */
export function resolveToolboxText(
  text: ToolboxBilingualText | undefined,
  key: string,
  translate: ToolboxTextTranslate,
  language: string = i18n.language,
): string {
  if (text) {
    return language.startsWith("zh") ? text.zh : text.en;
  }
  return translate(key);
}

export function resolveToolboxNavigationAlias(
  target: string,
): ResolvedToolboxTarget | undefined {
  for (const tool of TOOLBOX_TOOLS) {
    for (const alias of tool.aliases) {
      if (alias.target !== target) continue;
      const isHubTool = tool.surfaces.includes("hub");
      return {
        toolId: tool.id,
        tab: isHubTool ? "more-tools" : tool.id,
        ...(isHubTool ? { moreToolsSection: tool.id } : {}),
        ...(alias.jttParserTab ? { jttParserTab: alias.jttParserTab } : {}),
      };
    }
  }
  return undefined;
}

const KNOWN_SURFACES: ReadonlySet<string> = new Set([
  "hub",
  "launcher-quick",
  "launcher-internal",
  "sidebar",
  "tray",
]);

const KEBAB_CASE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

export function assertToolboxRegistryIntegrity(
  tools: readonly ToolboxToolDescriptor[] = TOOLBOX_TOOLS,
): void {
  const seenIds = new Set<string>();
  for (const tool of tools) {
    if (seenIds.has(tool.id)) {
      throw new Error(`Duplicate toolbox tool id: ${tool.id}`);
    }
    seenIds.add(tool.id);

    for (const surface of tool.surfaces) {
      if (!KNOWN_SURFACES.has(surface)) {
        throw new Error(`Unknown toolbox surface: ${surface}`);
      }
    }

    if (tool.surfaces.includes("hub") && !KEBAB_CASE.test(tool.id)) {
      throw new Error(`Hub toolbox id is not stable kebab-case: ${tool.id}`);
    }

    for (const alias of tool.aliases) {
      if (!alias || typeof alias.target !== "string" || alias.target.length === 0) {
        throw new Error(`Toolbox alias is missing a target for ${tool.id}`);
      }
    }
  }
}
