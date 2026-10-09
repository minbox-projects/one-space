import { TerminalSquare } from "lucide-react";
import { ClaudeIcon, OpenAIIcon, AntigravityIcon, OpenCodeIcon } from "./icons";

/**
 * CLI tool icon shared by the AI-environments surfaces and the quick session
 * bar. Extracted from `AiEnvironments/index.tsx` so consumers that only need an
 * icon (for example the quick-AI entry) do not pull the whole page module.
 */
export const ToolIcon = ({ tool, className }: { tool: string; className?: string }) => {
  switch (tool.toLowerCase()) {
    case "claude":
      return <ClaudeIcon className={className} />;
    case "codex":
      return <OpenAIIcon className={className} />;
    case "antigravity":
      return <AntigravityIcon className={className} />;
    case "opencode":
      return <OpenCodeIcon className={className} />;
    default:
      return <TerminalSquare className={className} />;
  }
};
