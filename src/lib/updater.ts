import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { relaunch } from '@tauri-apps/plugin-process';

type UpdaterStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'installing' | 'error';
type UpdaterSource = 'tauri-updater' | 'github-fallback' | null;
type UpdaterErrorCode = 'EndpointInvalid' | 'NetworkError' | 'RateLimitExceeded' | 'SignatureOrInstallError' | 'Unknown' | null;

export interface UpdateManifest {
  version: string;
  currentVersion?: string;
  date?: string;
  body?: string;
}

export interface CurrentReleaseInfo {
  version: string;
  body: string;
  publishedAt?: string;
  htmlUrl?: string;
}

interface UpdaterState {
  status: UpdaterStatus;
  checking: boolean;
  updateAvailable: boolean;
  installable: boolean;
  source: UpdaterSource;
  error: string | null;
  errorCode: UpdaterErrorCode;
  notice: string | null;
  manifest: UpdateManifest | null;
  downloadProgress: number;
  lastCheckedAt: number | null;
}

interface NativeUpdateMetadata {
  id: string;
  version: string;
  currentVersion: string;
  date?: string | null;
  body?: string | null;
}

type NativeDownloadEvent =
  | { event: 'Started'; data?: { contentLength?: number } }
  | { event: 'Progress'; data?: { chunkLength?: number } }
  | { event: 'Finished' };

interface PendingUpdate {
  id: string;
  manifest: UpdateManifest;
}

const GITHUB_REPO = 'minbox-projects/one-space';
const UPDATE_CHECK_THROTTLE_MS = 30_000;
const APP_UPDATER_DOWNLOAD_EVENT = 'app-updater-download-event';

let pendingUpdate: PendingUpdate | null = null;
let inFlightCheckPromise: Promise<UpdateManifest | null> | null = null;
let checkEpoch = 0;
let downloadListener: Promise<() => void> | null = null;
let downloadedBytes = 0;
let totalBytes = 0;
const subscribers = new Set<(next: UpdaterState) => void>();

let state: UpdaterState = {
  status: 'idle',
  checking: false,
  updateAvailable: false,
  installable: false,
  source: null,
  error: null,
  errorCode: null,
  notice: null,
  manifest: null,
  downloadProgress: 0,
  lastCheckedAt: null,
};

function emit(next: Partial<UpdaterState>) {
  state = { ...state, ...next };
  for (const sub of subscribers) sub(state);
}

export function normalizeVersion(version: string): string {
  return version.trim().replace(/^v/i, '');
}

export function parseVersion(version: string): [number, number, number] {
  const parts = normalizeVersion(version).split('.').map((p) => parseInt(p, 10) || 0);
  return [parts[0] || 0, parts[1] || 0, parts[2] || 0];
}

export function isVersionGreater(a: string, b: string): boolean {
  const pa = parseVersion(a);
  const pb = parseVersion(b);
  for (let i = 0; i < 3; i += 1) {
    if (pa[i] > pb[i]) return true;
    if (pa[i] < pb[i]) return false;
  }
  return false;
}

function classifyError(error: unknown): UpdaterErrorCode {
  const text = String(error ?? '').toLowerCase();
  if (
    text.includes('rate limit') ||
    text.includes('403') ||
    text.includes('secondary rate')
  ) {
    return 'RateLimitExceeded';
  }
  if (
    text.includes('valid release json') ||
    text.includes('latest.json') ||
    text.includes('404') ||
    text.includes('not found')
  ) {
    return 'EndpointInvalid';
  }
  if (
    text.includes('network') ||
    text.includes('dns') ||
    text.includes('timed out') ||
    text.includes('connection')
  ) {
    return 'NetworkError';
  }
  if (
    text.includes('signature') ||
    text.includes('install') ||
    text.includes('relaunch') ||
    text.includes('download')
  ) {
    return 'SignatureOrInstallError';
  }
  return 'Unknown';
}

function httpStatusFromError(error: unknown): number | null {
  const match = String(error ?? '').match(/HTTP\s+(\d{3})/i);
  return match ? Number(match[1]) : null;
}

async function fetchGithubJson(path: string): Promise<unknown> {
  const text = await invoke<string>('proxy_http_request', {
    url: `https://api.github.com/repos/${GITHUB_REPO}${path}`,
    method: 'GET',
    headers: { Accept: 'application/vnd.github+json' },
    body: null,
  });
  return JSON.parse(text);
}

export async function fetchCurrentVersionReleaseNotes(
  version: string,
  force = false,
): Promise<CurrentReleaseInfo | null> {
  const norm = normalizeVersion(version);
  if (!norm) return null;
  const cacheKey = `onespace_release_notes_${norm}`;

  if (!force && typeof window !== 'undefined' && window.localStorage) {
    try {
      const cached = localStorage.getItem(cacheKey);
      if (cached) {
        const parsed = JSON.parse(cached) as CurrentReleaseInfo;
        if (parsed && parsed.body) {
          return parsed;
        }
      }
    } catch {
      // Ignore localStorage errors
    }
  }

  const fetchTag = async (tag: string) => {
    try {
      return await fetchGithubJson(`/releases/tags/${tag}`);
    } catch (error) {
      const status = httpStatusFromError(error);
      if (status !== null) {
        throw new Error(`HTTP ${status}`);
      }
      throw error;
    }
  };

  try {
    let json: { body?: string; published_at?: string; created_at?: string; html_url?: string };
    try {
      json = (await fetchTag(`v${norm}`)) as typeof json;
    } catch (e: unknown) {
      if (e instanceof Error && e.message.includes('404')) {
        json = (await fetchTag(norm)) as typeof json;
      } else {
        throw e;
      }
    }

    const info: CurrentReleaseInfo = {
      version: norm,
      body: (json.body || '').trim(),
      publishedAt: json.published_at || json.created_at,
      htmlUrl: json.html_url || `https://github.com/${GITHUB_REPO}/releases/tag/v${norm}`,
    };

    if (typeof window !== 'undefined' && window.localStorage && info.body) {
      try {
        localStorage.setItem(cacheKey, JSON.stringify(info));
      } catch {
        // Ignore cache storage error
      }
    }

    return info;
  } catch (err) {
    // If request failed, fall back to cached version if any
    if (typeof window !== 'undefined' && window.localStorage) {
      try {
        const cached = localStorage.getItem(cacheKey);
        if (cached) {
          return JSON.parse(cached) as CurrentReleaseInfo;
        }
      } catch {
        // Ignore
      }
    }
    console.warn(`Failed to fetch release notes for v${norm}:`, err);
    return null;
  }
}

async function checkViaGithubFallback(): Promise<UpdateManifest | null> {
  const currentVersion = normalizeVersion(await getVersion());
  let json: { tag_name?: string; name?: string; body?: string; published_at?: string; created_at?: string };
  try {
    json = (await fetchGithubJson('/releases/latest')) as typeof json;
  } catch (error) {
    const status = httpStatusFromError(error);
    if (status !== null) {
      throw new Error(`GitHub API HTTP ${status}`);
    }
    throw error;
  }
  const latestVersion = normalizeVersion(json.tag_name || json.name || '');
  if (!latestVersion) {
    throw new Error('GitHub fallback returned empty version');
  }
  if (!isVersionGreater(latestVersion, currentVersion)) {
    return null;
  }
  return {
    version: latestVersion,
    currentVersion,
    body: json.body || '',
    date: json.published_at || json.created_at,
  };
}

function handleDownloadEvent(event: NativeDownloadEvent) {
  if (event.event === 'Started') {
    totalBytes = event.data?.contentLength || 0;
  } else if (event.event === 'Progress') {
    downloadedBytes += event.data?.chunkLength ?? 0;
    const pct = totalBytes > 0 ? Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)) : 0;
    emit({ downloadProgress: pct });
  } else if (event.event === 'Finished') {
    emit({ downloadProgress: 100 });
  }
}

function ensureDownloadListener(): Promise<() => void> {
  if (!downloadListener) {
    downloadListener = listen<NativeDownloadEvent>(APP_UPDATER_DOWNLOAD_EVENT, (event) => {
      if (event.payload) {
        handleDownloadEvent(event.payload);
      }
    }).catch((error) => {
      console.error('Failed to listen for updater download events:', error);
      downloadListener = null;
      return () => {};
    });
  }
  return downloadListener;
}

export function checkForUpdates(
  silent = false,
  allowFallback = true,
  force = false,
): Promise<UpdateManifest | null> {
  if (!force && !state.checking && state.lastCheckedAt && Date.now() - state.lastCheckedAt < UPDATE_CHECK_THROTTLE_MS) {
    if (state.updateAvailable) {
      return Promise.resolve(pendingUpdate?.manifest ?? state.manifest);
    }
    return Promise.resolve(null);
  }

  if (inFlightCheckPromise) {
    return inFlightCheckPromise;
  }

  const epoch = checkEpoch;
  const publish = (next: Partial<UpdaterState>) => {
    if (epoch === checkEpoch) emit(next);
  };

  const promise = (async (): Promise<UpdateManifest | null> => {
    publish({
      status: 'checking',
      checking: true,
      error: null,
      errorCode: null,
      notice: null,
      downloadProgress: 0,
    });

    try {
      const metadata = await invoke<NativeUpdateMetadata | null>('app_updater_check');
      if (epoch !== checkEpoch) return null;
      const lastCheckedAt = Date.now();

      if (!metadata) {
        if (allowFallback && !silent) {
          try {
            const manifest = await checkViaGithubFallback();
            if (epoch !== checkEpoch) return null;
            if (manifest) {
              pendingUpdate = null;
              publish({
                status: 'available',
                checking: false,
                updateAvailable: true,
                installable: false,
                source: 'github-fallback',
                manifest,
                notice: 'fallbackCheckNotice',
                lastCheckedAt: Date.now(),
              });
              return manifest;
            }
          } catch (fallbackError) {
            console.error('Fallback check failed after tauri updater returned no updates:', fallbackError);
          }
        }

        if (epoch !== checkEpoch) return null;
        pendingUpdate = null;
        publish({
          status: 'idle',
          checking: false,
          updateAvailable: false,
          installable: false,
          source: 'tauri-updater',
          manifest: null,
          lastCheckedAt,
        });
        return null;
      }

      const manifest: UpdateManifest = {
        version: normalizeVersion(metadata.version),
        currentVersion: metadata.currentVersion,
        date: metadata.date ?? undefined,
        body: metadata.body ?? undefined,
      };
      pendingUpdate = { id: metadata.id, manifest };
      publish({
        status: 'available',
        checking: false,
        updateAvailable: true,
        installable: true,
        source: 'tauri-updater',
        manifest,
        lastCheckedAt,
      });
      return manifest;
    } catch (e) {
      if (epoch !== checkEpoch) return null;
      const errorCode = classifyError(e);
      console.error('Failed to check for updates:', e);

      if (allowFallback && (errorCode === 'EndpointInvalid' || errorCode === 'NetworkError')) {
        try {
          const manifest = await checkViaGithubFallback();
          if (epoch !== checkEpoch) return null;
          const lastCheckedAt = Date.now();
          if (!manifest) {
            pendingUpdate = null;
            publish({
              status: 'idle',
              checking: false,
              updateAvailable: false,
              installable: false,
              source: 'github-fallback',
              manifest: null,
              lastCheckedAt,
            });
            return null;
          }
          pendingUpdate = null;
          publish({
            status: 'available',
            checking: false,
            updateAvailable: true,
            installable: false,
            source: 'github-fallback',
            manifest,
            notice: 'fallbackCheckNotice',
            lastCheckedAt,
          });
          return manifest;
        } catch (fallbackError) {
          console.error('Fallback check failed:', fallbackError);
          if (epoch !== checkEpoch) return null;
        }
      }

      publish({
        status: 'error',
        checking: false,
        error: silent ? null : (errorCode === 'RateLimitExceeded' ? 'rateLimitNotice' : String(e)),
        errorCode,
      });
      return null;
    } finally {
      if (epoch === checkEpoch) {
        inFlightCheckPromise = null;
      }
    }
  })();

  inFlightCheckPromise = promise;
  return promise;
}

export function invalidateUpdaterTransport() {
  checkEpoch += 1;
  inFlightCheckPromise = null;
  const next: Partial<UpdaterState> = { lastCheckedAt: null };
  if (state.checking) next.checking = false;
  if (state.status === 'checking') next.status = 'idle';
  emit(next);
}

export async function downloadUpdateIfAvailable(silent = false) {
  if (!pendingUpdate || !state.installable || state.status === 'downloading' || state.status === 'installing') {
    return false;
  }
  const updateId = pendingUpdate.id;
  try {
    downloadedBytes = 0;
    totalBytes = 0;
    emit({ status: 'downloading', error: null, errorCode: null, downloadProgress: 0 });
    await ensureDownloadListener();
    await invoke('app_updater_download', { updateId });
    emit({ status: 'downloaded', downloadProgress: 100 });
    return true;
  } catch (e) {
    console.error('Failed to download update:', e);
    emit({
      status: 'error',
      error: silent ? null : String(e),
      errorCode: classifyError(e),
    });
    return false;
  }
}

export async function installDownloadedUpdate() {
  if (!pendingUpdate || state.status !== 'downloaded') return false;
  const updateId = pendingUpdate.id;
  try {
    emit({ status: 'installing', error: null, errorCode: null });
    await invoke('app_updater_install', { updateId });
    await relaunch();
    return true;
  } catch (e) {
    console.error('Failed to install downloaded update:', e);
    emit({
      status: 'error',
      error: String(e),
      errorCode: classifyError(e),
    });
    return false;
  }
}

export async function installUpdate() {
  if (!pendingUpdate || !state.installable) return false;
  if (state.status === 'downloaded') return true;
  return downloadUpdateIfAvailable();
}

export function getUpdaterState() {
  return state;
}

export function subscribeUpdater(listener: (next: UpdaterState) => void) {
  subscribers.add(listener);
  listener(state);
  return () => {
    subscribers.delete(listener);
  };
}

export function useUpdater() {
  const [snapshot, setSnapshot] = useState<UpdaterState>(state);

  useEffect(() => subscribeUpdater(setSnapshot), []);

  return {
    ...snapshot,
    checkForUpdates: (...args: Parameters<typeof checkForUpdates>) => checkForUpdates(...args),
    downloadUpdateIfAvailable: (...args: Parameters<typeof downloadUpdateIfAvailable>) =>
      downloadUpdateIfAvailable(...args),
    installDownloadedUpdate: (...args: Parameters<typeof installDownloadedUpdate>) =>
      installDownloadedUpdate(...args),
    installUpdate: (...args: Parameters<typeof installUpdate>) => installUpdate(...args),
  };
}

export function __resetUpdaterForTest() {
  checkEpoch += 1;
  pendingUpdate = null;
  inFlightCheckPromise = null;
  downloadedBytes = 0;
  totalBytes = 0;
  if (downloadListener) {
    void downloadListener.then((off) => off()).catch(() => {});
    downloadListener = null;
  }
  state = {
    status: 'idle',
    checking: false,
    updateAvailable: false,
    installable: false,
    source: null,
    error: null,
    errorCode: null,
    notice: null,
    manifest: null,
    downloadProgress: 0,
    lastCheckedAt: null,
  };
  for (const sub of subscribers) sub(state);
}
