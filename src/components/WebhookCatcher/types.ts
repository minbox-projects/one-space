export interface CustomResponseConfig {
  statusCode: number;
  contentType: string;
  body: string;
  delayMs: number;
}

export interface ForwardConfig {
  enabled: boolean;
  targetUrl: string;
}

export interface WebhookEndpoint {
  id: string;
  name: string;
  path: string;
  isActive: boolean;
  customResponse: CustomResponseConfig;
  forwardConfig?: ForwardConfig;
  createdAt: number;
}

export interface DetectedSignature {
  provider: string;
  headerName: string;
  headerValue: string;
}

export interface CapturedWebhookRequest {
  id: string;
  endpointId: string;
  endpointName: string;
  timestamp: number;
  method: string;
  path: string;
  queryParams: Record<string, string>;
  headers: Record<string, string>;
  clientIp: string;
  body: string;
  bodyFormat: string; // "json" | "form" | "xml" | "raw"
  isTruncated: boolean;
  detectedSignature?: DetectedSignature;
}

export interface ReplayRequest {
  requestId: string;
  targetUrl: string;
  overrideHeaders?: Record<string, string>;
  overrideBody?: string;
}

export interface ReplayResult {
  statusCode: number;
  durationMs: number;
  headers: Record<string, string>;
  responseBody: string;
  executedAt: number;
}

export interface WebhookServerStatus {
  isRunning: boolean;
  port: number;
  activeEndpointsCount: number;
  capturedRequestsCount: number;
}
