#!/usr/bin/env node
// Behavior tests for `tools/diagnose-deepseek-cache.mjs observe`
// (plan 20261015-deepseek-cache-diagnostics, Step 2 / REQ-003 / AC-003).
//
// Every test spawns the real CLI process and talks to it over real loopback
// sockets against a local mock gateway. No internal function is imported, no
// internal module is mocked, and no external model or network call is made.
//
// The asserted PUBLIC output contract (returned to Backend as the agreed shape):
//
//   observation event:
//     {
//       event: "observation",
//       comparison: "append-only" | "changed" | "unavailable",
//       first_changed_path: string,   // required only when comparison=changed
//       reason: string,               // required only when comparison=unavailable
//       usage: { ... },               // see below
//       http_status: number,
//     }
//
//   comparison `reason` codes (static availability codes):
//     no-predecessor | overlap | unsupported-format | oversized-metadata |
//     parse-failure
//
//   usage object:
//     availability: "available" | "unavailable"
//     when available:
//       inclusive_input_tokens: number
//       cache_hit_tokens: number
//       cache_miss_tokens: number
//       output_tokens: number | null      // null when upstream omits a count
//       cache_hit_rate: number | null     // cache_hit_tokens / inclusive_input_tokens;
//                                         // null when inclusive_input_tokens is 0
//     when unavailable:
//       reason: "missing-usage" | "malformed-usage" | "native-nested-disagree"
//               | "unsupported-format"
//     fields: { native: boolean, nested: boolean }   // source presence
//
// A non-identity response `content-encoding` is unsupported: its usage is
// reported as `unsupported-format`, never as missing usage, and the compressed
// response bytes are forwarded unchanged.
//
// Selected conversations: the model is `--model deepseek-v4.1-flash`, the route
// is /v1/chat/completions, and the session identity uses the gateway's existing
// header precedence (x-session-affinity first). A request is compared only
// against the previous *completed* selected request.

import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import net from "node:net";
import zlib from "node:zlib";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { fileURLToPath } from "node:url";

const TOOL_PATH = fileURLToPath(
  new URL("./diagnose-deepseek-cache.mjs", import.meta.url),
);
const MODEL = "deepseek-v4.1-flash";
const SESSION_HEADER = "x-session-affinity";
const CHAT_PATH = "/v1/chat/completions";

const COMPARISON_REASONS = new Set([
  "no-predecessor",
  "overlap",
  "unsupported-format",
  "oversized-metadata",
  "parse-failure",
]);
const USAGE_REASONS = new Set([
  "missing-usage",
  "malformed-usage",
  "native-nested-disagree",
  "unsupported-format",
]);

const VALID_NATIVE_USAGE = {
  prompt_tokens: 100,
  completion_tokens: 7,
  prompt_cache_hit_tokens: 98,
  prompt_cache_miss_tokens: 2,
};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function withTimeout(promise, ms, label) {
  let timer;
  const guard = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`INFRA: timed out waiting for ${label}`)),
      ms,
    );
  });
  try {
    return await Promise.race([promise, guard]);
  } finally {
    clearTimeout(timer);
  }
}

async function waitFor(predicate, { timeout = 8000, label = "condition" } = {}) {
  const start = Date.now();
  for (;;) {
    const value = predicate();
    if (value) return value;
    if (Date.now() - start > timeout) {
      throw new Error(`INFRA: timed out waiting for ${label}`);
    }
    await delay(5);
  }
}

// A configurable loopback gateway. `setHandler(fn)` decides each response; the
// handler receives the captured record, the ServerResponse and the 1-based
// request index.
function createMockGateway() {
  const requests = [];
  let handler = (record, res) => {
    res.writeHead(200, { "content-type": "application/json" });
    res.end("{}");
  };
  const server = http.createServer((req, res) => {
    const chunks = [];
    const state = { reqAborted: false, resClosedEarly: false };
    req.on("aborted", () => {
      state.reqAborted = true;
    });
    res.on("close", () => {
      if (!res.writableFinished) state.resClosedEarly = true;
    });
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      const record = {
        method: req.method,
        url: req.url,
        headers: req.headers,
        body: Buffer.concat(chunks),
        state,
      };
      requests.push(record);
      Promise.resolve(handler(record, res, requests.length)).catch(() => {});
    });
  });
  return {
    requests,
    server,
    setHandler(fn) {
      handler = fn;
    },
    async listen() {
      server.listen(0, "127.0.0.1");
      await once(server, "listening");
      return server.address().port;
    },
    async close() {
      server.closeAllConnections?.();
      await new Promise((resolve) => server.close(() => resolve()));
    },
  };
}

function startObserver(gatewayPort, { gatewayUrl } = {}) {
  const child = spawn(
    process.execPath,
    [
      TOOL_PATH,
      "observe",
      "--listen",
      "0",
      "--gateway",
      gatewayUrl ?? `http://127.0.0.1:${gatewayPort}`,
      "--model",
      MODEL,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );

  const events = [];
  const chunks = [];
  let stderr = "";
  let pending = "";

  child.stdout.on("data", (data) => {
    chunks.push(data);
    pending += data.toString("utf8");
    let index;
    while ((index = pending.indexOf("\n")) >= 0) {
      const line = pending.slice(0, index);
      pending = pending.slice(index + 1);
      if (line.trim() === "") continue;
      try {
        events.push(JSON.parse(line));
      } catch {
        events.push({ event: "__unparsed__", line });
      }
    }
  });
  child.stderr.on("data", (data) => {
    stderr += data.toString("utf8");
  });

  const observations = () =>
    events.filter((event) => event.event === "observation");

  return {
    child,
    events,
    observations,
    stdoutText: () => Buffer.concat(chunks).toString("utf8"),
    stderrText: () => stderr,
    async waitListening() {
      return waitFor(
        () => {
          const listening = events.find(
            (event) => event.event === "listening" && Number.isInteger(event.port),
          );
          if (listening) return listening;
          if (child.exitCode !== null || child.signalCode !== null) {
            throw new Error(`INFRA: observer exited before listening: ${stderr}`);
          }
          return null;
        },
        { label: "listening", timeout: 8000 },
      );
    },
    async waitForObservations(min) {
      await waitFor(() => observations().length >= min, {
        label: `${min} observation event(s)`,
        timeout: 8000,
      });
    },
    async stop() {
      if (child.exitCode !== null || child.signalCode !== null) return;
      child.kill("SIGTERM");
      try {
        await withTimeout(once(child, "exit"), 3000, "observer SIGTERM exit");
      } catch {
        child.kill("SIGKILL");
      }
    },
  };
}

async function setup(t, handler) {
  const mock = createMockGateway();
  if (handler) mock.setHandler(handler);
  const gatewayPort = await mock.listen();
  const observer = startObserver(gatewayPort);
  t.after(async () => {
    await observer.stop();
    await mock.close();
  });
  const listening = await observer.waitListening();
  return { mock, observer, port: listening.port, gatewayPort };
}

// Wait for at least `min` observations, then let the stream quiesce so every
// already-completed request's event is collected deterministically.
async function collect(observer, min) {
  await observer.waitForObservations(min);
  let previous = -1;
  while (previous !== observer.observations().length) {
    previous = observer.observations().length;
    await delay(120);
  }
  return observer.observations();
}

function request(port, { method = "POST", path = CHAT_PATH, headers, body } = {}) {
  return new Promise((resolve, reject) => {
    const req = http.request(
      { host: "127.0.0.1", port, method, path, headers, agent: false },
      (res) => {
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () =>
          resolve({
            status: res.statusCode,
            headers: res.headers,
            body: Buffer.concat(chunks),
          }),
        );
      },
    );
    req.on("error", reject);
    if (body !== undefined) req.write(body);
    req.end();
  });
}

function chatHeaders({ session, extra = {}, contentType = "application/json" } = {}) {
  const headers = {};
  if (contentType) headers["content-type"] = contentType;
  if (session) headers[SESSION_HEADER] = session;
  return { ...headers, ...extra };
}

function sendJson(port, { session, path = CHAT_PATH, body, extra = {} } = {}) {
  const text = typeof body === "string" ? body : JSON.stringify(body);
  const headers = chatHeaders({ session, extra });
  headers["content-length"] = Buffer.byteLength(text);
  return request(port, { path, headers, body: text });
}

function chatBody({ messages, model = MODEL, tools, metadata, user_id, extra = {} }) {
  const body = { model, messages };
  if (tools !== undefined) body.tools = tools;
  if (metadata !== undefined) body.metadata = metadata;
  if (user_id !== undefined) body.user_id = user_id;
  return { ...body, ...extra };
}

function chatResponse(usage) {
  const payload = {
    id: "chatcmpl-test",
    object: "chat.completion",
    choices: [
      {
        index: 0,
        message: { role: "assistant", content: "ok" },
        finish_reason: "stop",
      },
    ],
  };
  if (usage !== undefined) payload.usage = usage;
  return JSON.stringify(payload);
}

function jsonReply(usage) {
  return (record, res) => {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(chatResponse(usage));
  };
}

function assertAvailableUsage(usage, expected) {
  assert.equal(usage?.availability, "available", "usage must be available");
  assert.equal(usage.inclusive_input_tokens, expected.inclusive);
  assert.equal(usage.cache_hit_tokens, expected.hit);
  assert.equal(usage.cache_miss_tokens, expected.miss);
  if (expected.output === null) {
    assert.equal(usage.output_tokens, null, "unknown output stays null");
  } else {
    assert.equal(usage.output_tokens, expected.output);
  }
  assert.ok(
    Math.abs(usage.cache_hit_rate - expected.rate) < 1e-9,
    `cache_hit_rate must be ${expected.rate}, got ${usage.cache_hit_rate}`,
  );
}

function assertUnavailableUsage(usage, reason) {
  assert.equal(usage?.availability, "unavailable", "usage must be unavailable");
  assert.ok(
    USAGE_REASONS.has(usage.reason),
    `usage.reason must be a known code, got ${usage.reason}`,
  );
  if (reason) assert.equal(usage.reason, reason);
}

const MESSAGES = [
  { role: "system", content: "You are helpful." },
  { role: "user", content: "hello" },
];

function appended(messages, extra) {
  return messages.concat(extra ?? [{ role: "assistant", content: "ok" }]);
}

// ---------------------------------------------------------------------------
// 1. Comparison classification
// ---------------------------------------------------------------------------

test(
  "observe reports append-only for a selected conversation and no predecessor for the first request",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    const obs = await collect(observer, 2);
    assert.equal(obs[0].comparison, "unavailable");
    assert.equal(obs[0].reason, "no-predecessor");
    assert.equal(obs[1].comparison, "append-only");
  },
);

test(
  "observe reports the changed reasoning_content path for an empty and a null old value",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    const base = [
      { role: "system", content: "sys" },
      { role: "assistant", content: "a", reasoning_content: "think" },
      { role: "user", content: "u" },
    ];
    const emptyReasoning = [
      { role: "system", content: "sys" },
      { role: "assistant", content: "a", reasoning_content: "" },
      { role: "user", content: "u" },
    ];
    const nullReasoning = [
      { role: "system", content: "sys" },
      { role: "assistant", content: "a", reasoning_content: null },
      { role: "user", content: "u" },
    ];
    await sendJson(port, { session: "S1", body: chatBody({ messages: base }) });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: emptyReasoning }),
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: nullReasoning }),
    });
    const obs = await collect(observer, 3);
    assert.equal(obs[1].comparison, "changed");
    assert.equal(obs[1].first_changed_path, "$.messages[1].reasoning_content");
    assert.equal(obs[2].comparison, "changed");
    assert.equal(obs[2].first_changed_path, "$.messages[1].reasoning_content");
  },
);

test(
  "observe reports the changed tool definition path",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    const tool = (name, description) => ({
      type: "function",
      function: { name, description, parameters: { type: "object", properties: {} } },
    });
    const baseTools = [tool("f", "d1"), tool("g", "d2")];
    const changedTools = [tool("f", "d1-changed"), tool("g", "d2")];
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES, tools: baseTools }),
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES, tools: changedTools }),
    });
    const obs = await collect(observer, 2);
    assert.equal(obs[1].comparison, "changed");
    assert.equal(obs[1].first_changed_path, "$.tools[0].function.description");
  },
);

test(
  "observe treats a reordered tool list as a change at the tools position",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    const tool = (name) => ({
      type: "function",
      function: { name, description: name, parameters: { type: "object" } },
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES, tools: [tool("f"), tool("g")] }),
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES, tools: [tool("g"), tool("f")] }),
    });
    const obs = await collect(observer, 2);
    assert.equal(obs[1].comparison, "changed");
    assert.match(obs[1].first_changed_path, /^\$\.tools\[/);
  },
);

test(
  "observe reports system content, user_id and metadata.user_id differences",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    const baseBody = chatBody({
      messages: MESSAGES,
      user_id: "u1",
      metadata: { user_id: "m1" },
    });
    await sendJson(port, { session: "S1", body: baseBody });
    await sendJson(port, {
      session: "S1",
      body: chatBody({
        messages: MESSAGES,
        user_id: "u1",
        metadata: { user_id: "m2" },
      }),
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({
        messages: MESSAGES,
        user_id: "u2",
        metadata: { user_id: "m2" },
      }),
    });
    const systemChanged = MESSAGES.map((message, index) =>
      index === 0 ? { ...message, content: "changed system" } : message,
    );
    await sendJson(port, {
      session: "S1",
      body: chatBody({
        messages: systemChanged,
        user_id: "u2",
        metadata: { user_id: "m2" },
      }),
    });
    const obs = await collect(observer, 4);
    assert.equal(obs[1].comparison, "changed");
    assert.equal(obs[1].first_changed_path, "$.metadata.user_id");
    assert.equal(obs[2].comparison, "changed");
    assert.equal(obs[2].first_changed_path, "$.user_id");
    assert.equal(obs[3].comparison, "changed");
    assert.equal(obs[3].first_changed_path, "$.messages[0].content");
  },
);

// ---------------------------------------------------------------------------
// 2. Transparent transport
// ---------------------------------------------------------------------------

test(
  "observe forwards request bytes and identity headers unchanged and returns the JSON response byte-for-byte",
  { timeout: 15000 },
  async (t) => {
    const { mock, port, gatewayPort } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    const text = JSON.stringify(chatBody({ messages: MESSAGES }));
    const response = await request(port, {
      headers: {
        ...chatHeaders({ session: "S1" }),
        "content-length": Buffer.byteLength(text),
        authorization: "Bearer local-key",
        "x-request-marker": "marker-1",
      },
      body: text,
    });
    assert.equal(mock.requests.length, 1);
    const captured = mock.requests[0];
    assert.equal(captured.url, CHAT_PATH);
    assert.deepEqual(captured.body, Buffer.from(text));
    assert.equal(captured.headers.authorization, "Bearer local-key");
    assert.equal(captured.headers[SESSION_HEADER], "S1");
    assert.match(captured.headers["content-type"], /application\/json/);
    assert.equal(captured.headers["x-request-marker"], "marker-1");
    assert.equal(captured.headers.host, `127.0.0.1:${gatewayPort}`);
    assert.equal(response.body.toString("utf8"), chatResponse(VALID_NATIVE_USAGE));
  },
);

test(
  "observe forwards a fragmented SSE response byte-for-byte and reads its usage",
  { timeout: 15000 },
  async (t) => {
    const sse = [
      'data: {"id":"c1","choices":[{"index":0,"delta":{"role":"assistant","content":"He"}}]}\n\n',
      'data: {"id":"c1","choices":[{"index":0,"delta":{"content":"llo"}}]}\n\n',
      'data: {"id":"c1","choices":[],"usage":{"prompt_tokens":100,"completion_tokens":7,"prompt_cache_hit_tokens":98,"prompt_cache_miss_tokens":2}}\n\n',
      "data: [DONE]\n\n",
    ].join("");
    const handler = async (record, res, index) => {
      if (index === 2) {
        res.writeHead(200, { "content-type": "text/event-stream" });
        const buffer = Buffer.from(sse);
        for (let i = 0; i < buffer.length; i += 7) {
          res.write(buffer.subarray(i, i + 7));
          await delay(1);
        }
        res.end();
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(chatResponse(VALID_NATIVE_USAGE));
    };
    const { observer, port } = await setup(t, handler);
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    const response = await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    assert.equal(response.body.toString("utf8"), sse);
    const obs = await collect(observer, 2);
    assert.equal(obs[1].comparison, "append-only");
    assertAvailableUsage(obs[1].usage, {
      inclusive: 100,
      hit: 98,
      miss: 2,
      output: 7,
      rate: 0.98,
    });
  },
);

// ---------------------------------------------------------------------------
// 3. Usage normalization
// ---------------------------------------------------------------------------

test(
  "observe reports available numeric usage for native-only, nested-only and consistent dual shapes",
  { timeout: 15000 },
  async (t) => {
    const usages = [
      VALID_NATIVE_USAGE,
      {
        prompt_tokens: 100,
        completion_tokens: 7,
        prompt_tokens_details: { cached_tokens: 98 },
      },
      {
        prompt_tokens: 100,
        completion_tokens: 7,
        prompt_tokens_details: { cached_tokens: 98 },
        prompt_cache_hit_tokens: 98,
        prompt_cache_miss_tokens: 2,
      },
    ];
    const handler = (record, res, index) => jsonReply(usages[index - 1])(record, res, index);
    const { observer, port } = await setup(t, handler);
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(appended(MESSAGES)) }),
    });
    const obs = await collect(observer, 3);
    for (const observation of obs) {
      assertAvailableUsage(observation.usage, {
        inclusive: 100,
        hit: 98,
        miss: 2,
        output: 7,
        rate: 0.98,
      });
    }
  },
);

test(
  "observe distinguishes zero cache hits from missing usage and marks malformed usage unavailable",
  { timeout: 15000 },
  async (t) => {
    const responses = [
      { prompt_tokens: 100, completion_tokens: 7, prompt_cache_hit_tokens: 0 },
      undefined,
      { prompt_tokens: 100, prompt_cache_hit_tokens: "abc" },
    ];
    const handler = (record, res, index) =>
      jsonReply(responses[index - 1])(record, res, index);
    const { observer, port } = await setup(t, handler);
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(appended(MESSAGES)) }),
    });
    const obs = await collect(observer, 3);
    assertAvailableUsage(obs[0].usage, {
      inclusive: 100,
      hit: 0,
      miss: 100,
      output: 7,
      rate: 0,
    });
    assertUnavailableUsage(obs[1].usage, "missing-usage");
    assertUnavailableUsage(obs[2].usage, "malformed-usage");
  },
);

test(
  "observe reports native/nested disagreement as unavailable without merging the hit",
  { timeout: 15000 },
  async (t) => {
    const responses = [
      {
        prompt_tokens: 100,
        completion_tokens: 7,
        prompt_cache_hit_tokens: 98,
        prompt_tokens_details: { cached_tokens: 98 },
      },
      {
        prompt_tokens: 100,
        completion_tokens: 7,
        prompt_cache_hit_tokens: 90,
        prompt_tokens_details: { cached_tokens: 98 },
      },
    ];
    const handler = (record, res, index) =>
      jsonReply(responses[index - 1])(record, res, index);
    const { observer, port } = await setup(t, handler);
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    const obs = await collect(observer, 2);
    assertAvailableUsage(obs[0].usage, {
      inclusive: 100,
      hit: 98,
      miss: 2,
      output: 7,
      rate: 0.98,
    });
    assertUnavailableUsage(obs[1].usage, "native-nested-disagree");
    assert.equal(obs[1].usage.fields?.native, true);
    assert.equal(obs[1].usage.fields?.nested, true);
  },
);

test(
  "observe reports unknown output tokens as null without inventing a count",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(
      t,
      jsonReply({ prompt_tokens: 100, prompt_cache_hit_tokens: 0 }),
    );
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    const obs = await collect(observer, 1);
    assertAvailableUsage(obs[0].usage, {
      inclusive: 100,
      hit: 0,
      miss: 100,
      output: null,
      rate: 0,
    });
  },
);

// ---------------------------------------------------------------------------
// 4. Session identity selection
// ---------------------------------------------------------------------------

function mutateUserMessage(messages, content) {
  return messages.map((message, index) =>
    index === 1 ? { ...message, content } : message,
  );
}

test(
  "observe selects the highest-precedence non-empty session header",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    await sendJson(port, {
      session: "S-high",
      body: chatBody({ messages: MESSAGES }),
      extra: { "x-session-id": "S-low" },
    });
    await sendJson(port, {
      session: "S-high",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await sendJson(port, {
      session: undefined,
      body: chatBody({ messages: mutateUserMessage(MESSAGES, "low-session-change") }),
      extra: { "x-session-id": "S-low" },
    });
    const obs = await collect(observer, 2);
    assert.equal(
      obs.filter((o) => o.comparison === "changed").length,
      0,
      "the lower-precedence session must not become the observed baseline",
    );
    assert.equal(
      obs.filter((o) => o.comparison === "append-only").length,
      1,
    );
  },
);

test(
  "observe ignores another conversation and a missing identity without polluting the baseline",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    await sendJson(port, { session: "S-main", body: chatBody({ messages: MESSAGES }) });
    await sendJson(port, {
      session: "S-other",
      body: chatBody({ messages: mutateUserMessage(MESSAGES, "other-session-change") }),
    });
    await sendJson(port, {
      session: "S-main",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await sendJson(port, {
      session: undefined,
      body: chatBody({ messages: mutateUserMessage(MESSAGES, "missing-identity-change") }),
    });
    await sendJson(port, {
      session: "S-main",
      body: chatBody({ messages: appended(appended(MESSAGES)) }),
    });
    const obs = await collect(observer, 3);
    assert.equal(obs.filter((o) => o.comparison === "changed").length, 0);
    assert.equal(obs.filter((o) => o.comparison === "append-only").length, 2);
  },
);

test(
  "observe passes other models and other routes through without affecting the baseline",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    await sendJson(port, { session: "S-main", body: chatBody({ messages: MESSAGES }) });
    await sendJson(port, {
      session: "S-main",
      body: chatBody({ messages: mutateUserMessage(MESSAGES, "other-model-change"), model: "other-model" }),
    });
    await sendJson(port, {
      session: "S-main",
      path: "/v1/completions",
      body: chatBody({ messages: mutateUserMessage(MESSAGES, "other-route-change") }),
    });
    await sendJson(port, {
      session: "S-main",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    const obs = await collect(observer, 2);
    assert.equal(obs.filter((o) => o.comparison === "changed").length, 0);
    assert.equal(obs.filter((o) => o.comparison === "append-only").length, 1);
  },
);

// ---------------------------------------------------------------------------
// 5. Overlapping selected requests
// ---------------------------------------------------------------------------

test(
  "observe marks both concurrently overlapped selected requests unavailable and never fabricates a predecessor",
  { timeout: 15000 },
  async (t) => {
    let releaseFirst;
    let releaseSecond;
    const firstGate = new Promise((resolve) => {
      releaseFirst = resolve;
    });
    const secondGate = new Promise((resolve) => {
      releaseSecond = resolve;
    });
    const handler = async (record, res, index) => {
      if (index === 1) await firstGate;
      if (index === 2) await secondGate;
      res.writeHead(200, { "content-type": "application/json" });
      res.end(chatResponse(VALID_NATIVE_USAGE));
    };
    const { mock, observer, port } = await setup(t, handler);

    const firstResponse = sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES }),
    });
    await waitFor(() => mock.requests.length >= 1, { label: "first upstream request" });
    const secondResponse = sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await waitFor(() => mock.requests.length >= 2, { label: "second upstream request" });
    releaseFirst();
    releaseSecond();
    await firstResponse;
    await secondResponse;

    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(appended(MESSAGES)) }),
    });
    const obs = await collect(observer, 3);
    for (const overlapped of obs.slice(0, 2)) {
      assert.equal(
        overlapped.comparison,
        "unavailable",
        "each concurrent selected request is comparison-unavailable",
      );
      assert.equal(
        overlapped.reason,
        "overlap",
        "every request that ran concurrently with another must be marked overlap",
      );
    }
    assert.equal(
      obs[2].comparison,
      "unavailable",
      "the request after the overlaps has no known predecessor",
    );
    assert.equal(obs[2].reason, "no-predecessor");
    assert.equal(
      obs.filter((o) => o.comparison === "append-only").length,
      0,
      "no overlapped request may be reused as a known predecessor",
    );
  },
);

// ---------------------------------------------------------------------------
// 6. Disconnect cancellation
// ---------------------------------------------------------------------------

function sendAndAbort(port, { session, body }) {
  const text = JSON.stringify(body);
  const headers = chatHeaders({ session });
  headers["content-length"] = Buffer.byteLength(text);
  return new Promise((resolve) => {
    const req = http.request(
      { host: "127.0.0.1", port, method: "POST", path: CHAT_PATH, headers, agent: false },
      (res) => {
        res.on("data", () => {
          req.destroy();
          resolve("destroyed");
        });
        res.on("end", () => resolve("ended"));
      },
    );
    req.on("error", () => resolve("error"));
    req.end(text);
  });
}

test(
  "observe aborts upstream on client disconnect and still compares the next request",
  { timeout: 15000 },
  async (t) => {
    const handler = (record, res, index) => {
      if (index === 2) {
        res.writeHead(200, { "content-type": "application/json" });
        res.write('{"partial"');
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(chatResponse(VALID_NATIVE_USAGE));
    };
    const { mock, observer, port } = await setup(t, handler);
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    await waitFor(() => mock.requests.length >= 1, { label: "first request" });

    await sendAndAbort(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await waitFor(
      () =>
        mock.requests.length >= 2 &&
        (mock.requests[1].state.reqAborted || mock.requests[1].state.resClosedEarly),
      { label: "upstream abort observed" },
    );

    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    const obs = await collect(observer, 2);
    assert.equal(obs.filter((o) => o.comparison === "changed").length, 0);
    assert.equal(obs[obs.length - 1].comparison, "append-only");
    assertAvailableUsage(obs[obs.length - 1].usage, {
      inclusive: 100,
      hit: 98,
      miss: 2,
      output: 7,
      rate: 0.98,
    });
  },
);

// ---------------------------------------------------------------------------
// 7. Unsupported observation formats pass through as unavailable
// ---------------------------------------------------------------------------

test(
  "observe passes an unsupported content encoding through and marks the comparison unavailable",
  { timeout: 15000 },
  async (t) => {
    const { mock, observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    const gzipBody = zlib.gzipSync(
      Buffer.from(JSON.stringify(chatBody({ messages: appended(MESSAGES) }))),
    );
    await request(port, {
      headers: {
        ...chatHeaders({ session: "S1" }),
        "content-encoding": "gzip",
        "content-length": gzipBody.length,
      },
      body: gzipBody,
    });
    assert.deepEqual(mock.requests[1].body, gzipBody);
    const obs = await collect(observer, 2);
    assert.equal(obs[1].comparison, "unavailable");
    assert.equal(obs[1].reason, "unsupported-format");
  },
);

test(
  "observe marks a non-JSON request body unavailable while forwarding its bytes",
  { timeout: 15000 },
  async (t) => {
    const { mock, observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    const raw = "this-is-not-json";
    await request(port, {
      headers: {
        ...chatHeaders({ session: "S1" }),
        "content-length": Buffer.byteLength(raw),
      },
      body: raw,
    });
    assert.equal(mock.requests[1].body.toString("utf8"), raw);
    const obs = await collect(observer, 2);
    assert.equal(obs[1].comparison, "unavailable");
    assert.ok(
      obs[1].reason === "parse-failure" || obs[1].reason === "unsupported-format",
      `expected a format availability reason, got ${obs[1].reason}`,
    );
  },
);

test(
  "observe marks oversized isolation metadata unavailable while forwarding it unchanged",
  { timeout: 15000 },
  async (t) => {
    const { mock, observer, port } = await setup(t, jsonReply(VALID_NATIVE_USAGE));
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    const body = chatBody({
      messages: appended(MESSAGES),
      metadata: { user_id: "x".repeat(1024 * 1024) },
    });
    const text = JSON.stringify(body);
    await request(port, {
      headers: {
        ...chatHeaders({ session: "S1" }),
        "content-length": Buffer.byteLength(text),
      },
      body: text,
    });
    assert.deepEqual(mock.requests[1].body, Buffer.from(text));
    const obs = await collect(observer, 2);
    assert.equal(obs[1].comparison, "unavailable");
    assert.equal(obs[1].reason, "oversized-metadata");
  },
);

// ---------------------------------------------------------------------------
// 8. Output secrecy
// ---------------------------------------------------------------------------

test(
  "observe never prints echoed fixture secrets, auth values or session values",
  { timeout: 15000 },
  async (t) => {
    const marker = "FIXTURE-SECRET-9f3a";
    const handler = (record, res) => {
      res.writeHead(401, {
        "content-type": "application/json",
        "x-echo-secret": marker,
        "x-request-id": marker,
      });
      res.end(
        JSON.stringify({
          error: { message: `bad key ${marker}`, authorization: `Bearer ${marker}` },
        }),
      );
    };
    const { observer, port } = await setup(t, handler);
    const response = await sendJson(port, {
      session: marker,
      extra: { authorization: `Bearer ${marker}` },
      body: chatBody({ messages: MESSAGES, metadata: { user_id: marker } }),
    });
    await delay(200);
    const output = observer.stdoutText() + observer.stderrText();
    assert.ok(
      !output.includes(marker),
      "the child's stdout/stderr must not contain the echoed fixture value",
    );
    assert.equal(response.status, 401);
    assert.ok(response.body.toString("utf8").includes(marker));
  },
);

// ---------------------------------------------------------------------------
// 9. Lifecycle
// ---------------------------------------------------------------------------

function connectRefused(port) {
  return new Promise((resolve) => {
    const socket = net.connect({ host: "127.0.0.1", port });
    socket.on("connect", () => {
      socket.destroy();
      resolve(false);
    });
    socket.on("error", () => resolve(true));
  });
}

test(
  "SIGTERM releases the loopback listener and any active socket",
  { timeout: 15000 },
  async (t) => {
    const handler = (record, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.write('{"partial"');
    };
    const { mock, observer, port } = await setup(t, handler);
    t.after(async () => {
      await observer.stop();
    });

    const activeSocketClosed = new Promise((resolve) => {
      const body = JSON.stringify(chatBody({ messages: MESSAGES }));
      const req = http.request(
        {
          host: "127.0.0.1",
          port,
          method: "POST",
          path: CHAT_PATH,
          headers: {
            ...chatHeaders({ session: "S1" }),
            "content-length": Buffer.byteLength(body),
          },
          agent: false,
        },
        (res) => {
          res.on("data", () => {});
          res.on("end", () => resolve("end"));
        },
      );
      req.on("error", () => resolve("error"));
      req.on("close", () => resolve("close"));
      req.end(body);
    });
    await waitFor(() => mock.requests.length >= 1, { label: "held upstream request" });

    observer.child.kill("SIGTERM");
    await withTimeout(once(observer.child, "exit"), 3000, "observer exit");
    const outcome = await withTimeout(activeSocketClosed, 3000, "active socket close");
    assert.ok(
      outcome === "error" || outcome === "close",
      `active socket must be released, got ${outcome}`,
    );
    assert.equal(await connectRefused(port), true, "listener must be released");
  },
);

// ---------------------------------------------------------------------------
// 10. Overlap must cover both concurrent selected requests
// ---------------------------------------------------------------------------

test(
  "observe marks both genuinely concurrent selected requests overlapped and never fabricates a predecessor",
  { timeout: 15000 },
  async (t) => {
    let releaseFirst;
    let releaseSecond;
    const firstGate = new Promise((resolve) => {
      releaseFirst = resolve;
    });
    const secondGate = new Promise((resolve) => {
      releaseSecond = resolve;
    });
    const handler = async (record, res, index) => {
      if (index === 1) await firstGate;
      if (index === 2) await secondGate;
      res.writeHead(200, { "content-type": "application/json" });
      res.end(chatResponse(VALID_NATIVE_USAGE));
    };
    const { mock, observer, port } = await setup(t, handler);

    const firstResponse = sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES }),
    });
    await waitFor(() => mock.requests.length >= 1, { label: "first upstream request" });
    const secondResponse = sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await waitFor(() => mock.requests.length >= 2, { label: "second upstream request" });
    releaseFirst();
    releaseSecond();
    await firstResponse;
    await secondResponse;

    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(appended(MESSAGES)) }),
    });
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(appended(appended(MESSAGES))) }),
    });
    const obs = await collect(observer, 4);

    for (const overlapped of obs.slice(0, 2)) {
      assert.equal(
        overlapped.comparison,
        "unavailable",
        "each concurrent selected request is comparison-unavailable",
      );
      assert.equal(
        overlapped.reason,
        "overlap",
        "every request that ran concurrently with another must be marked overlap",
      );
    }
    assert.equal(
      obs[2].comparison,
      "unavailable",
      "no overlapped request may be reused as a known predecessor",
    );
    assert.equal(obs[2].reason, "no-predecessor");
    assert.equal(
      obs[3].comparison,
      "append-only",
      "a later sequential request may append once both overlaps have completed",
    );
  },
);

// ---------------------------------------------------------------------------
// 11. Compressed response bodies are unsupported, never missing usage
// ---------------------------------------------------------------------------

test(
  "observe forwards a gzip-encoded response unchanged and reports unsupported-format instead of missing usage",
  { timeout: 15000 },
  async (t) => {
    const gzipBody = zlib.gzipSync(Buffer.from(chatResponse(VALID_NATIVE_USAGE)));
    const handler = (record, res) => {
      res.writeHead(200, {
        "content-type": "application/json",
        "content-encoding": "gzip",
      });
      res.end(gzipBody);
    };
    const { observer, port } = await setup(t, handler);
    const response = await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES }),
    });
    assert.deepEqual(
      response.body,
      gzipBody,
      "compressed response bytes must be forwarded unchanged",
    );
    const obs = await collect(observer, 1);
    assertUnavailableUsage(obs[0].usage, "unsupported-format");
  },
);

// ---------------------------------------------------------------------------
// 12. SSE usage after the capture cap is still observed
// ---------------------------------------------------------------------------

test(
  "observe extracts trailing SSE usage past the capture cap while forwarding every byte",
  { timeout: 30000 },
  async (t) => {
    const filler = 'data: {"choices":[{"delta":{"content":"x"}}]}\n\n';
    const fillerCount = Math.ceil(
      (8 * 1024 * 1024 + 65536) / Buffer.byteLength(filler),
    );
    const usageEvent =
      'data: {"id":"c1","choices":[],"usage":{"prompt_tokens":100,"completion_tokens":7,"prompt_cache_hit_tokens":98,"prompt_cache_miss_tokens":2}}\n\n';
    const sse = filler.repeat(fillerCount) + usageEvent + "data: [DONE]\n\n";
    const handler = (record, res) => {
      res.writeHead(200, { "content-type": "text/event-stream" });
      res.end(sse);
    };
    const { observer, port } = await setup(t, handler);
    const response = await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: MESSAGES }),
    });
    assert.equal(
      response.body.toString("utf8"),
      sse,
      "every forwarded SSE byte must be preserved",
    );
    const obs = await collect(observer, 1);
    assertAvailableUsage(obs[0].usage, {
      inclusive: 100,
      hit: 98,
      miss: 2,
      output: 7,
      rate: 0.98,
    });
  },
);

// ---------------------------------------------------------------------------
// 13. Zero inclusive input has no rate denominator
// ---------------------------------------------------------------------------

test(
  "observe reports a null cache_hit_rate for zero inclusive input and zero hits",
  { timeout: 15000 },
  async (t) => {
    const { observer, port } = await setup(
      t,
      jsonReply({
        prompt_tokens: 0,
        completion_tokens: 0,
        prompt_cache_hit_tokens: 0,
        prompt_cache_miss_tokens: 0,
      }),
    );
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    const obs = await collect(observer, 1);
    const usage = obs[0].usage;
    assert.equal(usage?.availability, "available", "zero-token usage is valid and present");
    assert.equal(usage.inclusive_input_tokens, 0);
    assert.equal(usage.cache_hit_tokens, 0);
    assert.equal(usage.cache_miss_tokens, 0);
    assert.equal(usage.cache_hit_rate, null, "a zero denominator must not report a 0% rate");
    assert.equal(usage.fields?.native, true);
  },
);

// ===========================================================================
// `compare` behavior tests
// (plan 20261015-deepseek-cache-diagnostics, Step 2 / REQ-004 / AC-004).
//
// Every test spawns the real CLI with `node <tool> compare` and talks only to
// loopback mocks. No internal function is imported and no external provider is
// contacted. The six credential environment variables are stripped from the
// inherited environment and re-injected as loopback fixtures only, so no real
// key is ever present in a spawned process.
//
// PUBLIC `comparison` output contract (agreed shape returned to Backend):
//
//   comparison event (one JSON line on stdout):
//     {
//       event: "comparison",
//       attempts: number,               // total model POST attempts actually made
//       status: "completed" | "incomplete",
//       comparable: false,              // the tool NEVER self-certifies account equivalence
//       effectiveness: "unverified",   // no external account/log evidence => never "passing"
//       target_met: boolean | null,     // numeric 98% judgment only; null unless completed
//       reason?: string,                // static code, always present when status="incomplete"
//       cost: { availability: "unknown" },   // no external pricing evidence
//       started_at: string,             // ISO 8601
//       completed_at: string,           // ISO 8601, >= started_at
//       series: [
//         {
//           path: "gateway" | "upstream",
//           round: 1 | 2,
//           attempts: number,           // POSTs made for this series (<=10)
//           warmup: coverage,           // first two requests of the series
//           measurement: coverage,      // the remaining eight requests
//         }, ...                        // one entry per reached series
//       ]
//     }
//   coverage = {
//     requests: number,                 // attempts made in this phase (warmup or measurement)
//     valid_usage: number,              // those attempts whose response had valid usage
//     inclusive_input_tokens: number,   // summed inclusive input over valid usages
//     cache_hit_tokens: number,         // summed cache hits over valid usages
//     cache_hit_rate: number | null,    // cache_hit_tokens / inclusive_input_tokens;
//                                       // null when inclusive_input_tokens is 0
//     fields: { native: number, nested: number },
//                                       // valid usages with a native / nested hit source;
//                                       // dual presence counts both, tokens are not doubled
//   }
//
// Endpoint construction appends the chat route to the configured base without
// duplicating a trailing `/v1`, and both `http:` and `https:` base URLs are
// supported; a TLS or transport failure is a structured `transport-failure`.
// A measured response with zero inclusive input has no positive denominator and
// is therefore not cache-eligible: the run stops with `status: "incomplete"`,
// `target_met: null` and the real attempt count instead of a 0% measurement.
//
// Fixed synthetic workload (deterministic, in memory):
//   - a public 256 KiB long prefix,
//   - ten append-only request bodies per series,
//   - every body sets `thinking: { type: "disabled" }` and `max_tokens: 32`,
//   - gateway and upstream, round 1 and round 2, use the same sequence and
//     differ only in the configured `model`,
//   - no model response is fed into a later request.
//
// Budget and stop rule: two rounds x (2 warmup + 8 measurement) per path, run
// sequentially, at most 40 POSTs, no retries and no hidden warmup. Cold warmup
// tokens are reported but excluded from measurement rates. Any non-2xx status,
// transport failure or missing/invalid usage stops the run at that attempt with
// truthful partial coverage, `status: "incomplete"`, `target_met: null` and the
// real attempt count. `target_met` reflects the gateway path's aggregated
// measured rate against 0.98; the upstream path is reference context only.
//
// Static incomplete `reason` codes: `missing-config` (a required credential
// variable is absent), `request-rejected` (non-2xx or rejected options),
// `transport-failure`, `usage-unavailable` (missing usage),
// `usage-invalid` (malformed usage or native/nested disagreement).

const GATEWAY_MODEL = "gateway-fixture-model";
const UPSTREAM_MODEL = "upstream-fixture-model";
const FIXTURE_GATEWAY_KEY = "fixture-gateway-key-not-secret";
const FIXTURE_UPSTREAM_KEY = "fixture-upstream-key-not-secret";
const RESPONSE_MARKER = "SYNTHETIC-COMPLETION-MARKER";

const COMPARE_CREDENTIAL_VARS = [
  "CACHE_DIAG_GATEWAY_BASE_URL",
  "CACHE_DIAG_GATEWAY_MODEL",
  "CACHE_DIAG_GATEWAY_KEY",
  "CACHE_DIAG_UPSTREAM_BASE_URL",
  "CACHE_DIAG_UPSTREAM_MODEL",
  "CACHE_DIAG_UPSTREAM_KEY",
];

const INCOMPLETE_REASONS = new Set([
  "missing-config",
  "request-rejected",
  "transport-failure",
  "usage-unavailable",
  "usage-invalid",
]);

// Each completed series runs two cold warmups (large inclusive input, zero
// hits) before its eight measurements, so including warmup tokens would visibly
// dominate a token-weighted rate.
const WARMUP_USAGE = { inclusive: 5000, hit: 0 };

// Sixteen measured requests per path: eight for round 1 then eight for round 2.
// The gateway fixture uses very unequal prompt sizes so a token-weighted rate
// (2800 / 3400) differs sharply from an average of per-request percentages.
const GATEWAY_BELOW_TARGET_MEASURED = [
  [1000, 1000], [1000, 1000],
  [100, 0], [100, 0], [100, 0], [100, 0], [100, 0], [100, 0],
  [100, 100], [100, 100], [100, 100], [100, 100],
  [100, 100], [100, 100], [100, 100], [100, 100],
];
const GATEWAY_ABOVE_TARGET_MEASURED = Array.from({ length: 16 }, () => [100, 100]);
const UPSTREAM_ALL_MISS_MEASURED = Array.from({ length: 16 }, () => [100, 0]);

const GATEWAY_BELOW_TARGET_RATE = 2800 / 3400;
const WARMUP_INCLUDED_RATE = 2800 / (3400 + 20000);

function compareEnv(gatewayPort, upstreamPort, overrides = {}) {
  const env = { ...process.env };
  for (const name of COMPARE_CREDENTIAL_VARS) delete env[name];
  Object.assign(env, {
    CACHE_DIAG_GATEWAY_BASE_URL: `http://127.0.0.1:${gatewayPort}`,
    CACHE_DIAG_GATEWAY_MODEL: GATEWAY_MODEL,
    CACHE_DIAG_GATEWAY_KEY: FIXTURE_GATEWAY_KEY,
    CACHE_DIAG_UPSTREAM_BASE_URL: `http://127.0.0.1:${upstreamPort}`,
    CACHE_DIAG_UPSTREAM_MODEL: UPSTREAM_MODEL,
    CACHE_DIAG_UPSTREAM_KEY: FIXTURE_UPSTREAM_KEY,
  });
  for (const [name, value] of Object.entries(overrides)) {
    if (value === undefined) delete env[name];
    else env[name] = value;
  }
  return env;
}

// Spawn the real CLI. Resolves on exit so tests assert on the emitted contract;
// a stub that exits immediately satisfies the spawn and fails on assertions
// rather than on a socket timeout.
function runCompare(env, { timeout = 20000 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [TOOL_PATH, "compare"], {
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (data) => {
      stdout += data.toString("utf8");
    });
    child.stderr.on("data", (data) => {
      stderr += data.toString("utf8");
    });
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error("INFRA: compare did not exit before the test timeout"));
    }, timeout);
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(new Error(`INFRA: compare spawn failed: ${error.message}`));
    });
    child.on("exit", (code, signal) => {
      clearTimeout(timer);
      resolve({ code, signal, stdout, stderr, output: stdout + stderr });
    });
  });
}

async function startCompareMocks(t, { gateway, upstream } = {}) {
  const gatewayMock = createMockGateway();
  const upstreamMock = createMockGateway();
  if (gateway) gatewayMock.setHandler(gateway);
  if (upstream) upstreamMock.setHandler(upstream);
  const gatewayPort = await gatewayMock.listen();
  const upstreamPort = await upstreamMock.listen();
  t.after(async () => {
    await gatewayMock.close();
    await upstreamMock.close();
  });
  return { gatewayMock, upstreamMock, gatewayPort, upstreamPort };
}

function parseComparison(stdout) {
  for (const line of stdout.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    let value;
    try {
      value = JSON.parse(trimmed);
    } catch {
      continue;
    }
    if (value !== null && typeof value === "object" && value.event === "comparison") {
      return value;
    }
  }
  return null;
}

function parseBody(record) {
  return JSON.parse(record.body.toString("utf8"));
}

// Map a per-endpoint request index to its series usage, mirroring the fixed
// 2-warmup + 8-measured layout of each ten-request series.
function seriesMeasuredUsage(measured, index) {
  if (index === 1 || index === 2 || index === 11 || index === 12) return WARMUP_USAGE;
  const offset = index <= 10 ? index - 3 : index - 5;
  const [inclusive, hit] = measured[offset];
  return { inclusive, hit };
}

function validCompareHandler(measured) {
  return (record, res, index) => {
    const { inclusive, hit } = seriesMeasuredUsage(measured, index);
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        id: "cmpl-compare",
        object: "chat.completion",
        choices: [
          {
            index: 0,
            message: { role: "assistant", content: RESPONSE_MARKER },
            finish_reason: "stop",
          },
        ],
        usage: {
          prompt_tokens: inclusive,
          completion_tokens: 4,
          prompt_cache_hit_tokens: hit,
          prompt_cache_miss_tokens: inclusive - hit,
        },
      }),
    );
  };
}

// A consistent dual-source response: both the native hit/miss fields and the
// nested OpenAI cache detail are present and equal. Source presence counts
// both, but the tokens are counted once.
function dualCompareHandler(measured) {
  return (record, res, index) => {
    const { inclusive, hit } = seriesMeasuredUsage(measured, index);
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        id: "cmpl-dual",
        object: "chat.completion",
        choices: [
          {
            index: 0,
            message: { role: "assistant", content: RESPONSE_MARKER },
            finish_reason: "stop",
          },
        ],
        usage: {
          prompt_tokens: inclusive,
          completion_tokens: 4,
          prompt_cache_hit_tokens: hit,
          prompt_cache_miss_tokens: inclusive - hit,
          prompt_tokens_details: { cached_tokens: hit },
        },
      }),
    );
  };
}

function aggregateMeasurement(comparison, path) {
  let requests = 0;
  let valid = 0;
  let inclusive = 0;
  let hit = 0;
  for (const series of comparison.series) {
    if (series.path !== path) continue;
    requests += series.measurement.requests;
    valid += series.measurement.valid_usage;
    inclusive += series.measurement.inclusive_input_tokens;
    hit += series.measurement.cache_hit_tokens;
  }
  return {
    requests,
    valid,
    inclusive,
    hit,
    rate: inclusive === 0 ? 0 : hit / inclusive,
  };
}

test(
  "compare makes exactly 40 POSTs, excludes cold warmup and reports token-weighted rates below target",
  { timeout: 20000 },
  async (t) => {
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, {
        gateway: validCompareHandler(GATEWAY_BELOW_TARGET_MEASURED),
        upstream: validCompareHandler(UPSTREAM_ALL_MISS_MEASURED),
      });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(
      json,
      `compare must print a comparison event, stdout=${JSON.stringify(result.stdout)}`,
    );

    const posts = gatewayMock.requests.length + upstreamMock.requests.length;
    assert.equal(posts, 40, "the run must make exactly 40 model POST attempts");
    assert.equal(json.attempts, 40, "the reported attempt count must be 40");
    assert.equal(gatewayMock.requests.length, 20);
    assert.equal(upstreamMock.requests.length, 20);

    assert.equal(result.code, 0);
    assert.equal(json.status, "completed");
    assert.equal(
      json.comparable,
      false,
      "the tool must never self-certify account equivalence",
    );
    assert.equal(json.effectiveness, "unverified");
    assert.equal(json.cost?.availability, "unknown");

    const started = Date.parse(json.started_at);
    const completed = Date.parse(json.completed_at);
    assert.ok(Number.isFinite(started), "started_at must be an ISO timestamp");
    assert.ok(Number.isFinite(completed), "completed_at must be an ISO timestamp");
    assert.ok(started <= completed, "started_at must not be after completed_at");

    assert.ok(Array.isArray(json.series), "series must be an array");
    assert.equal(json.series.length, 4);
    for (const series of json.series) {
      assert.ok(series.path === "gateway" || series.path === "upstream");
      assert.ok(series.round === 1 || series.round === 2);
      assert.equal(series.attempts, 10);
      assert.equal(series.warmup.requests, 2);
      assert.equal(series.warmup.valid_usage, 2);
      assert.equal(series.warmup.inclusive_input_tokens, 10000);
      assert.equal(series.warmup.cache_hit_tokens, 0);
      assert.equal(series.warmup.cache_hit_rate, 0);
      assert.equal(series.measurement.requests, 8);
      assert.equal(series.measurement.valid_usage, 8);
    }

    const gatewayRound1 = json.series.find(
      (series) => series.path === "gateway" && series.round === 1,
    ).measurement;
    assert.equal(gatewayRound1.inclusive_input_tokens, 2600);
    assert.equal(gatewayRound1.cache_hit_tokens, 2000);
    const gatewayRound2 = json.series.find(
      (series) => series.path === "gateway" && series.round === 2,
    ).measurement;
    assert.equal(gatewayRound2.inclusive_input_tokens, 800);
    assert.equal(gatewayRound2.cache_hit_tokens, 800);

    const gateway = aggregateMeasurement(json, "gateway");
    assert.equal(gateway.requests, 16);
    assert.equal(gateway.valid, 16);
    assert.equal(gateway.inclusive, 3400);
    assert.equal(gateway.hit, 2800);
    assert.ok(
      Math.abs(gateway.rate - GATEWAY_BELOW_TARGET_RATE) < 1e-9,
      `gateway token-weighted rate must be ${GATEWAY_BELOW_TARGET_RATE}, got ${gateway.rate}`,
    );
    assert.ok(
      Math.abs(gateway.rate - WARMUP_INCLUDED_RATE) > 0.5,
      "cold warmup tokens must be excluded from the measurement rate",
    );
    assert.equal(json.target_met, false);

    const upstream = aggregateMeasurement(json, "upstream");
    assert.equal(upstream.requests, 16);
    assert.equal(
      upstream.valid,
      16,
      "an explicit zero cache hit is valid usage, not missing usage",
    );
    assert.equal(upstream.inclusive, 1600);
    assert.equal(upstream.hit, 0);
    assert.equal(upstream.rate, 0);
  },
);

test(
  "compare reports target_met true for an above-target measurement while keeping account equivalence unverified",
  { timeout: 20000 },
  async (t) => {
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, {
        gateway: validCompareHandler(GATEWAY_ABOVE_TARGET_MEASURED),
        upstream: validCompareHandler(UPSTREAM_ALL_MISS_MEASURED),
      });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");
    assert.equal(
      gatewayMock.requests.length + upstreamMock.requests.length,
      40,
    );
    assert.equal(json.status, "completed");
    assert.equal(json.target_met, true);
    assert.equal(
      json.comparable,
      false,
      "a met numeric target still does not certify account equivalence",
    );
    assert.equal(json.effectiveness, "unverified");
    assert.equal(json.cost?.availability, "unknown");
    const gateway = aggregateMeasurement(json, "gateway");
    assert.ok(Math.abs(gateway.rate - 1) < 1e-9, "the gateway measurement is 100%");
  },
);

test(
  "compare stops with incomplete coverage and the real attempt count when measured usage is missing",
  { timeout: 20000 },
  async (t) => {
    const noUsage = (record, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          id: "cmpl-nousage",
          object: "chat.completion",
          choices: [
            {
              index: 0,
              message: { role: "assistant", content: "ok" },
              finish_reason: "stop",
            },
          ],
        }),
      );
    };
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, { gateway: noUsage, upstream: noUsage });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");

    const posts = gatewayMock.requests.length + upstreamMock.requests.length;
    assert.equal(posts, 1, "a missing usage stops the run at the first request");
    assert.equal(json.attempts, posts, "the reported count must equal actual POSTs");
    assert.equal(json.attempts, 1);
    assert.equal(json.status, "incomplete");
    assert.equal(json.reason, "usage-unavailable");
    assert.equal(json.target_met, null);
    assert.equal(json.effectiveness, "unverified");
  },
);

test(
  "compare treats native/nested cache disagreement as invalid measured usage and stops",
  { timeout: 20000 },
  async (t) => {
    const disagree = (record, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          id: "cmpl-disagree",
          object: "chat.completion",
          choices: [
            {
              index: 0,
              message: { role: "assistant", content: RESPONSE_MARKER },
              finish_reason: "stop",
            },
          ],
          usage: {
            prompt_tokens: 100,
            completion_tokens: 4,
            prompt_cache_hit_tokens: 98,
            prompt_cache_miss_tokens: 2,
            prompt_tokens_details: { cached_tokens: 90 },
          },
        }),
      );
    };
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, { gateway: disagree, upstream: disagree });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");

    const posts = gatewayMock.requests.length + upstreamMock.requests.length;
    assert.equal(posts, 1, "invalid usage stops the run at the first request");
    assert.equal(json.attempts, posts);
    assert.equal(json.attempts, 1);
    assert.equal(json.status, "incomplete");
    assert.equal(json.reason, "usage-invalid");
    assert.equal(json.target_met, null);
    assert.ok(INCOMPLETE_REASONS.has(json.reason));
  },
);

test(
  "compare stops at a rejected first attempt without retry or extra warmup and never echoes the secret",
  { timeout: 20000 },
  async (t) => {
    const marker = "FIXTURE-COMPARE-SECRET-7c4b";
    const rejectFirst = (record, res, index) => {
      if (index === 1) {
        res.writeHead(401, {
          "content-type": "application/json",
          "x-echo-secret": marker,
        });
        res.end(
          JSON.stringify({
            error: { message: `bad key ${marker}`, authorization: `Bearer ${marker}` },
          }),
        );
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end("{}");
    };
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, { gateway: rejectFirst, upstream: rejectFirst });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");

    const posts = gatewayMock.requests.length + upstreamMock.requests.length;
    assert.equal(posts, 1, "a rejected attempt must not be retried or re-warmed");
    assert.equal(json.attempts, posts);
    assert.equal(json.attempts, 1);
    assert.equal(json.status, "incomplete");
    assert.equal(json.reason, "request-rejected");
    assert.equal(json.target_met, null);
    assert.ok(!result.output.includes(marker), "an echoed upstream error must not leak");
    assert.ok(
      !result.output.includes(FIXTURE_GATEWAY_KEY),
      "the gateway credential must not leak",
    );
    assert.ok(
      !result.output.includes(FIXTURE_UPSTREAM_KEY),
      "the upstream credential must not leak",
    );
    assert.ok(result.output.length < 8192, "output must be bounded metadata");
  },
);

test(
  "compare stops at a transport failure without retry",
  { timeout: 20000 },
  async (t) => {
    const destroyFirst = (record, res, index) => {
      if (index === 1) {
        res.destroy();
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end("{}");
    };
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, { gateway: destroyFirst, upstream: destroyFirst });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");

    const posts = gatewayMock.requests.length + upstreamMock.requests.length;
    assert.equal(posts, 1, "a transport failure must not be retried");
    assert.equal(json.attempts, posts);
    assert.equal(json.attempts, 1);
    assert.equal(json.status, "incomplete");
    assert.equal(json.reason, "transport-failure");
    assert.equal(json.target_met, null);
  },
);

test(
  "compare reports a static missing-config incomplete result with zero attempts when any required env var is absent",
  { timeout: 30000 },
  async (t) => {
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t);
    const complete = compareEnv(gatewayPort, upstreamPort);

    for (const name of COMPARE_CREDENTIAL_VARS) {
      const env = { ...complete };
      delete env[name];
      const result = await runCompare(env);
      const json = parseComparison(result.stdout);
      assert.ok(json, `a missing ${name} must still print a comparison event`);
      assert.equal(json.attempts, 0, `a missing ${name} must make no attempt`);
      assert.equal(
        gatewayMock.requests.length + upstreamMock.requests.length,
        0,
        "missing configuration must issue zero POSTs",
      );
      assert.equal(json.status, "incomplete");
      assert.equal(json.reason, "missing-config");
      assert.equal(json.target_met, null);
      assert.notEqual(result.code, 0, `a missing ${name} must exit nonzero`);
      assert.ok(
        !result.output.includes(FIXTURE_GATEWAY_KEY) &&
          !result.output.includes(FIXTURE_UPSTREAM_KEY),
        "a missing-config report must not leak credentials",
      );
    }
  },
);

test(
  "compare builds one deterministic 256 KiB append-only fixture per series differing only by model",
  { timeout: 20000 },
  async (t) => {
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, {
        gateway: validCompareHandler(GATEWAY_ABOVE_TARGET_MEASURED),
        upstream: validCompareHandler(UPSTREAM_ALL_MISS_MEASURED),
      });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");

    assert.equal(gatewayMock.requests.length, 20, "the gateway path runs two series");
    assert.equal(upstreamMock.requests.length, 20, "the upstream path runs two series");

    const seriesList = [
      {
        path: "gateway",
        model: GATEWAY_MODEL,
        bodies: gatewayMock.requests.slice(0, 10).map(parseBody),
      },
      {
        path: "gateway",
        model: GATEWAY_MODEL,
        bodies: gatewayMock.requests.slice(10, 20).map(parseBody),
      },
      {
        path: "upstream",
        model: UPSTREAM_MODEL,
        bodies: upstreamMock.requests.slice(0, 10).map(parseBody),
      },
      {
        path: "upstream",
        model: UPSTREAM_MODEL,
        bodies: upstreamMock.requests.slice(10, 20).map(parseBody),
      },
    ];

    for (const series of seriesList) {
      assert.equal(series.bodies.length, 10);
      for (let i = 0; i < series.bodies.length - 1; i += 1) {
        const before = series.bodies[i].messages;
        const after = series.bodies[i + 1].messages;
        assert.ok(Array.isArray(before) && Array.isArray(after));
        assert.ok(
          after.length >= before.length,
          "each request must append to, never rewrite, the message history",
        );
        for (let m = 0; m < before.length; m += 1) {
          assert.deepEqual(after[m], before[m], "earlier messages must be unchanged");
        }
      }
      for (const body of series.bodies) {
        assert.equal(body.model, series.model);
        assert.equal(body.thinking?.type, "disabled");
        assert.equal(body.max_tokens, 32);
        assert.ok(
          !JSON.stringify(body).includes(RESPONSE_MARKER),
          "no model response may be fed into a later request",
        );
      }
      const totalContent = series.bodies[0].messages.reduce(
        (sum, message) =>
          sum + (typeof message.content === "string" ? message.content.length : 0),
        0,
      );
      assert.ok(
        totalContent >= 256 * 1024,
        `the public long prefix must be at least 256 KiB, got ${totalContent}`,
      );
    }

    const normalize = (body) => {
      const copy = JSON.parse(JSON.stringify(body));
      copy.model = "<model>";
      return JSON.stringify(copy);
    };
    const reference = seriesList[0].bodies.map(normalize);
    for (const series of seriesList) {
      assert.deepEqual(
        series.bodies.map(normalize),
        reference,
        "fixtures must be identical across paths and rounds except the model",
      );
    }
  },
);

test(
  "compare appends the chat endpoint to a base URL that already includes /v1 without doubling the version segment",
  { timeout: 20000 },
  async (t) => {
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, {
        gateway: validCompareHandler(GATEWAY_ABOVE_TARGET_MEASURED),
        upstream: validCompareHandler(UPSTREAM_ALL_MISS_MEASURED),
      });
    const result = await runCompare(
      compareEnv(gatewayPort, upstreamPort, {
        CACHE_DIAG_GATEWAY_BASE_URL: `http://127.0.0.1:${gatewayPort}/provider/v1`,
        CACHE_DIAG_UPSTREAM_BASE_URL: `http://127.0.0.1:${upstreamPort}/v1`,
      }),
    );
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");
    assert.equal(
      gatewayMock.requests.length + upstreamMock.requests.length,
      40,
      "the 40-attempt budget must still hold",
    );
    assert.equal(json.attempts, 40);
    assert.deepEqual(
      [...new Set(gatewayMock.requests.map((record) => record.url))],
      ["/provider/v1/chat/completions"],
      "a base URL that already includes /provider/v1 must not double the version segment",
    );
    assert.deepEqual(
      [...new Set(upstreamMock.requests.map((record) => record.url))],
      ["/v1/chat/completions"],
      "a base URL that already includes /v1 must not double the version segment",
    );
  },
);

test(
  "compare reports a structured transport-failure for an https endpoint whose TLS handshake fails without crashing or leaking",
  { timeout: 20000 },
  async (t) => {
    const marker = "FIXTURE-TLS-SECRET-2a91";
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, {
        gateway: validCompareHandler(GATEWAY_ABOVE_TARGET_MEASURED),
        upstream: validCompareHandler(UPSTREAM_ALL_MISS_MEASURED),
      });
    const result = await runCompare(
      compareEnv(gatewayPort, upstreamPort, {
        CACHE_DIAG_GATEWAY_BASE_URL: `https://127.0.0.1:${gatewayPort}`,
        CACHE_DIAG_GATEWAY_KEY: marker,
      }),
    );
    const json = parseComparison(result.stdout);
    assert.ok(
      json,
      `an https transport failure must still print a comparison event, output=${JSON.stringify(result.output)}`,
    );
    assert.equal(json.status, "incomplete");
    assert.equal(json.reason, "transport-failure");
    assert.equal(json.attempts, 1, "the failed attempt must be counted once and not retried");
    assert.equal(json.target_met, null);
    assert.equal(
      gatewayMock.requests.length,
      0,
      "a failed TLS handshake must not reach the mock as plaintext",
    );
    assert.ok(!result.output.includes(marker), "the credential must not leak");
    assert.ok(!result.output.includes("https://"), "the raw endpoint URL must not leak");
    assert.ok(
      !/at .+\(.+:\d+:\d+\)/.test(result.output),
      "a raw stack trace must not be printed",
    );
    assert.ok(result.output.length < 8192, "output must be bounded metadata");
  },
);

test(
  "compare counts failed and invalid attempts in their warmup and measurement coverage",
  { timeout: 30000 },
  async (t) => {
    const noUsage = (record, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          id: "cmpl-nousage",
          object: "chat.completion",
          choices: [
            {
              index: 0,
              message: { role: "assistant", content: "ok" },
              finish_reason: "stop",
            },
          ],
        }),
      );
    };
    {
      const { gatewayPort, upstreamPort } = await startCompareMocks(t, {
        gateway: noUsage,
        upstream: noUsage,
      });
      const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
      const json = parseComparison(result.stdout);
      assert.ok(json, "compare must print a comparison event");
      assert.equal(json.attempts, 1);
      const series = json.series.find(
        (entry) => entry.path === "gateway" && entry.round === 1,
      );
      assert.ok(series, "the failed first series must still be reported");
      assert.equal(series.attempts, 1);
      assert.equal(
        series.warmup.requests,
        1,
        "a failed warmup request must still be counted in warmup coverage",
      );
      assert.equal(series.warmup.valid_usage, 0);
      assert.equal(series.measurement.requests, 0);
      assert.equal(series.measurement.valid_usage, 0);
    }

    const invalidAtThree = (record, res, index) => {
      if (index < 3) {
        return validCompareHandler(GATEWAY_ABOVE_TARGET_MEASURED)(record, res, index);
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          id: "cmpl-invalid",
          object: "chat.completion",
          choices: [
            {
              index: 0,
              message: { role: "assistant", content: RESPONSE_MARKER },
              finish_reason: "stop",
            },
          ],
          usage: { prompt_tokens: 100, prompt_cache_hit_tokens: "abc" },
        }),
      );
    };
    {
      const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
        await startCompareMocks(t, {
          gateway: invalidAtThree,
          upstream: invalidAtThree,
        });
      const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
      const json = parseComparison(result.stdout);
      assert.ok(json, "compare must print a comparison event");
      assert.equal(json.attempts, 3);
      assert.equal(json.status, "incomplete");
      assert.equal(json.reason, "usage-invalid");
      assert.equal(json.target_met, null);
      const series = json.series.find(
        (entry) => entry.path === "gateway" && entry.round === 1,
      );
      assert.ok(series, "the failed series must still be reported");
      assert.equal(series.attempts, 3);
      assert.equal(series.warmup.requests, 2);
      assert.equal(series.warmup.valid_usage, 2);
      assert.equal(
        series.measurement.requests,
        1,
        "an invalid measurement attempt must still be counted in measurement coverage",
      );
      assert.equal(series.measurement.valid_usage, 0);
      assert.equal(
        series.measurement.cache_hit_rate,
        null,
        "a zero denominator must not report a 0% rate",
      );
      assert.equal(gatewayMock.requests.length, 3);
      assert.equal(upstreamMock.requests.length, 0, "the run must stop before the upstream path");
    }
  },
);

test(
  "compare stops as incomplete when a measured response has no positive cache denominator",
  { timeout: 20000 },
  async (t) => {
    const handler = (record, res, index) => {
      if (index < 3) {
        return validCompareHandler(GATEWAY_ABOVE_TARGET_MEASURED)(record, res, index);
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(
        JSON.stringify({
          id: "cmpl-zero",
          object: "chat.completion",
          choices: [
            {
              index: 0,
              message: { role: "assistant", content: RESPONSE_MARKER },
              finish_reason: "stop",
            },
          ],
          usage: {
            prompt_tokens: 0,
            completion_tokens: 0,
            prompt_cache_hit_tokens: 0,
            prompt_cache_miss_tokens: 0,
          },
        }),
      );
    };
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, { gateway: handler, upstream: handler });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");
    assert.equal(
      json.status,
      "incomplete",
      "a zero-denominator measurement is not a completed 0% measurement",
    );
    assert.equal(
      json.target_met,
      null,
      "an ineligible measurement must not report a target verdict",
    );
    assert.ok(
      typeof json.reason === "string" && json.reason.length > 0,
      "an incomplete report must carry a static reason",
    );
    assert.equal(
      json.attempts,
      3,
      "the ineligible measurement attempt is counted and no later request is made",
    );
    assert.equal(
      gatewayMock.requests.length,
      3,
      "no further gateway call is made after an ineligible measurement",
    );
    assert.equal(upstreamMock.requests.length, 0, "the upstream path must not run");
  },
);

test(
  "compare reports per-coverage native and nested source presence without double counting tokens",
  { timeout: 20000 },
  async (t) => {
    const { gatewayMock, upstreamMock, gatewayPort, upstreamPort } =
      await startCompareMocks(t, {
        gateway: dualCompareHandler(GATEWAY_ABOVE_TARGET_MEASURED),
        upstream: validCompareHandler(UPSTREAM_ALL_MISS_MEASURED),
      });
    const result = await runCompare(compareEnv(gatewayPort, upstreamPort));
    const json = parseComparison(result.stdout);
    assert.ok(json, "compare must print a comparison event");
    assert.equal(json.status, "completed");
    assert.equal(
      gatewayMock.requests.length + upstreamMock.requests.length,
      40,
    );

    for (const series of json.series.filter((entry) => entry.path === "gateway")) {
      assert.equal(series.warmup.valid_usage, 2);
      assert.equal(
        series.warmup.fields?.native,
        2,
        "native presence is counted per valid warmup usage",
      );
      assert.equal(
        series.warmup.fields?.nested,
        2,
        "nested presence is counted per valid warmup usage",
      );
      assert.equal(series.measurement.valid_usage, 8);
      assert.equal(series.measurement.fields?.native, 8);
      assert.equal(series.measurement.fields?.nested, 8);
    }

    const gatewayRound1 = json.series.find(
      (entry) => entry.path === "gateway" && entry.round === 1,
    ).measurement;
    assert.equal(
      gatewayRound1.inclusive_input_tokens,
      800,
      "dual source presence must not double-count inclusive tokens",
    );
    assert.equal(
      gatewayRound1.cache_hit_tokens,
      800,
      "a consistent dual hit is counted once",
    );

    const upstreamRound1 = json.series.find(
      (entry) => entry.path === "upstream" && entry.round === 1,
    ).measurement;
    assert.equal(upstreamRound1.fields?.native, 8, "a native-only series reports native presence");
    assert.equal(
      upstreamRound1.fields?.nested,
      0,
      "a native-only series reports no nested presence",
    );
  },
);

// ===========================================================================
// 14. Formal-review regressions: HTTPS gateway construction, split CRLF event
//     boundaries and late-body overlap.
//
// Finding A / SPEC-001 / STD-001: `observe` accepts an `https:` `--gateway`
//   URL, but `handleRequest` builds the upstream request with the `http` module
//   regardless of protocol. `http.request({ protocol: "https:" })` throws
//   `ERR_INVALID_PROTOCOL` synchronously inside the request listener, so the
//   observer process crashes and drops the client instead of answering 502.
// Finding B / SPEC-002: the SSE feeder normalizes `\r\n` to `\n` per chunk, so
//   an event boundary whose `\r\n` is split across two chunks (`...\r\n\r` +
//   `\n...`) never collapses to `\n\n`; the usage event and `[DONE]` merge into
//   one unparsable event and the usage is lost.
// Finding C / SPEC-003: a selected request that starts while an earlier
//   selected response is still held is only detected once its own body ends,
//   so a genuinely overlapping pair is misreported as a sequential chain.
// ===========================================================================

// Spawn the real `observe` CLI against a gateway URL derived from the mock's
// port, so an `https:` target can be exercised without a certificate or any
// extra dependency.
async function setupGateway(t, handler, gatewayUrlFor) {
  const mock = createMockGateway();
  if (handler) mock.setHandler(handler);
  const gatewayPort = await mock.listen();
  const observer = startObserver(gatewayPort, {
    gatewayUrl: gatewayUrlFor(gatewayPort),
  });
  t.after(async () => {
    await observer.stop();
    await mock.close();
  });
  const listening = await observer.waitListening();
  return { mock, observer, port: listening.port, gatewayPort };
}

test(
  "observe answers HTTP 502 for an https gateway whose TLS handshake fails instead of crashing or dropping the client",
  { timeout: 15000 },
  async (t) => {
    // The mock is plain HTTP; pointing the observer at it as `https://` makes
    // its real TLS handshake fail. The observer must surface a bounded 502, not
    // throw an uncaught protocol error and die.
    const { observer, port } = await setupGateway(
      t,
      jsonReply(VALID_NATIVE_USAGE),
      (gatewayPort) => `https://127.0.0.1:${gatewayPort}`,
    );

    let firstOutcome;
    try {
      const response = await sendJson(port, {
        session: "S1",
        body: chatBody({ messages: MESSAGES }),
      });
      firstOutcome = {
        kind: "response",
        status: response.status,
        body: response.body.toString("utf8"),
      };
    } catch (error) {
      firstOutcome = { kind: "error", message: String(error && error.message) };
    }
    assert.equal(
      firstOutcome.kind,
      "response",
      `the observer must answer the client with HTTP 502, not drop the socket or crash: ${JSON.stringify(firstOutcome)}`,
    );
    assert.equal(firstOutcome.status, 502, "a failed TLS handshake is a bad gateway");
    assert.ok(
      !/at .+\(.+:\d+:\d+\)/.test(firstOutcome.body),
      "no raw stack trace may be forwarded to the client",
    );

    // The process must survive a gateway construction/handshake failure.
    await delay(150);
    assert.equal(
      observer.child.exitCode,
      null,
      "the observer process must not exit on a TLS handshake failure",
    );
    assert.equal(
      observer.child.signalCode,
      null,
      "the observer process must not be killed on a TLS handshake failure",
    );
    assert.ok(
      !/at .+\(.+:\d+:\d+\)/.test(observer.stderrText()),
      `no raw stack trace may be printed, stderr=${JSON.stringify(observer.stderrText())}`,
    );

    // The listener must still accept and serve the next connection.
    const second = await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    assert.equal(second.status, 502, "the surviving listener must still answer");

    // SIGTERM must release the listener cleanly.
    observer.child.kill("SIGTERM");
    await withTimeout(once(observer.child, "exit"), 3000, "observer SIGTERM exit");
    assert.equal(
      await connectRefused(port),
      true,
      "SIGTERM must release the loopback listener",
    );
  },
);

test(
  "observe reads SSE usage when a CRLF event boundary is split across chunks and still forwards every byte",
  { timeout: 15000 },
  async (t) => {
    // The separating `\r\n\r\n` is split as `\r\n\r` | `\n`: the first write
    // ends mid-CRLF and the second begins with the final `\n`. Per-chunk `\r\n`
    // normalization leaves `\n\r\n` between the two events instead of `\n\n`,
    // so the usage event and `[DONE]` merge into one unparsable event and the
    // usage is lost. There is exactly one case; the split constants are not
    // enumerated.
    const first = 'data: {"id":"c1","choices":[],"usage":{"prompt_tokens":100,"completion_tokens":7,"prompt_cache_hit_tokens":98,"prompt_cache_miss_tokens":2}}\r\n\r';
    const second = "\ndata: [DONE]\r\n\r\n";
    const sse = first + second;
    const handler = async (record, res, index) => {
      if (index === 2) {
        res.writeHead(200, { "content-type": "text/event-stream" });
        res.write(first);
        await delay(30);
        res.end(second);
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(chatResponse(VALID_NATIVE_USAGE));
    };
    const { observer, port } = await setup(t, handler);

    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    const response = await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    assert.equal(
      response.body.toString("utf8"),
      sse,
      "the split SSE bytes must be forwarded unchanged",
    );

    const obs = await collect(observer, 2);
    assert.equal(obs[1].comparison, "append-only");
    assertAvailableUsage(obs[1].usage, {
      inclusive: 100,
      hit: 98,
      miss: 2,
      output: 7,
      rate: 0.98,
    });
  },
);

// Opens a Chat request whose body is written in two parts: `first` is flushed
// immediately and the remainder is sent by the caller later, so the request is
// genuinely in flight (headers and a partial body) before `rest` is sent.
function openDeferredChat(port, { session, bodyText, splitAt }) {
  const headers = chatHeaders({ session });
  headers["content-length"] = Buffer.byteLength(bodyText);
  const req = http.request({
    host: "127.0.0.1",
    port,
    method: "POST",
    path: CHAT_PATH,
    headers,
    agent: false,
  });
  const response = new Promise((resolve, reject) => {
    req.on("response", (res) => {
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () =>
        resolve({
          status: res.statusCode,
          headers: res.headers,
          body: Buffer.concat(chunks),
        }),
      );
    });
    req.on("error", reject);
  });
  const bytes = Buffer.from(bodyText);
  return {
    req,
    response,
    first: bytes.subarray(0, splitAt),
    rest: bytes.subarray(splitAt),
  };
}

test(
  "observe marks a selected request that starts while an earlier held response is in flight as overlap",
  { timeout: 15000 },
  async (t) => {
    let releaseHeld;
    const heldGate = new Promise((resolve) => {
      releaseHeld = resolve;
    });
    const handler = async (record, res, index) => {
      if (index === 2) await heldGate; // A: full body upstream, response held
      res.writeHead(200, { "content-type": "application/json" });
      res.end(chatResponse(VALID_NATIVE_USAGE));
    };
    const { mock, observer, port } = await setup(t, handler);

    // Track upstream request headers so B's arrival at the observer is proven
    // deterministically rather than by a fixed sleep.
    let upstreamRequests = 0;
    mock.server.on("request", () => {
      upstreamRequests += 1;
    });

    // P: a prior completed selected request establishes a predecessor.
    await sendJson(port, { session: "S1", body: chatBody({ messages: MESSAGES }) });
    await waitFor(() => mock.requests.length >= 1, { label: "predecessor upstream request" });

    // A: full body reaches upstream; its response is withheld.
    const a = sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(MESSAGES) }),
    });
    await waitFor(() => mock.requests.length >= 2, { label: "held upstream request" });

    // B: starts while A is still active, sending headers and only a partial
    // JSON body. Once the mock parses B's headers the overlap visibly exists,
    // even though B's body has not ended.
    const bBody = JSON.stringify(chatBody({ messages: appended(appended(MESSAGES)) }));
    const b = openDeferredChat(port, { session: "S1", bodyText: bBody, splitAt: 12 });
    await new Promise((resolve) => b.req.write(b.first, resolve));
    await waitFor(() => upstreamRequests >= 3, {
      label: "B upstream request headers while A is held",
    });

    // A completes first; only then is B's remaining body sent.
    releaseHeld();
    const aResponse = await a;
    await delay(40);
    b.req.end(b.rest);
    const bResponse = await b.response;

    // C: the next fully sequential request has no known predecessor.
    await sendJson(port, {
      session: "S1",
      body: chatBody({ messages: appended(appended(appended(MESSAGES))) }),
    });

    const obs = await collect(observer, 4);
    assert.equal(obs.length, 4);
    assert.equal(obs[0].comparison, "unavailable");
    assert.equal(obs[0].reason, "no-predecessor");
    for (const overlapped of [obs[1], obs[2]]) {
      assert.equal(
        overlapped.comparison,
        "unavailable",
        "a request overlapping a held response must be comparison-unavailable",
      );
      assert.equal(
        overlapped.reason,
        "overlap",
        "both overlapping requests must be marked overlap, not compared as a chain",
      );
    }
    assert.equal(
      obs[3].comparison,
      "unavailable",
      "the next sequential request must not invent a predecessor from an overlap",
    );
    assert.equal(obs[3].reason, "no-predecessor");
    assert.equal(
      obs.filter((event) => event.comparison === "append-only").length,
      0,
      "an overlapping pair must never be reported as a sequential append-only chain",
    );

    // Transport bytes are still preserved for the overlapping pair.
    assert.equal(aResponse.body.toString("utf8"), chatResponse(VALID_NATIVE_USAGE));
    assert.equal(bResponse.body.toString("utf8"), chatResponse(VALID_NATIVE_USAGE));
  },
);


