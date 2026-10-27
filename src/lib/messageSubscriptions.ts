import type { MessageRecord, MessageSeverity, MessageSource } from "./messages";

export interface FocusedMessageTypeDef {
  id: string; // 格式: `${source}:${category}`
  source: MessageSource;
  category: string;
  defaultSeverity: MessageSeverity;
  nameKey: string;
  descKey: string;
  isRecommended: boolean;
}

export interface FocusedPreferences {
  highlightToast: boolean;
  extendedToastDuration: boolean;
}

export interface FocusedMessageSubscriptionConfig {
  subscribedTypeIds: string[];
  manualStarredIds: string[];
  preferences: FocusedPreferences;
}

/** 系统已支持的消息类型目录全集 */
export const FOCUSED_MESSAGE_TYPE_CATALOG: FocusedMessageTypeDef[] = [
  // --- AI 网关 ---
  {
    id: "ai_gateway:key_auth_failed",
    source: "ai_gateway",
    category: "key_auth_failed",
    defaultSeverity: "error",
    nameKey: "msgType_ai_gateway_key_auth_failed_title",
    descKey: "msgType_ai_gateway_key_auth_failed_desc",
    isRecommended: true,
  },
  {
    id: "ai_gateway:mapping_auto_disabled",
    source: "ai_gateway",
    category: "mapping_auto_disabled",
    defaultSeverity: "warning",
    nameKey: "msgType_ai_gateway_mapping_auto_disabled_title",
    descKey: "msgType_ai_gateway_mapping_auto_disabled_desc",
    isRecommended: true,
  },
  {
    id: "ai_gateway:upstream_timeout",
    source: "ai_gateway",
    category: "upstream_timeout",
    defaultSeverity: "warning",
    nameKey: "msgType_ai_gateway_upstream_timeout_title",
    descKey: "msgType_ai_gateway_upstream_timeout_desc",
    isRecommended: false,
  },
  {
    id: "ai_gateway:template_sync",
    source: "ai_gateway",
    category: "template_sync",
    defaultSeverity: "info",
    nameKey: "msgType_ai_gateway_template_sync_title",
    descKey: "msgType_ai_gateway_template_sync_desc",
    isRecommended: false,
  },

  // --- 服务可用性监控 ---
  {
    id: "uptime_monitor:status_down",
    source: "uptime_monitor",
    category: "status_down",
    defaultSeverity: "error",
    nameKey: "msgType_uptime_monitor_status_down_title",
    descKey: "msgType_uptime_monitor_status_down_desc",
    isRecommended: true,
  },
  {
    id: "uptime_monitor:status_recovered",
    source: "uptime_monitor",
    category: "status_recovered",
    defaultSeverity: "success",
    nameKey: "msgType_uptime_monitor_status_recovered_title",
    descKey: "msgType_uptime_monitor_status_recovered_desc",
    isRecommended: false,
  },
  {
    id: "uptime_monitor:ssl_certificate",
    source: "uptime_monitor",
    category: "ssl_certificate",
    defaultSeverity: "warning",
    nameKey: "msgType_uptime_monitor_ssl_certificate_title",
    descKey: "msgType_uptime_monitor_ssl_certificate_desc",
    isRecommended: true,
  },

  // --- 服务器监控 ---
  {
    id: "server_monitor:collect_failed",
    source: "server_monitor",
    category: "collect_failed",
    defaultSeverity: "error",
    nameKey: "msgType_server_monitor_collect_failed_title",
    descKey: "msgType_server_monitor_collect_failed_desc",
    isRecommended: true,
  },

  // --- SSL 证书 ---
  {
    id: "ssl_certificate:expiring_soon",
    source: "ssl_certificate",
    category: "expiring_soon",
    defaultSeverity: "warning",
    nameKey: "msgType_ssl_certificate_expiring_soon_title",
    descKey: "msgType_ssl_certificate_expiring_soon_desc",
    isRecommended: true,
  },

  // --- SSH 隧道 ---
  {
    id: "ssh_tunnels:disconnect",
    source: "ssh_tunnels",
    category: "disconnect",
    defaultSeverity: "error",
    nameKey: "msgType_ssh_tunnels_disconnect_title",
    descKey: "msgType_ssh_tunnels_disconnect_desc",
    isRecommended: true,
  },
  {
    id: "ssh_tunnels:supervisor",
    source: "ssh_tunnels",
    category: "supervisor",
    defaultSeverity: "warning",
    nameKey: "msgType_ssh_tunnels_supervisor_title",
    descKey: "msgType_ssh_tunnels_supervisor_desc",
    isRecommended: false,
  },
  {
    id: "ssh_tunnels:connect",
    source: "ssh_tunnels",
    category: "connect",
    defaultSeverity: "info",
    nameKey: "msgType_ssh_tunnels_connect_title",
    descKey: "msgType_ssh_tunnels_connect_desc",
    isRecommended: false,
  },

  // --- 数据同步 ---
  {
    id: "sync:sync_conflict",
    source: "sync",
    category: "sync_conflict",
    defaultSeverity: "warning",
    nameKey: "msgType_sync_sync_conflict_title",
    descKey: "msgType_sync_sync_conflict_desc",
    isRecommended: true,
  },
  {
    id: "sync:sync_failed",
    source: "sync",
    category: "sync_failed",
    defaultSeverity: "error",
    nameKey: "msgType_sync_sync_failed_title",
    descKey: "msgType_sync_sync_failed_desc",
    isRecommended: true,
  },
  {
    id: "sync:sync_completed",
    source: "sync",
    category: "sync_completed",
    defaultSeverity: "info",
    nameKey: "msgType_sync_sync_completed_title",
    descKey: "msgType_sync_sync_completed_desc",
    isRecommended: false,
  },

  // --- 备份管理 ---
  {
    id: "backup:backup_failed",
    source: "backup",
    category: "backup_failed",
    defaultSeverity: "error",
    nameKey: "msgType_backup_backup_failed_title",
    descKey: "msgType_backup_backup_failed_desc",
    isRecommended: true,
  },
  {
    id: "backup:auto_backup",
    source: "backup",
    category: "auto_backup",
    defaultSeverity: "info",
    nameKey: "msgType_backup_auto_backup_title",
    descKey: "msgType_backup_auto_backup_desc",
    isRecommended: false,
  },

  // --- MCP 服务器生态 ---
  {
    id: "mcp_servers:runtime",
    source: "mcp_servers",
    category: "runtime",
    defaultSeverity: "error",
    nameKey: "msgType_mcp_servers_runtime_title",
    descKey: "msgType_mcp_servers_runtime_desc",
    isRecommended: true,
  },
  {
    id: "mcp_servers:connect",
    source: "mcp_servers",
    category: "connect",
    defaultSeverity: "info",
    nameKey: "msgType_mcp_servers_connect_title",
    descKey: "msgType_mcp_servers_connect_desc",
    isRecommended: false,
  },

  // --- 系统与环境底层 ---
  {
    id: "system:storage",
    source: "system",
    category: "storage",
    defaultSeverity: "warning",
    nameKey: "msgType_system_storage_title",
    descKey: "msgType_system_storage_desc",
    isRecommended: true,
  },
  {
    id: "system:recovery",
    source: "system",
    category: "recovery",
    defaultSeverity: "info",
    nameKey: "msgType_system_recovery_title",
    descKey: "msgType_system_recovery_desc",
    isRecommended: false,
  },

  // --- 客户端更新 ---
  {
    id: "updater:available",
    source: "updater",
    category: "available",
    defaultSeverity: "info",
    nameKey: "msgType_updater_available_title",
    descKey: "msgType_updater_available_desc",
    isRecommended: false,
  },
  {
    id: "updater:failed",
    source: "updater",
    category: "failed",
    defaultSeverity: "warning",
    nameKey: "msgType_updater_failed_title",
    descKey: "msgType_updater_failed_desc",
    isRecommended: false,
  },
];

export const FOCUSED_SUBSCRIPTIONS_STORAGE_KEY = "onespace_focused_messages_config_v1";

/** 默认推荐订阅配置 */
export function getDefaultFocusedSubscriptionConfig(): FocusedMessageSubscriptionConfig {
  const recommendedIds = FOCUSED_MESSAGE_TYPE_CATALOG
    .filter((item) => item.isRecommended)
    .map((item) => item.id);

  return {
    subscribedTypeIds: recommendedIds,
    manualStarredIds: [],
    preferences: {
      highlightToast: true,
      extendedToastDuration: true,
    },
  };
}

/** 保存配置到本地存储 */
export function saveFocusedSubscriptionConfig(
  config: FocusedMessageSubscriptionConfig,
): void {
  if (typeof window === "undefined" || !window.localStorage) return;
  try {
    localStorage.setItem(
      FOCUSED_SUBSCRIPTIONS_STORAGE_KEY,
      JSON.stringify(config),
    );
  } catch (e) {
    console.error("Failed to save focused messages subscription config", e);
  }
}

/** 加载本地配置，若无则回退到推荐预设 */
export function loadFocusedSubscriptionConfig(): FocusedMessageSubscriptionConfig {
  if (typeof window === "undefined" || !window.localStorage) {
    return getDefaultFocusedSubscriptionConfig();
  }
  try {
    const raw = localStorage.getItem(FOCUSED_SUBSCRIPTIONS_STORAGE_KEY);
    if (!raw) {
      const defaults = getDefaultFocusedSubscriptionConfig();
      saveFocusedSubscriptionConfig(defaults);
      return defaults;
    }
    const parsed = JSON.parse(raw);
    return {
      subscribedTypeIds: Array.isArray(parsed.subscribedTypeIds)
        ? parsed.subscribedTypeIds
        : getDefaultFocusedSubscriptionConfig().subscribedTypeIds,
      manualStarredIds: Array.isArray(parsed.manualStarredIds)
        ? parsed.manualStarredIds
        : [],
      preferences: {
        highlightToast: parsed.preferences?.highlightToast ?? true,
        extendedToastDuration: parsed.preferences?.extendedToastDuration ?? true,
      },
    };
  } catch {
    return getDefaultFocusedSubscriptionConfig();
  }
}

/** 判定单条消息是否属于特别关注 */
export function isMessageFocused(
  message: Pick<MessageRecord, "id" | "source" | "category">,
  config: FocusedMessageSubscriptionConfig,
): boolean {
  // 1. 用户手动星标记录
  if (config.manualStarredIds && config.manualStarredIds.includes(message.id)) {
    return true;
  }
  // 2. 命中已订阅的消息类型
  const typeKey = `${message.source}:${message.category}`;
  return config.subscribedTypeIds.includes(typeKey);
}
