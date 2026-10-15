#!/usr/bin/env node
// Temporary DeepSeek cache diagnostic tool
// (plan 20261015-deepseek-cache-diagnostics, Step 2 / REQ-003, REQ-004).
//
// `observe` binds to loopback and transparently forwards request and response
// bodies to an explicitly selected loopback gateway. It observes only the
// selected Chat conversation for the requested model, retains exactly one
// preceding completed sample, and emits one JSONL `observation` event per
// completed selected request. Every other request is an unobserved
// pass-through. `compare` runs a bounded, sequential two-path measurement
// against explicitly configured endpoints and emits one metadata-only report.
//
// Output is metadata-only: comparison classification, one difference path or a
// static availability reason, numeric usage/source-field presence and the
// upstream HTTP status. Bodies, message/tool values, header values, keys,
// session identities, URLs, error text and hashes are never printed, and no
// request/response sample is persisted.
//
// It uses only Node built-in modules, creates no permanent configuration, reads
// no encrypted or client configuration and prints no environment values. Its
// only network operations are to the explicitly configured endpoints: the
// `observe` loopback gateway target given on the command line and the six
// explicitly configured `compare` gateway/upstream endpoints.

import http from "node:http";
import https from "node:https";

const HOP_BY_HOP_HEADERS = new Set([
  "connection",
  "proxy-connection",
  "keep-alive",
  "transfer-encoding",
  "upgrade",
  "te",
  "trailer",
  "proxy-authorization",
]);

// Only native Chat Completions routes are observed; everything else (and every
// non-POST request) is an unobserved pass-through.
const CHAT_PATHS = new Set(["/v1/chat/completions", "/chat/completions"]);

// Highest precedence first; mirrors the gateway's recognized session headers.
const SESSION_ID_HEADERS = [
  "x-session-affinity",
  "x-opencode-session",
  "session-id",
  "session_id",
  "conversation_id",
  "thread-id",
  "x-session-id",
];

// Bounded capture: bytes beyond these limits are still forwarded unchanged,
// they simply stop feeding the diagnostic parse. SSE responses are parsed
// incrementally per complete event, so this capture limit no longer bounds
// where usage may appear; a single unterminated event is bounded separately.
const REQUEST_CAPTURE_LIMIT = 8 * 1024 * 1024;
const RESPONSE_CAPTURE_LIMIT = 8 * 1024 * 1024;
const SSE_EVENT_LIMIT = 1024 * 1024;

// Isolation metadata above this serialized size is reported as
// `oversized-metadata` instead of being compared.
const MAX_ISOLATION_METADATA_BYTES = 64 * 1024;

function jsonLine(value) {
  return `${JSON.stringify(value)}\n`;
}

// Fixed metadata error: static text only, never echoing argument or
// environment values.
function failInvalidArguments(message) {
  process.stderr.write(
    jsonLine({ event: "error", reason: "invalid-arguments", message }),
  );
  process.exit(2);
}

function isLoopbackHost(host) {
  if (!host) return false;
  const normalized = host.replace(/^\[/, "").replace(/\]$/, "").toLowerCase();
  if (normalized === "localhost" || normalized === "::1") return true;
  return /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(normalized);
}

function parseObserveArgs(argv) {
  const opts = { listen: null, gateway: null, model: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const take = () => {
      i += 1;
      if (i >= argv.length) {
        failInvalidArguments("observe option requires a value");
      }
      return argv[i];
    };
    switch (arg) {
      case "--listen": {
        const port = Number(take());
        if (!Number.isInteger(port) || port < 0 || port > 65535) {
          failInvalidArguments("--listen must be an integer port between 0 and 65535");
        }
        opts.listen = port;
        break;
      }
      case "--gateway": {
        const raw = take();
        let url;
        try {
          url = new URL(raw);
        } catch {
          failInvalidArguments("--gateway must be an absolute URL");
        }
        if (url.protocol !== "http:" && url.protocol !== "https:") {
          failInvalidArguments("--gateway must use http or https");
        }
        if (!isLoopbackHost(url.hostname)) {
          failInvalidArguments("--gateway must target a loopback host");
        }
        opts.gateway = url;
        break;
      }
      case "--model": {
        const raw = take();
        if (raw.trim() === "") {
          failInvalidArguments("--model must be a non-empty model id");
        }
        opts.model = raw;
        break;
      }
      default:
        failInvalidArguments("unknown observe argument");
    }
  }
  if (opts.listen === null) failInvalidArguments("observe requires --listen");
  if (opts.gateway === null) failInvalidArguments("observe requires --gateway");
  if (opts.model === null) failInvalidArguments("observe requires --model");
  return opts;
}

function forwardableRequestHeaders(headers) {
  const out = {};
  for (const [name, value] of Object.entries(headers)) {
    const lower = name.toLowerCase();
    if (lower === "host" || HOP_BY_HOP_HEADERS.has(lower)) continue;
    out[name] = value;
  }
  return out;
}

function forwardableResponseHeaders(headers) {
  const out = {};
  for (const [name, value] of Object.entries(headers)) {
    if (HOP_BY_HOP_HEADERS.has(name.toLowerCase())) continue;
    out[name] = value;
  }
  return out;
}

function resolveSessionId(headers) {
  for (const name of SESSION_ID_HEADERS) {
    const value = headers[name];
    if (typeof value !== "string") continue;
    const trimmed = value.trim();
    if (trimmed !== "") return trimmed;
  }
  return null;
}

function isChatRoute(req) {
  if (req.method !== "POST") return false;
  const path = (req.url || "").split("?")[0];
  return CHAT_PATHS.has(path);
}

// ---------------------------------------------------------------------------
// Usage normalization (native DeepSeek hit/miss fields + nested OpenAI details)
// ---------------------------------------------------------------------------

function strictNonNegativeInteger(value) {
  if (typeof value === "number") {
    return Number.isInteger(value) && value >= 0 ? value : null;
  }
  if (typeof value === "string") {
    const text = value.trim();
    if (!/^[0-9]+$/.test(text)) return null;
    const parsed = Number(text);
    return Number.isSafeInteger(parsed) ? parsed : null;
  }
  return null;
}

// `absent` = key missing or null; `invalid` = present but not a strict count;
// `present` = a non-negative integer or its supported numeric-string form.
function readNativeCount(object, key) {
  if (!Object.prototype.hasOwnProperty.call(object, key)) {
    return { state: "absent" };
  }
  const value = object[key];
  if (value === null) return { state: "absent" };
  const parsed = strictNonNegativeInteger(value);
  return parsed === null ? { state: "invalid" } : { state: "present", value: parsed };
}

function readNestedCount(object, outer, key) {
  const details = object[outer];
  if (details === null || typeof details !== "object" || Array.isArray(details)) {
    return { state: "absent" };
  }
  return readNativeCount(details, key);
}

function pickPresentField(object, preferred, fallback) {
  if (
    Object.prototype.hasOwnProperty.call(object, preferred) &&
    object[preferred] !== null
  ) {
    return preferred;
  }
  if (
    Object.prototype.hasOwnProperty.call(object, fallback) &&
    object[fallback] !== null
  ) {
    return fallback;
  }
  return null;
}

function normalizeUsage(usage) {
  if (usage === undefined || usage === null) {
    return { availability: "unavailable", reason: "missing-usage", fields: { native: false, nested: false } };
  }
  if (typeof usage !== "object" || Array.isArray(usage)) {
    return { availability: "unavailable", reason: "malformed-usage", fields: { native: false, nested: false } };
  }

  const nativeHit = readNativeCount(usage, "prompt_cache_hit_tokens");
  const nativeMiss = readNativeCount(usage, "prompt_cache_miss_tokens");
  const nestedPrimary = readNestedCount(usage, "prompt_tokens_details", "cached_tokens");
  const nestedRead =
    nestedPrimary.state === "absent"
      ? readNestedCount(usage, "input_tokens_details", "cached_tokens")
      : nestedPrimary;

  const fields = {
    native: nativeHit.state !== "absent" || nativeMiss.state !== "absent",
    nested: nestedRead.state !== "absent",
  };
  const unavailable = (reason) => ({ availability: "unavailable", reason, fields });

  if (
    nativeHit.state === "invalid" ||
    nativeMiss.state === "invalid" ||
    nestedRead.state === "invalid"
  ) {
    return unavailable("malformed-usage");
  }

  const inclusiveField = pickPresentField(usage, "input_tokens", "prompt_tokens");
  if (inclusiveField === null) return unavailable("malformed-usage");
  const inclusive = strictNonNegativeInteger(usage[inclusiveField]);
  if (inclusive === null) return unavailable("malformed-usage");

  const nativeHitValue = nativeHit.state === "present" ? nativeHit.value : null;
  const nestedValue = nestedRead.state === "present" ? nestedRead.value : null;

  let hit;
  if (nativeHitValue !== null && nestedValue !== null) {
    if (nativeHitValue !== nestedValue) return unavailable("native-nested-disagree");
    hit = nativeHitValue;
  } else if (nativeHitValue !== null) {
    hit = nativeHitValue;
  } else if (nestedValue !== null) {
    hit = nestedValue;
  } else {
    hit = 0;
  }

  if (hit > inclusive) return unavailable("malformed-usage");
  if (nativeMiss.state === "present" && nativeMiss.value !== inclusive - hit) {
    return unavailable("malformed-usage");
  }

  const outputField = pickPresentField(usage, "output_tokens", "completion_tokens");
  const output =
    outputField === null ? null : strictNonNegativeInteger(usage[outputField]);

  return {
    availability: "available",
    inclusive_input_tokens: inclusive,
    cache_hit_tokens: hit,
    cache_miss_tokens: inclusive - hit,
    output_tokens: output,
    cache_hit_rate: inclusive === 0 ? null : hit / inclusive,
    fields,
  };
}

function usageFromBufferedResponse(text) {
  let payload;
  try {
    payload = JSON.parse(text);
  } catch {
    return normalizeUsage(undefined);
  }
  if (payload === null || typeof payload !== "object" || Array.isArray(payload)) {
    return normalizeUsage(undefined);
  }
  return normalizeUsage(payload.usage);
}

function normalizeUsageUnavailable(reason) {
  return {
    availability: "unavailable",
    reason,
    fields: { native: false, nested: false },
  };
}

// Incremental SSE observation: complete events are processed as they arrive and
// dropped immediately, so a usage event is still read after any amount of
// preceding stream bytes while retained memory stays bounded. Forwarded bytes
// are never affected.
function processSseEvent(context, event) {
  const dataLines = [];
  for (const line of event.split("\n")) {
    if (line.startsWith("data:")) {
      dataLines.push(line.slice(5).replace(/^ /, ""));
    }
  }
  if (dataLines.length === 0) return;
  const data = dataLines.join("\n");
  if (data === "[DONE]") return;
  let payload;
  try {
    payload = JSON.parse(data);
  } catch {
    return;
  }
  if (
    payload !== null &&
    typeof payload === "object" &&
    !Array.isArray(payload) &&
    payload.usage !== undefined
  ) {
    context.streamUsage = payload.usage;
    context.streamUsageFound = true;
  }
}

function feedSse(context, chunk) {
  context.sseBuffer += chunk.toString("utf8");
  // Normalize CR and CRLF to LF on the accumulated buffer so an event boundary
  // split across chunks (for example `\r\n\r` then `\n`, or `\r` then `\n`)
  // still collapses to a real blank line. A trailing lone CR is deliberately
  // held back: converting it before its following `\n` arrives would turn the
  // single CRLF terminator into a false `\n\n` boundary once the next chunk
  // starts with `\n`. Forwarded bytes are never affected.
  context.sseBuffer = context.sseBuffer.replace(/\r\n|\r(?!$)/g, "\n");
  let boundary;
  while ((boundary = context.sseBuffer.indexOf("\n\n")) >= 0) {
    const event = context.sseBuffer.slice(0, boundary);
    context.sseBuffer = context.sseBuffer.slice(boundary + 2);
    processSseEvent(context, event);
  }
  if (context.sseBuffer.length > SSE_EVENT_LIMIT) context.sseBuffer = "";
}

function usageFromContext(context) {
  if (context.unsupportedResponseFormat) {
    return normalizeUsageUnavailable("unsupported-format");
  }
  if (context.responseIsSse) {
    return normalizeUsage(context.streamUsageFound ? context.streamUsage : undefined);
  }
  return usageFromBufferedResponse(
    Buffer.concat(context.responseCapture).toString("utf8"),
  );
}

// ---------------------------------------------------------------------------
// Structured comparison (values only, never their contents)
// ---------------------------------------------------------------------------

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

// First differing JSON path, or null when deeply equal.
function firstDiffPath(a, b, path) {
  if (Object.is(a, b)) return null;
  if (a === undefined || b === undefined) return path;
  if (Array.isArray(a) && Array.isArray(b)) {
    const shared = Math.min(a.length, b.length);
    for (let i = 0; i < shared; i += 1) {
      const sub = firstDiffPath(a[i], b[i], `${path}[${i}]`);
      if (sub) return sub;
    }
    return a.length === b.length ? null : `${path}[${shared}]`;
  }
  if (isPlainObject(a) && isPlainObject(b)) {
    const keys = [];
    for (const key of Object.keys(a)) keys.push(key);
    for (const key of Object.keys(b)) {
      if (!keys.includes(key)) keys.push(key);
    }
    for (const key of keys) {
      const sub = firstDiffPath(a[key], b[key], `${path}.${key}`);
      if (sub) return sub;
    }
    return null;
  }
  return path;
}

// Messages are compared index-by-index; a shared equal prefix with the current
// list at least as long is an append-only input.
function compareMessages(previous, current) {
  const before = Array.isArray(previous) ? previous : [];
  const after = Array.isArray(current) ? current : [];
  const shared = Math.min(before.length, after.length);
  for (let i = 0; i < shared; i += 1) {
    const sub = firstDiffPath(before[i], after[i], `$.messages[${i}]`);
    if (sub) return { comparison: "changed", firstChangedPath: sub };
  }
  if (after.length >= before.length) return { comparison: "append-only" };
  return { comparison: "changed", firstChangedPath: `$.messages[${shared}]` };
}

function compareBodies(previous, current) {
  const messages = compareMessages(previous.messages, current.messages);
  if (messages.comparison === "changed") return messages;

  const toolsPath = firstDiffPath(previous.tools, current.tools, "$.tools");
  if (toolsPath) return { comparison: "changed", firstChangedPath: toolsPath };

  const metadataPath = firstDiffPath(previous.metadata, current.metadata, "$.metadata");
  if (metadataPath) return { comparison: "changed", firstChangedPath: metadataPath };

  const userIdPath = firstDiffPath(previous.user_id, current.user_id, "$.user_id");
  if (userIdPath) return { comparison: "changed", firstChangedPath: userIdPath };

  return { comparison: "append-only" };
}

function isOversizedIsolationMetadata(body) {
  for (const value of [body.metadata, body.user_id]) {
    if (value === undefined) continue;
    let serialized;
    try {
      serialized = JSON.stringify(value);
    } catch {
      return true;
    }
    if (typeof serialized === "string" && serialized.length > MAX_ISOLATION_METADATA_BYTES) {
      return true;
    }
  }
  return false;
}

// ---------------------------------------------------------------------------
// Transparent loopback reverse proxy + observation
//
// Context lifetime: transport-level concurrency, deferred comparison + release
// ---------------------------------------------------------------------------

// Potential overlap is tracked from the moment a same-session chat request
// arrives (its headers / first body byte), not only when its body finishes. A
// request that starts while an earlier selected response is still held is
// therefore known to be concurrent before its own body ends. The actual
// comparison verdict is deferred until the peers' selected/unselected state is
// known; forwarding, piping and backpressure are never delayed.
function registerInflight(context, state) {
  if (!context.chatRoute || context.session === null) return;
  let inflight = state.sessionInflight.get(context.session);
  if (!inflight) {
    inflight = new Set();
    state.sessionInflight.set(context.session, inflight);
  }
  for (const peer of inflight) {
    peer.peers.add(context);
    context.peers.add(peer);
  }
  inflight.add(context);
}

function unregisterInflight(context, state) {
  if (!context.chatRoute || context.session === null) return;
  const inflight = state.sessionInflight.get(context.session);
  if (!inflight) return;
  inflight.delete(context);
  if (inflight.size === 0) state.sessionInflight.delete(context.session);
}

// ORCH-001: the bounded request copy exists only to classify the request and
// the buffered response copy only to read usage. Both are freed as soon as that
// work is done so no per-route/session sample lingers until process exit.
function releaseRequestCapture(context) {
  context.requestCapture = [];
  context.requestCaptureBytes = 0;
}

function releaseResponseCapture(context) {
  context.responseCapture = [];
  context.responseCaptureBytes = 0;
  context.sseBuffer = "";
}

// A resolved context has a known selected/unselected state: its body ended and
// was classified, or it finished/aborted first. Resolving unblocks deferred
// peer comparisons that were waiting on it.
function resolveContext(context, state) {
  if (context.resolved) return;
  context.resolved = true;
  context.resolvedSelected = context.selected === true;
  flushDeferred(state);
}

function peersResolved(context) {
  for (const peer of context.peers) {
    if (!peer.resolved) return false;
  }
  return true;
}

// Emit deferred finalized observations strictly in completion order, so the
// diagnostic stream stays ordered once every peer's selectedness is known.
function flushDeferred(state) {
  while (state.deferred.length > 0) {
    const head = state.deferred[0];
    if (!peersResolved(head)) break;
    state.deferred.shift();
    emitObservation(head, state);
  }
}

function emitObservation(context, state) {
  // Overlap is symmetric: a selected request is comparison-unavailable when any
  // concurrent same-session peer was itself selected. Unrelated sessions, models
  // and routes never contribute a peer, so they cannot fake an overlap.
  let overlapped = false;
  for (const peer of context.peers) {
    if (peer.resolvedSelected) {
      overlapped = true;
      break;
    }
  }

  const event = { event: "observation" };
  if (overlapped) {
    event.comparison = "unavailable";
    event.reason = "overlap";
  } else if (!context.parseOk) {
    event.comparison = "unavailable";
    event.reason = context.parseReason;
  } else if (isOversizedIsolationMetadata(context.body)) {
    event.comparison = "unavailable";
    event.reason = "oversized-metadata";
  } else if (state.previous === null) {
    event.comparison = "unavailable";
    event.reason = "no-predecessor";
  } else {
    const result = compareBodies(state.previous, context.body);
    event.comparison = result.comparison;
    if (result.comparison === "changed") {
      event.first_changed_path = result.firstChangedPath;
    }
  }

  event.usage = context.pendingUsage;
  event.http_status = context.status || 502;

  // Only a non-overlapped, successfully parsed request becomes the retained
  // predecessor; an overlapped request drops any predecessor instead of
  // reusing or fabricating one.
  if (overlapped) {
    state.previous = null;
  } else if (context.parseOk) {
    state.previous = context.body;
  }

  process.stdout.write(jsonLine(event));

  // ORCH-001: after emission, only `state.previous` may retain a body sample.
  context.body = null;
  context.pendingUsage = null;
}

// One terminal path for a context: release the bounded copies, drop its
// transport-level registration, resolve it for waiting peers and queue a
// selected event for deferred, ordered emission. ORCH-001 bounds retained body
// samples to one `state.previous` plus the still-in-flight selected requests;
// aborted, unselected and overlapped contexts drop their body immediately.
function finishContext(context, state, aborted) {
  if (context.finished) return;
  context.finished = true;
  if (aborted) context.aborted = true;

  releaseRequestCapture(context);
  unregisterInflight(context, state);

  if (!context.aborted && context.selected) {
    context.pendingUsage = usageFromContext(context);
    releaseResponseCapture(context);
    state.deferred.push(context);
  } else {
    // ORCH-001: a non-selected or aborted context keeps no body sample.
    context.body = null;
    releaseResponseCapture(context);
  }

  resolveContext(context, state);
}

// The request body is forwarded byte-for-byte as it streams; a bounded copy is
// retained only to classify the request and never reaches the output.
function selectRequest(context, clientReq, opts, state) {
  if (!context.chatRoute || context.session === null) return;
  if (context.requestTruncated) return;

  const encoding = String(clientReq.headers["content-encoding"] || "").toLowerCase();
  if (encoding !== "" && encoding !== "identity") {
    context.parseReason = "unsupported-format";
  } else {
    let parsed;
    try {
      parsed = JSON.parse(Buffer.concat(context.requestCapture).toString("utf8"));
    } catch {
      parsed = undefined;
    }
    if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
      context.parseReason = "parse-failure";
    } else if (parsed.model !== opts.model) {
      return; // another model is never observed
    } else {
      context.parseOk = true;
      context.body = parsed;
    }
  }

  // ORCH-001: classification is complete; drop the bounded request copy now.
  releaseRequestCapture(context);

  if (context.parseOk) {
    if (state.selectedSession === null) state.selectedSession = context.session;
    if (context.session !== state.selectedSession) return;
  } else if (
    state.selectedSession === null ||
    context.session !== state.selectedSession
  ) {
    return;
  }

  context.selected = true;
}

function handleRequest(clientReq, clientRes, opts, state) {
  const context = {
    chatRoute: isChatRoute(clientReq),
    session: resolveSessionId(clientReq.headers),
    selected: false,
    aborted: false,
    finished: false,
    parseOk: false,
    parseReason: null,
    body: null,
    status: 0,
    responseContentType: "",
    responseIsSse: false,
    unsupportedResponseFormat: false,
    sseBuffer: "",
    streamUsage: undefined,
    streamUsageFound: false,
    requestCapture: [],
    requestCaptureBytes: 0,
    requestTruncated: false,
    responseCapture: [],
    responseCaptureBytes: 0,
    pendingUsage: null,
    peers: new Set(),
    resolved: false,
    resolvedSelected: false,
  };

  // Track concurrency from the transport start: this is what lets a later
  // same-session request be recognised as overlapping an already held response
  // before its own body ends. Diagnostics may resolve later; forwarding is
  // untouched.
  registerInflight(context, state);

  clientReq.on("data", (chunk) => {
    if (context.requestCaptureBytes >= REQUEST_CAPTURE_LIMIT) {
      context.requestTruncated = true;
      return;
    }
    const remaining = REQUEST_CAPTURE_LIMIT - context.requestCaptureBytes;
    const slice = chunk.length > remaining ? chunk.subarray(0, remaining) : chunk;
    context.requestCapture.push(slice);
    context.requestCaptureBytes += slice.length;
    if (slice.length < chunk.length) context.requestTruncated = true;
  });

  clientReq.on("end", () => selectRequest(context, clientReq, opts, state));

  const gatewayPort =
    opts.gateway.port || (opts.gateway.protocol === "https:" ? 443 : 80);

  // The transport module must match the configured protocol: the `http` module
  // throws `ERR_INVALID_PROTOCOL` synchronously for an `https:` URL. Both a
  // synchronous construction failure and an asynchronous TLS handshake failure
  // become the same bounded bad-gateway response instead of an uncaught crash.
  const transport = opts.gateway.protocol === "https:" ? https : http;
  let upstream;
  try {
    upstream = transport.request(
      {
        protocol: opts.gateway.protocol,
        hostname: opts.gateway.hostname.replace(/^\[/, "").replace(/\]$/, ""),
        port: gatewayPort,
        method: clientReq.method,
        path: clientReq.url || "/",
        headers: forwardableRequestHeaders(clientReq.headers),
      },
      (upstreamRes) => {
        context.status = upstreamRes.statusCode || 502;
        context.responseContentType = String(upstreamRes.headers["content-type"] || "");
        const contentEncoding = String(
          upstreamRes.headers["content-encoding"] || "",
        ).toLowerCase();
        context.unsupportedResponseFormat =
          contentEncoding !== "" && contentEncoding !== "identity";
        context.responseIsSse = context.responseContentType.includes("text/event-stream");
        upstreamRes.on("data", (chunk) => {
          if (context.unsupportedResponseFormat) return;
          if (context.responseIsSse) {
            feedSse(context, chunk);
            return;
          }
          if (context.responseCaptureBytes >= RESPONSE_CAPTURE_LIMIT) return;
          const remaining = RESPONSE_CAPTURE_LIMIT - context.responseCaptureBytes;
          const slice = chunk.length > remaining ? chunk.subarray(0, remaining) : chunk;
          context.responseCapture.push(slice);
          context.responseCaptureBytes += slice.length;
        });
        clientRes.writeHead(context.status, forwardableResponseHeaders(upstreamRes.headers));
        upstreamRes.pipe(clientRes);
      },
    );
  } catch {
    // No URL, credential or stack trace is emitted; the listener keeps serving.
    if (!clientRes.headersSent) {
      context.status = 502;
      clientRes.writeHead(502, { "content-type": "text/plain" });
    }
    clientRes.end();
    finishContext(context, state, true);
    return;
  }

  upstream.on("error", () => {
    context.aborted = true;
    if (!clientRes.headersSent) {
      context.status = 502;
      clientRes.writeHead(502, { "content-type": "text/plain" });
    }
    clientRes.end();
  });

  clientRes.on("finish", () => finishContext(context, state, false));

  clientReq.on("error", () => upstream.destroy());
  clientRes.on("close", () => {
    if (context.finished) return;
    finishContext(context, state, true);
    upstream.destroy();
  });

  clientReq.pipe(upstream);
}

function startObserve(opts) {
  const state = {
    selectedSession: null,
    // Same-session chat requests currently in flight, keyed by session identity.
    // Used to link transport-level concurrency peers from arrival time.
    sessionInflight: new Map(),
    previous: null,
    // Finalized selected contexts whose comparison awaits peer resolution.
    deferred: [],
  };
  const sockets = new Set();
  const server = http.createServer((req, res) => handleRequest(req, res, opts, state));

  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });

  server.on("error", () => {
    process.stderr.write(
      jsonLine({
        event: "error",
        reason: "listen-failed",
        message: "failed to bind loopback listener",
      }),
    );
    process.exit(1);
  });

  server.listen(opts.listen, "127.0.0.1", () => {
    process.stdout.write(
      jsonLine({ event: "listening", port: server.address().port }),
    );
  });

  let shuttingDown = false;
  const shutdown = () => {
    if (shuttingDown) return;
    shuttingDown = true;
    state.previous = null;
    for (const socket of sockets) socket.destroy();
    sockets.clear();
    server.close(() => process.exit(0));
    // Guarantee termination if the close callback is delayed.
    setTimeout(() => process.exit(0), 500).unref();
  };
  process.on("SIGTERM", shutdown);
  process.on("SIGINT", shutdown);
}

// ---------------------------------------------------------------------------
// Bounded two-path comparison (`compare`)
// ---------------------------------------------------------------------------

// Six required credential environment variables, read only from the process
// environment. No configuration discovery or mutation happens here.
const COMPARE_PATH_SPECS = [
  {
    path: "gateway",
    urlVar: "CACHE_DIAG_GATEWAY_BASE_URL",
    modelVar: "CACHE_DIAG_GATEWAY_MODEL",
    keyVar: "CACHE_DIAG_GATEWAY_KEY",
  },
  {
    path: "upstream",
    urlVar: "CACHE_DIAG_UPSTREAM_BASE_URL",
    modelVar: "CACHE_DIAG_UPSTREAM_MODEL",
    keyVar: "CACHE_DIAG_UPSTREAM_KEY",
  },
];

// Fixed synthetic workload: a public 256 KiB ASCII prefix, ten append-only
// requests per series, two rounds per path, two warmups per series. Two paths x
// two rounds x ten requests = at most 40 model POST attempts, with no retry.
const COMPARE_PREFIX_CHARS = 256 * 1024;
const COMPARE_SERIES_LENGTH = 10;
const COMPARE_WARMUP_COUNT = 2;
const COMPARE_ROUND_COUNT = 2;
const COMPARE_EFFECTIVENESS_TARGET = 0.98;
const COMPARE_MAX_ATTEMPTS = 40;

// A `compare` POST against an accepted-but-unanswered endpoint would otherwise
// wait indefinitely; this per-attempt bound may be overridden by
// `CACHE_DIAG_COMPARE_REQUEST_TIMEOUT_MS`.
const COMPARE_REQUEST_TIMEOUT_MS_DEFAULT = 600000;

function isNonEmptyEnvValue(value) {
  return typeof value === "string" && value !== "";
}

// Strict positive-millisecond override; an absent, unparsable, zero or negative
// value falls back to the default instead of failing the run.
function compareRequestTimeoutMs() {
  const parsed = strictNonNegativeInteger(
    process.env.CACHE_DIAG_COMPARE_REQUEST_TIMEOUT_MS,
  );
  if (parsed === null || parsed <= 0) return COMPARE_REQUEST_TIMEOUT_MS_DEFAULT;
  return parsed;
}

// Deterministic, public, ASCII-only prefix; it contains no environment,
// credential or model value and never depends on a model response.
function buildPublicPrefix() {
  const unit = "DeepSeek cache diagnostic synthetic public prefix line. ";
  return unit
    .repeat(Math.ceil(COMPARE_PREFIX_CHARS / unit.length))
    .slice(0, COMPARE_PREFIX_CHARS);
}

// Ten append-only request bodies for one series. Earlier messages are never
// rewritten, every body carries the explicit disabled thinking mode and the
// small output limit, and no model response is ever fed into a later body.
function buildSeriesBodies(model) {
  const messages = [{ role: "system", content: buildPublicPrefix() }];
  const bodies = [];
  for (let step = 0; step < COMPARE_SERIES_LENGTH; step += 1) {
    messages.push({ role: "user", content: `append-only step ${step}` });
    bodies.push({
      model,
      messages: messages.map((message) => ({ ...message })),
      thinking: { type: "disabled" },
      max_tokens: 32,
    });
  }
  return bodies;
}

// Append the chat completions route to the configured base without duplicating a
// version segment: a base that already ends in `/v1` gets `/chat/completions`,
// any other base gets `/v1/chat/completions`.
function compareEndpoint(baseUrl) {
  const url = new URL(baseUrl);
  const basePath = url.pathname.replace(/\/+$/, "");
  const route = /\/v1$/.test(basePath) ? "/chat/completions" : "/v1/chat/completions";
  url.pathname = `${basePath}${route}`;
  url.search = "";
  url.hash = "";
  return url;
}

// Returns null when any required variable is absent or the base URL is
// unusable; the caller then reports a static missing-config result with zero
// attempts. Values are never echoed.
function resolveCompareConfig(env) {
  const configs = [];
  for (const spec of COMPARE_PATH_SPECS) {
    const baseUrl = env[spec.urlVar];
    const model = env[spec.modelVar];
    const key = env[spec.keyVar];
    if (
      !isNonEmptyEnvValue(baseUrl) ||
      !isNonEmptyEnvValue(model) ||
      !isNonEmptyEnvValue(key)
    ) {
      return null;
    }
    let endpoint;
    try {
      endpoint = compareEndpoint(baseUrl);
    } catch {
      return null;
    }
    configs.push({ path: spec.path, endpoint, model, key });
  }
  return configs;
}

function seedCoverage() {
  return {
    requests: 0,
    valid_usage: 0,
    inclusive_input_tokens: 0,
    cache_hit_tokens: 0,
    cache_hit_rate: null,
    fields: { native: 0, nested: 0 },
  };
}

// `requests` counts every attempt made in the phase; `valid_usage` and the
// token sums only count attempts whose response carried valid usage. Native and
// nested source presence are counted per valid usage without double-counting the
// tokens.
function accumulateValidUsage(coverage, usage) {
  coverage.valid_usage += 1;
  coverage.inclusive_input_tokens += usage.inclusive_input_tokens;
  coverage.cache_hit_tokens += usage.cache_hit_tokens;
  if (usage.fields) {
    if (usage.fields.native) coverage.fields.native += 1;
    if (usage.fields.nested) coverage.fields.nested += 1;
  }
}

function finalizeCoverage(coverage) {
  coverage.cache_hit_rate =
    coverage.inclusive_input_tokens === 0
      ? null
      : coverage.cache_hit_tokens / coverage.inclusive_input_tokens;
}

// One POST; the credential travels only in the Authorization header and neither
// the body nor any raw error text is retained outside this call. The transport
// module follows the configured protocol, so an https endpoint is negotiated as
// TLS and any construction or handshake failure becomes a plain `transport`
// outcome instead of an uncaught error.
function postChat(endpoint, key, bodyText) {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (outcome) => {
      if (settled) return;
      settled = true;
      resolve(outcome);
    };
    const requester = endpoint.protocol === "https:" ? https : http;
    let req;
    try {
      req = requester.request(
        endpoint,
        {
          method: "POST",
          agent: false,
          headers: {
            "content-type": "application/json",
            "content-length": Buffer.byteLength(bodyText),
            authorization: `Bearer ${key}`,
          },
        },
        (res) => {
          const chunks = [];
          res.on("data", (chunk) => chunks.push(chunk));
          res.on("end", () =>
            finish({
              kind: "response",
              status: res.statusCode || 0,
              text: Buffer.concat(chunks).toString("utf8"),
            }),
          );
          res.on("error", () => finish({ kind: "transport" }));
          res.on("aborted", () => finish({ kind: "transport" }));
        },
      );
    } catch {
      finish({ kind: "transport" });
      return;
    }
    // Socket-level idle bound: it covers both awaiting response headers and a
    // stalled response body, resolving as the same `transport` outcome and
    // destroying the request so the process can exit.
    req.setTimeout(compareRequestTimeoutMs(), () => {
      finish({ kind: "transport" });
      req.destroy();
    });
    req.on("error", () => finish({ kind: "transport" }));
    req.end(bodyText);
  });
}

function usageFailureReason(usage) {
  return usage.reason === "missing-usage" ? "usage-unavailable" : "usage-invalid";
}

// Ten sequential requests of one series: the first two are unreported warmup and
// the rest are measured. Any rejection, transport failure or unusable usage stops
// the series immediately with its real attempt count, counting that attempt in
// its phase coverage. A measured response with zero inclusive input has no
// positive cache denominator and is not cache-eligible, so it also stops the run
// rather than producing a fabricated 0% measurement.
async function runSeries(config, bodies, round) {
  const warmup = seedCoverage();
  const measurement = seedCoverage();
  let attempts = 0;
  let failure = null;

  for (let index = 0; index < bodies.length; index += 1) {
    attempts += 1;
    const measured = index >= COMPARE_WARMUP_COUNT;
    const phase = measured ? measurement : warmup;
    phase.requests += 1;
    const outcome = await postChat(
      config.endpoint,
      config.key,
      JSON.stringify(bodies[index]),
    );
    if (outcome.kind === "transport") {
      failure = "transport-failure";
      break;
    }
    if (outcome.status < 200 || outcome.status >= 300) {
      failure = "request-rejected";
      break;
    }
    const usage = usageFromBufferedResponse(outcome.text);
    if (usage.availability !== "available") {
      failure = usageFailureReason(usage);
      break;
    }
    accumulateValidUsage(phase, usage);
    if (measured && usage.inclusive_input_tokens === 0) {
      failure = "usage-invalid";
      break;
    }
  }

  finalizeCoverage(warmup);
  finalizeCoverage(measurement);
  return {
    attempts,
    failure,
    series: { path: config.path, round, attempts, warmup, measurement },
  };
}

function aggregateMeasured(series, path) {
  let inclusive = 0;
  let hit = 0;
  for (const entry of series) {
    if (entry.path !== path) continue;
    inclusive += entry.measurement.inclusive_input_tokens;
    hit += entry.measurement.cache_hit_tokens;
  }
  return { inclusive, hit, rate: inclusive === 0 ? 0 : hit / inclusive };
}

function buildComparisonReport({
  startedAt,
  completedAt,
  attempts,
  series,
  failure,
  completed,
}) {
  const report = {
    event: "comparison",
    attempts,
    status: completed ? "completed" : "incomplete",
    comparable: false,
    effectiveness: "unverified",
    target_met: completed
      ? aggregateMeasured(series, "gateway").rate >= COMPARE_EFFECTIVENESS_TARGET
      : null,
    cost: { availability: "unknown" },
    started_at: startedAt.toISOString(),
    completed_at: completedAt.toISOString(),
    series,
  };
  if (!completed) report.reason = failure;
  return report;
}

async function runComparison(configs, startedAt) {
  const series = [];
  let attempts = 0;
  let failure = null;

  for (const config of configs) {
    if (failure) break;
    const bodies = buildSeriesBodies(config.model);
    for (let round = 1; round <= COMPARE_ROUND_COUNT; round += 1) {
      if (failure) break;
      const result = await runSeries(config, bodies, round);
      attempts += result.attempts;
      series.push(result.series);
      failure = result.failure;
      if (attempts >= COMPARE_MAX_ATTEMPTS) break;
    }
  }

  const completed =
    failure === null &&
    series.length === configs.length * COMPARE_ROUND_COUNT &&
    series.every((entry) => entry.attempts === COMPARE_SERIES_LENGTH);

  return buildComparisonReport({
    startedAt,
    completedAt: new Date(),
    attempts,
    series,
    failure,
    completed,
  });
}

function compareCommand() {
  const startedAt = new Date();
  const configs = resolveCompareConfig(process.env);
  if (configs === null) {
    process.stdout.write(
      jsonLine(
        buildComparisonReport({
          startedAt,
          completedAt: new Date(),
          attempts: 0,
          series: [],
          failure: "missing-config",
          completed: false,
        }),
      ),
    );
    process.exitCode = 1;
    return;
  }

  runComparison(configs, startedAt).then((report) => {
    process.stdout.write(jsonLine(report));
    process.exitCode = report.status === "completed" ? 0 : 1;
  });
}

function main() {
  const argv = process.argv.slice(2);
  const command = argv[0];

  if (command === "observe") {
    const opts = parseObserveArgs(argv.slice(1));
    startObserve(opts);
    return;
  }

  if (command === "compare") {
    if (argv.length > 1) {
      failInvalidArguments("compare takes no arguments");
    }
    compareCommand();
    return;
  }

  failInvalidArguments("expected a command: observe or compare");
}

main();
