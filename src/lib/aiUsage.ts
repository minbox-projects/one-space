import { invoke } from '@tauri-apps/api/core';

export type AiModelId = 'claude' | 'antigravity' | 'codex' | 'opencode';
export type AiUsageWindowDays = 7 | 15 | 30;

export interface AiUsageSummary {
  total_tokens: number;
  calls: number;
  sessions: number;
  cache_hit_rate: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
}

export interface AiUsageDaily extends AiUsageSummary {
  date: string;
}

export interface AiUsageToolStats {
  tool: AiModelId;
  source_status: string;
  summary: AiUsageSummary;
  daily: AiUsageDaily[];
  peak_day?: {
    date: string;
    total_tokens: number;
    calls: number;
  } | null;
  scanned_sessions: number;
  scanned_calls: number;
  errors: string[];
}

export interface AiUsageModelStats extends AiUsageSummary {
  model: string;
}

export interface AiUsageDayBreakdown {
  tool: AiModelId;
  total_tokens: number;
  calls: number;
  cache_hit_rate: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
  models: AiUsageModelStats[];
}

export interface AiUsageDayStats {
  date: string;
  total_tokens: number;
  calls: number;
  sessions: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
  breakdown: AiUsageDayBreakdown[];
}

export interface AntigravityQuotaBucket {
  id: string;
  name: string;
  window: string;
  remaining_fraction: number;
  reset_time: string;
  description: string | null;
}

export interface AntigravityQuotaGroup {
  name: string;
  description: string | null;
  buckets: AntigravityQuotaBucket[];
}

export interface AntigravityQuota {
  groups: AntigravityQuotaGroup[];
}

export function sessionsUsageToolStats(
  tool: AiModelId,
  days: AiUsageWindowDays,
): Promise<AiUsageToolStats> {
  return invoke<AiUsageToolStats>('sessions_usage_tool_stats', { tool, days });
}

export function sessionsUsageDayStats(date: string): Promise<AiUsageDayStats> {
  return invoke<AiUsageDayStats>('sessions_usage_day_stats', { date });
}

export function sessionsUsageClearCache(): Promise<void> {
  return invoke<void>('sessions_usage_clear_cache');
}

export function sessionsAntigravityQuota(
  forceRefresh = false,
): Promise<AntigravityQuota> {
  if (forceRefresh) {
    return invoke<AntigravityQuota>('sessions_antigravity_quota', {
      forceRefresh: true,
    });
  }
  return invoke<AntigravityQuota>('sessions_antigravity_quota');
}
