import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";

export type CaptureState =
  | "in_progress"
  | "completed"
  | "rejected"
  | "upstream_error"
  | "request_transfer_error"
  | "response_transfer_error"
  | "client_disconnected"
  | "interrupted";

export interface AiRequestCaptureConfig {
  enabled: boolean;
  port: number;
  upstreamBaseUrl: string;
}

export interface AiRequestCaptureStatus {
  running: boolean;
  listenAddress: string;
  port: number;
  lastError: string | null;
}

export interface AiRequestCaptureValidationError {
  field: string;
  message: string;
}

export interface AiRequestCaptureConfigApplyResult {
  config: AiRequestCaptureConfig;
  status: AiRequestCaptureStatus;
  validationErrors: AiRequestCaptureValidationError[];
}

export interface AiRequestCaptureHeader {
  name: string;
  values: string[];
}

export interface CapturedBody {
  data: string;
  encoding: string | null;
  capturedBytes: number;
  totalBytes: number;
  truncated: boolean;
}

export interface AiRequestCaptureListItem {
  id: string;
  startedAt: number;
  completedAt: number | null;
  state: CaptureState;
  method: string;
  requestPathAndQuery: string;
  upstreamUrl: string;
  responseStatus: number | null;
  durationMs: number | null;
  provider: string | null;
  model: string | null;
  inputTokens: number | null;
  outputTokens: number | null;
  totalTokens: number | null;
}

export interface AiRequestCaptureListResult {
  items: AiRequestCaptureListItem[];
  total: number;
  page: number;
  pageSize: number;
}

export interface AiRequestCaptureDetail extends AiRequestCaptureListItem {
  httpVersion: string;
  requestHeaders: AiRequestCaptureHeader[];
  requestBody: CapturedBody;
  responseHeaders: AiRequestCaptureHeader[];
  responseBody: CapturedBody;
  error: string | null;
}

export interface CaptureListQuery {
  search?: string;
  method?: string;
  states?: CaptureState[];
  provider?: string;
  model?: string;
  page: number;
  pageSize: number;
}

export interface AiRequestCaptureExportInput {
  query: CaptureListQuery;
  outputPath: string;
}

export interface AiRequestCaptureCurlResult {
  command: string;
  complete: boolean;
  warning: string | null;
}

export interface AiRequestCaptureUpdateEvent {
  kind: string;
  id?: string | null;
}

export class AiRequestCaptureError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AiRequestCaptureError";
  }
}

function normalizeError(error: unknown) {
  if (error instanceof AiRequestCaptureError) return error;
  if (error instanceof Error) return new AiRequestCaptureError(error.message);
  if (typeof error === "string") return new AiRequestCaptureError(error);
  try {
    return new AiRequestCaptureError(JSON.stringify(error) || "AI request capture command failed");
  } catch {
    return new AiRequestCaptureError("AI request capture command failed");
  }
}

async function captureInvoke<T>(command: string, args?: Record<string, unknown>) {
  try {
    return args ? await invoke<T>(command, args) : await invoke<T>(command);
  } catch (error) {
    throw normalizeError(error);
  }
}

export function aiRequestCaptureGetConfig() {
  return captureInvoke<AiRequestCaptureConfig>("ai_request_capture_get_config");
}

export function aiRequestCaptureSaveConfig(config: AiRequestCaptureConfig) {
  return captureInvoke<AiRequestCaptureConfigApplyResult>("ai_request_capture_save_config", { config });
}

export function aiRequestCaptureStart() {
  return captureInvoke<AiRequestCaptureStatus>("ai_request_capture_start");
}

export function aiRequestCaptureStop() {
  return captureInvoke<AiRequestCaptureStatus>("ai_request_capture_stop");
}

export function aiRequestCaptureStatus() {
  return captureInvoke<AiRequestCaptureStatus>("ai_request_capture_status");
}

export function aiRequestCaptureList(query: CaptureListQuery) {
  return captureInvoke<AiRequestCaptureListResult>("ai_request_capture_list", { query });
}

export function aiRequestCaptureGet(id: string) {
  return captureInvoke<AiRequestCaptureDetail>("ai_request_capture_get", { id });
}

export function aiRequestCaptureClear() {
  return captureInvoke<{ cleared: number }>("ai_request_capture_clear");
}

export function aiRequestCaptureExportHar(input: AiRequestCaptureExportInput) {
  return captureInvoke<{ outputPath: string; exported: number }>("ai_request_capture_export_har", { input });
}

export function aiRequestCaptureGenerateCurl(id: string) {
  return captureInvoke<AiRequestCaptureCurlResult>("ai_request_capture_generate_curl", { id });
}

export function subscribeAiRequestCaptureUpdates(
  handler: (payload: AiRequestCaptureUpdateEvent) => void,
) {
  return listen<AiRequestCaptureUpdateEvent>("ai-request-capture-updated", (event: Event<AiRequestCaptureUpdateEvent>) => {
    handler(event.payload);
  });
}

export function subscribeAiRequestCaptureStatus(
  handler: (payload: AiRequestCaptureStatus) => void,
) {
  return listen<AiRequestCaptureStatus>("ai-request-capture-status-update", (event: Event<AiRequestCaptureStatus>) => {
    handler(event.payload);
  });
}
