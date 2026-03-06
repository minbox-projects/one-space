import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { check, type DownloadEvent, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

type UpdaterStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'installing' | 'error';
type UpdaterSource = 'tauri-updater' | 'github-fallback' | null;
type UpdaterErrorCode = 'EndpointInvalid' | 'NetworkError' | 'SignatureOrInstallError' | 'Unknown' | null;

interface UpdateManifest {
  version: string;
  currentVersion?: string;
  date?: string;
  body?: string;
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

const GITHUB_REPO = 'minbox-projects/one-space';

let pendingUpdate: Update | null = null;
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

function normalizeVersion(version: string): string {
  return version.trim().replace(/^v/i, '');
}

function parseVersion(version: string): [number, number, number] {
  const parts = normalizeVersion(version).split('.').map((p) => parseInt(p, 10) || 0);
  return [parts[0] || 0, parts[1] || 0, parts[2] || 0];
}

function isVersionGreater(a: string, b: string): boolean {
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

async function checkViaGithubFallback(): Promise<UpdateManifest | null> {
  const currentVersion = normalizeVersion(await getVersion());
  const res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/releases/latest`, {
    headers: { Accept: 'application/vnd.github+json' },
  });
  if (!res.ok) {
    throw new Error(`GitHub API HTTP ${res.status}`);
  }
  const json = await res.json();
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

export async function checkForUpdates(silent = false, allowFallback = true) {
  emit({
    status: 'checking',
    checking: true,
    error: null,
    errorCode: null,
    notice: null,
    downloadProgress: 0,
  });

  try {
    const update = await check();
    const lastCheckedAt = Date.now();

    if (!update) {
      pendingUpdate = null;
      emit({
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

    pendingUpdate = update;
    emit({
      status: 'available',
      checking: false,
      updateAvailable: true,
      installable: true,
      source: 'tauri-updater',
      manifest: {
        version: normalizeVersion(update.version),
        currentVersion: update.currentVersion,
        date: update.date,
        body: update.body,
      },
      lastCheckedAt,
    });
    return update;
  } catch (e) {
    const errorCode = classifyError(e);
    console.error('Failed to check for updates:', e);

    if (allowFallback && (errorCode === 'EndpointInvalid' || errorCode === 'NetworkError')) {
      try {
        const manifest = await checkViaGithubFallback();
        const lastCheckedAt = Date.now();
        if (!manifest) {
          pendingUpdate = null;
          emit({
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
        emit({
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
      }
    }

    emit({
      status: 'error',
      checking: false,
      error: silent ? null : String(e),
      errorCode,
    });
    return null;
  }
}

export async function downloadUpdateIfAvailable(silent = false) {
  if (!pendingUpdate || !state.installable || state.status === 'downloading' || state.status === 'installing') {
    return false;
  }
  try {
    let downloadedBytes = 0;
    let totalBytes = 0;
    emit({ status: 'downloading', error: null, errorCode: null, downloadProgress: 0 });
    await pendingUpdate.download((event: DownloadEvent) => {
      if (event.event === 'Started') {
        totalBytes = event.data.contentLength || 0;
      } else if (event.event === 'Progress') {
        downloadedBytes += event.data.chunkLength;
        const pct = totalBytes > 0 ? Math.min(100, Math.round((downloadedBytes / totalBytes) * 100)) : 0;
        emit({ downloadProgress: pct });
      } else if (event.event === 'Finished') {
        emit({ downloadProgress: 100 });
      }
    });
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
  try {
    emit({ status: 'installing', error: null, errorCode: null });
    await pendingUpdate.install();
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
    checkForUpdates,
    downloadUpdateIfAvailable,
    installDownloadedUpdate,
    installUpdate,
  };
}
