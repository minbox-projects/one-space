import { invoke } from "@tauri-apps/api/core";

export interface DnsRecordAnswer {
  name: string;
  type: number;
  type_name: string;
  TTL: number;
  data: string;
}

export interface DnsServerCompareItem {
  server_id: string;
  server_name: string;
  endpoint: string;
  latency_ms: number;
  status: number;
  status_text: string;
  answers: DnsRecordAnswer[];
  error?: string | null;
}

export interface DnsCompareReport {
  domain: string;
  record_type: string;
  queried_at: number;
  is_consistent: boolean;
  servers: DnsServerCompareItem[];
}

export interface TcpPingResult {
  host: string;
  port: number;
  success: boolean;
  latency_ms?: number | null;
  error?: string | null;
}

export interface TcpPingPacket {
  session_id: string;
  seq: number;
  host: string;
  port: number;
  success: boolean;
  latency_ms?: number | null;
  error?: string | null;
  timestamp: number;
}

export interface TcpPingStats {
  sent: number;
  received: number;
  lost: number;
  lossRate: number; // 0 ~ 100
  min: number;
  avg: number;
  max: number;
  last: number;
  jitter: number; // 抖动 (相邻包延迟波动平均)
}

export interface IpIntelligenceResult {
  ip: string;
  is_private: boolean;
  is_loopback: boolean;
  classification: string;
}

export async function dnsQuery(
  domain: string,
  recordType: string,
): Promise<DnsCompareReport> {
  return invoke<DnsCompareReport>("dns_inspector_query", {
    domain,
    recordType,
  });
}

export async function dnsTcpPing(
  host: string,
  port = 80,
  timeoutMs = 3000,
): Promise<TcpPingResult> {
  return invoke<TcpPingResult>("dns_inspector_tcp_ping", {
    host,
    port,
    timeoutMs,
  });
}

export async function startTcpPingStream(
  sessionId: string,
  host: string,
  port = 80,
  intervalMs = 1000,
  maxCount?: number,
  timeoutMs = 3000,
): Promise<boolean> {
  return invoke<boolean>("dns_inspector_tcp_ping_stream_start", {
    sessionId,
    host,
    port,
    intervalMs,
    maxCount,
    timeoutMs,
  });
}

export async function stopTcpPingStream(sessionId: string): Promise<boolean> {
  return invoke<boolean>("dns_inspector_tcp_ping_stream_stop", {
    sessionId,
  });
}

export function calculateTcpPingStats(packets: TcpPingPacket[]): TcpPingStats {
  if (packets.length === 0) {
    return {
      sent: 0,
      received: 0,
      lost: 0,
      lossRate: 0,
      min: 0,
      avg: 0,
      max: 0,
      last: 0,
      jitter: 0,
    };
  }

  const sent = packets.length;
  const successes = packets.filter((p) => p.success && typeof p.latency_ms === "number");
  const received = successes.length;
  const lost = sent - received;
  const lossRate = (lost / sent) * 100;

  if (received === 0) {
    return {
      sent,
      received: 0,
      lost,
      lossRate: 100,
      min: 0,
      avg: 0,
      max: 0,
      last: 0,
      jitter: 0,
    };
  }

  const latencies = successes.map((p) => p.latency_ms as number);
  const min = Math.min(...latencies);
  const max = Math.max(...latencies);
  const sum = latencies.reduce((acc, v) => acc + v, 0);
  const avg = Math.round((sum / received) * 10) / 10;
  const last = latencies[latencies.length - 1];

  let jitterSum = 0;
  let jitterCount = 0;
  for (let i = 1; i < latencies.length; i++) {
    jitterSum += Math.abs(latencies[i] - latencies[i - 1]);
    jitterCount++;
  }
  const jitter = jitterCount > 0 ? Math.round((jitterSum / jitterCount) * 10) / 10 : 0;

  return {
    sent,
    received,
    lost,
    lossRate: Math.round(lossRate * 10) / 10,
    min,
    avg,
    max,
    last,
    jitter,
  };
}

export async function dnsIpLookup(ip: string): Promise<IpIntelligenceResult> {
  return invoke<IpIntelligenceResult>("dns_inspector_ip_lookup", { ip });
}
