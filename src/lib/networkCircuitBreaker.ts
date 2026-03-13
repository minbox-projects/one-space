export const NETWORK_CIRCUIT_EVENT = 'onespace-network-circuit-open';
export const NETWORK_CIRCUIT_MESSAGE = '网络异常，请检查网络配置或设置系统代理';
const NETWORK_TIMEOUT_MS = 10_000;
const INSTALL_FLAG = '__ONESPACE_NETWORK_CIRCUIT_INSTALLED__';
const LOG_PREFIX = '[network-circuit-breaker]';

type TauriInternals = {
  invoke?: (cmd: string, args?: unknown, options?: unknown) => Promise<unknown>;
};

declare global {
  interface Window {
    __TAURI_INTERNALS__?: TauriInternals;
    [INSTALL_FLAG]?: boolean;
  }
}

function emitNetworkCircuit(kind: 'fetch' | 'invoke', target: string) {
  window.dispatchEvent(
    new CustomEvent(NETWORK_CIRCUIT_EVENT, {
      detail: {
        kind,
        target,
        timeoutMs: NETWORK_TIMEOUT_MS,
        message: NETWORK_CIRCUIT_MESSAGE,
        ts: Date.now(),
      },
    }),
  );
}

function createTimeoutError(target: string) {
  const error = new Error(`network timeout after ${NETWORK_TIMEOUT_MS}ms: ${target}`);
  (error as Error & { name: string }).name = 'NetworkTimeoutError';
  return error;
}

function shouldTimeoutInvoke(cmd: string): boolean {
  return (
    cmd === 'proxy_http_request' ||
    cmd === 'refresh_google_token' ||
    cmd === 'sync_run_now' ||
    cmd === 'ai_news_sync_now' ||
    cmd === 'skills_sync_now' ||
    cmd === 'subagents_sync_now' ||
    cmd === 'test_proxy_connection' ||
    cmd === 'plugin:updater|check'
  );
}

function toFetchTarget(input: RequestInfo | URL): string {
  if (typeof input === 'string') return input;
  if (input instanceof URL) return input.toString();
  if (typeof Request !== 'undefined' && input instanceof Request) return input.url;
  return String(input);
}

function linkAbortSignal(signal: AbortSignal | null | undefined, controller: AbortController): () => void {
  if (!signal) return () => {};
  if (signal.aborted) {
    controller.abort((signal as AbortSignal & { reason?: unknown }).reason);
    return () => {};
  }
  const onAbort = () => {
    controller.abort((signal as AbortSignal & { reason?: unknown }).reason);
  };
  signal.addEventListener('abort', onAbort, { once: true });
  return () => signal.removeEventListener('abort', onAbort);
}

function installFetchTimeoutPatch() {
  const originalFetch = window.fetch.bind(window);
  const wrappedFetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const target = toFetchTarget(input);
    const controller = new AbortController();
    const requestSignal =
      typeof Request !== 'undefined' && input instanceof Request ? input.signal : undefined;
    const unlink = linkAbortSignal(init?.signal ?? requestSignal, controller);
    let timedOut = false;

    const timeoutId = window.setTimeout(() => {
      timedOut = true;
      controller.abort();
      emitNetworkCircuit('fetch', target);
    }, NETWORK_TIMEOUT_MS);

    try {
      return await originalFetch(input, {
        ...init,
        signal: controller.signal,
      });
    } catch (error) {
      if (timedOut) {
        throw createTimeoutError(target);
      }
      throw error;
    } finally {
      unlink();
      window.clearTimeout(timeoutId);
    }
  };
  try {
    window.fetch = wrappedFetch;
  } catch (error) {
    console.warn(`${LOG_PREFIX} failed to patch window.fetch`, error);
  }
}

function installInvokeTimeoutPatch() {
  const internals = window.__TAURI_INTERNALS__;
  if (!internals || typeof internals.invoke !== 'function') return;
  const originalInvoke = internals.invoke.bind(internals);

  const wrappedInvoke = (cmd: string, args?: unknown, options?: unknown) => {
    if (!shouldTimeoutInvoke(cmd)) {
      return originalInvoke(cmd, args, options);
    }
    return new Promise((resolve, reject) => {
      let settled = false;
      const timeoutId = window.setTimeout(() => {
        if (settled) return;
        settled = true;
        emitNetworkCircuit('invoke', cmd);
        reject(createTimeoutError(cmd));
      }, NETWORK_TIMEOUT_MS);

      originalInvoke(cmd, args, options)
        .then((result) => {
          if (settled) return;
          settled = true;
          window.clearTimeout(timeoutId);
          resolve(result);
        })
        .catch((error) => {
          if (settled) return;
          settled = true;
          window.clearTimeout(timeoutId);
          reject(error);
        });
    });
  };
  try {
    internals.invoke = wrappedInvoke;
  } catch (error) {
    console.warn(`${LOG_PREFIX} failed to patch tauri invoke`, error);
  }
}

export function installNetworkCircuitBreaker() {
  if ((window as Window)[INSTALL_FLAG]) return;
  (window as Window)[INSTALL_FLAG] = true;
  try {
    installFetchTimeoutPatch();
    installInvokeTimeoutPatch();
  } catch (error) {
    console.warn(`${LOG_PREFIX} install failed`, error);
  }
}
