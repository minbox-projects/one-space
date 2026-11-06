import { invoke } from "@tauri-apps/api/core";

export interface UrlHopDetail {
  hop_index: number;
  url: string;
  status_code: number;
  status_text: string;
  latency_ms: number;
  location?: string | null;
  resolved_next_url?: string | null;
  set_cookies: string[];
  headers: Array<[string, string]>;
  is_final: boolean;
}

export interface UrlTraceReport {
  initial_url: string;
  final_url: string;
  total_hops: number;
  total_duration_ms: number;
  has_loop: boolean;
  hops: UrlHopDetail[];
  error?: string | null;
}

export async function traceRedirectHops(
  initialUrl: string,
  userAgent?: string,
  customHeaders?: Array<[string, string]>,
  maxHops = 12,
): Promise<UrlTraceReport> {
  return invoke<UrlTraceReport>("url_trace_redirects", {
    initialUrl,
    userAgent,
    customHeaders,
    maxHops,
  });
}
