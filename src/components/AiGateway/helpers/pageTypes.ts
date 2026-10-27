import type { GatewayUpstreamProviderWithKeys } from "@/lib/aiGateway";

export type AiGatewayTab =
  | "providers"
  | "model-pools"
  | "models"
  | "keys"
  | "terminals"
  | "usage";

export type UsageSubTab = "stats" | "logs" | "history";

export function emptyProvider(): GatewayUpstreamProviderWithKeys {
  return {
    id: "",
    name: "",
    base_url: "",
    keys: [],
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
  };
}
