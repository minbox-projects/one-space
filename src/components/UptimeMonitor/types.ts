export type ProbeType = "http" | "tcp" | "ping" | "dns";

export type MonitorStatus = "up" | "down" | "degraded" | "paused" | "unknown";

export interface HttpProbeOptions {
  method: string;
  headers: Record<string, string>;
  body?: string;
  expectedStatus: number[];
  keyword?: string;
  keywordAbsent?: string;
  ignoreSslErrors: boolean;
  warnSslDaysRemaining?: number;
}

export interface TcpProbeOptions {
  host: string;
  port: number;
}

export interface DnsProbeOptions {
  recordType: string;
  expectedValue?: string;
}

export interface AlertConfig {
  systemNotification: boolean;
  soundAlert: boolean;
  webhookUrl?: string;
}

export interface MonitorTarget {
  id: string;
  name: string;
  probeType: ProbeType;
  target: string;
  intervalSec: number;
  timeoutMs: number;
  retryCount: number;
  recoverCount: number;
  isEnabled: boolean;
  tags: string[];
  httpOptions?: HttpProbeOptions;
  tcpOptions?: TcpProbeOptions;
  dnsOptions?: DnsProbeOptions;
  alertConfig: AlertConfig;
  createdAt: number;
  updatedAt: number;
}

export interface HeartbeatBarPoint {
  timestamp: number;
  status: MonitorStatus;
  responseTimeMs: number;
  statusCode?: number;
  errorMessage?: string;
}

export interface MonitorIncident {
  id: number;
  monitorId: string;
  startedAt: number;
  resolvedAt?: number;
  durationSec?: number;
  cause: string;
}

export interface MonitorSummary {
  target: MonitorTarget;
  currentStatus: MonitorStatus;
  lastCheckedAt?: number;
  lastResponseTimeMs?: number;
  uptimeRatio24h: number;
  uptimeRatio7d: number;
  avgResponseTime24hMs: number;
  recentHeartbeats: HeartbeatBarPoint[];
  activeIncident?: MonitorIncident | null;
  sslDaysRemaining?: number | null;
}

export interface ProbeTestResult {
  success: boolean;
  status: MonitorStatus;
  responseTimeMs: number;
  statusCode?: number;
  message: string;
  sslDaysRemaining?: number | null;
}
