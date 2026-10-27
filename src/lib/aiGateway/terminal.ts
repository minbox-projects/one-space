export const AI_GATEWAY_RELEASE_PORT = 17688;

export const AI_GATEWAY_DEV_PORT = 17689;

export const AI_GATEWAY_DEFAULT_PORT = import.meta.env.DEV
  ? AI_GATEWAY_DEV_PORT
  : AI_GATEWAY_RELEASE_PORT;


/** Terminal tools AI Gateway is allowed to configure; Antigravity is excluded. */
export const AI_GATEWAY_SUPPORTED_TERMINAL_TOOLS = [
  "opencode",
  "codex",
  "claude",
] as const;


/** Build the local OpenAI-compatible base address (with `/v1` suffix) the listener binds to. */
export function localBaseUrl(port: number): string {
  return `http://127.0.0.1:${port}/v1`;
}


/** Build the local Anthropic Messages-compatible API endpoint address. */
export function localAnthropicMessagesUrl(port: number): string {
  return `http://127.0.0.1:${port}/v1/messages`;
}
