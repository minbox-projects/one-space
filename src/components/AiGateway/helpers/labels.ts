import type { GatewayConfig } from "@/lib/aiGateway";

// 映射标签：本地与上游相同时只展示一处，否则展示「本地 → 上游」。
export function formatMappingLabel(
  localModel: string,
  upstreamModel: string,
): string {
  return localModel && localModel !== upstreamModel
    ? `${localModel} → ${upstreamModel}`
    : upstreamModel;
}

export function findProviderName(
  config: GatewayConfig | null,
  providerId: string,
): string {
  return config?.providers.find((provider) => provider.id === providerId)?.name ??
    providerId;
}

// 删除/恢复映射时的展示标签：能唯一定位本地名时展示「本地 → 上游」，
// 同一上游对应多行时仅展示上游名以免误导。调用时基于删除前配置计算。
export function findMappingLabel(
  config: GatewayConfig | null,
  providerId: string,
  upstreamModel: string,
): string {
  const provider = config?.providers.find((item) => item.id === providerId);
  const locals = Array.from(
    new Set(
      (provider?.mappings ?? [])
        .filter((mapping) => mapping.upstream_model === upstreamModel)
        .map((mapping) => mapping.local_model.trim())
        .filter(Boolean),
    ),
  );
  if (locals.length === 1) return formatMappingLabel(locals[0], upstreamModel);
  return upstreamModel;
}
