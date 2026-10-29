import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  normalizeVersion,
  parseVersion,
  isVersionGreater,
  fetchCurrentVersionReleaseNotes,
  checkForUpdates,
  downloadUpdateIfAvailable,
  installDownloadedUpdate,
  invalidateUpdaterTransport,
  getUpdaterState,
  subscribeUpdater,
  __resetUpdaterForTest,
} from './updater';
import {
  invokeMock,
  listenMock,
  resetTauriMocks,
  appUpdaterCheckMock,
  appUpdaterDownloadMock,
  appUpdaterInstallMock,
  proxyHttpRequestMock,
  emitUpdaterDownloadEvent,
  APP_UPDATER_DOWNLOAD_EVENT,
} from '@/test/mocks/tauri';

const { getVersionMock, relaunchMock } = vi.hoisted(() => ({
  getVersionMock: vi.fn(async () => '0.1.41'),
  relaunchMock: vi.fn(async () => undefined),
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: getVersionMock,
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: relaunchMock,
}));

const RELEASE_API_PREFIX =
  'https://api.github.com/repos/minbox-projects/one-space';

function nativeMetadata(overrides: Record<string, unknown> = {}) {
  return {
    id: '0.1.41:0.1.42:darwin-aarch64',
    version: 'v0.1.42',
    currentVersion: '0.1.41',
    date: '2026-10-01T08:00:00+08:00',
    body: '## Upstream release\n- native updater facade',
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe('updater module', () => {
  beforeEach(() => {
    __resetUpdaterForTest();
    resetTauriMocks();
    getVersionMock.mockReset();
    getVersionMock.mockResolvedValue('0.1.41');
    relaunchMock.mockReset();
    relaunchMock.mockResolvedValue(undefined);
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('normalizes version strings properly', () => {
    expect(normalizeVersion('v0.1.41')).toBe('0.1.41');
    expect(normalizeVersion('V1.2.3')).toBe('1.2.3');
    expect(normalizeVersion('  0.2.0 ')).toBe('0.2.0');
  });

  it('parses version components with zero fallbacks', () => {
    expect(parseVersion('v1.2.3')).toEqual([1, 2, 3]);
    expect(parseVersion('2.5')).toEqual([2, 5, 0]);
    expect(parseVersion('not-a-version')).toEqual([0, 0, 0]);
  });

  it('correctly compares version precedence', () => {
    expect(isVersionGreater('0.1.42', '0.1.41')).toBe(true);
    expect(isVersionGreater('0.2.0', '0.1.99')).toBe(true);
    expect(isVersionGreater('1.0.0', '0.9.9')).toBe(true);
    expect(isVersionGreater('0.1.41', '0.1.41')).toBe(false);
    expect(isVersionGreater('0.1.40', '0.1.41')).toBe(false);
  });

  it('maps native check metadata into the manifest and updater state', async () => {
    const metadata = nativeMetadata();
    appUpdaterCheckMock.mockResolvedValue(metadata);

    const manifest = await checkForUpdates(false, false, true);

    expect(invokeMock).toHaveBeenCalledWith('app_updater_check');
    expect(manifest).toEqual({
      version: '0.1.42',
      currentVersion: '0.1.41',
      date: '2026-10-01T08:00:00+08:00',
      body: '## Upstream release\n- native updater facade',
    });
    expect(getUpdaterState()).toMatchObject({
      status: 'available',
      checking: false,
      updateAvailable: true,
      installable: true,
      source: 'tauri-updater',
      error: null,
      errorCode: null,
      notice: null,
      manifest,
      downloadProgress: 0,
    });
    expect(getUpdaterState().lastCheckedAt).toEqual(expect.any(Number));
    expect(proxyHttpRequestMock).not.toHaveBeenCalled();
  });

  it('reports idle without a GitHub fallback when the native check finds nothing and fallback is disabled', async () => {
    appUpdaterCheckMock.mockResolvedValue(null);

    const manifest = await checkForUpdates(false, false, true);

    expect(manifest).toBeNull();
    expect(getUpdaterState()).toMatchObject({
      status: 'idle',
      checking: false,
      updateAvailable: false,
      installable: false,
      source: 'tauri-updater',
      manifest: null,
    });
    expect(proxyHttpRequestMock).not.toHaveBeenCalled();
  });

  it('deduplicates concurrent check requests', async () => {
    const pending = deferred<unknown>();
    appUpdaterCheckMock.mockReturnValueOnce(pending.promise);

    const p1 = checkForUpdates(false, false, true);
    const p2 = checkForUpdates(false, false, true);

    expect(p1).toBe(p2);

    pending.resolve(null);
    const [res1, res2] = await Promise.all([p1, p2]);

    expect(res1).toBeNull();
    expect(res2).toBeNull();
    expect(appUpdaterCheckMock).toHaveBeenCalledTimes(1);
  });

  it('throttles non-forced calls within the throttle interval', async () => {
    appUpdaterCheckMock.mockResolvedValue(null);

    await checkForUpdates(false, false, true);
    expect(appUpdaterCheckMock).toHaveBeenCalledTimes(1);

    await checkForUpdates(false, false, false);
    expect(appUpdaterCheckMock).toHaveBeenCalledTimes(1);

    await checkForUpdates(false, false, true);
    expect(appUpdaterCheckMock).toHaveBeenCalledTimes(2);
  });

  it('invalidateUpdaterTransport starts a new check instead of joining the old in-flight check', async () => {
    const stale = deferred<unknown>();
    appUpdaterCheckMock.mockReturnValueOnce(stale.promise);
    appUpdaterCheckMock.mockResolvedValueOnce(
      nativeMetadata({
        id: '0.1.41:0.1.43:darwin-aarch64',
        version: '0.1.43',
      }),
    );

    const staleCheck = checkForUpdates(false, false, true);
    invalidateUpdaterTransport();
    const freshCheck = checkForUpdates(false, false, true);

    expect(freshCheck).not.toBe(staleCheck);
    await expect(freshCheck).resolves.toMatchObject({ version: '0.1.43' });
    expect(appUpdaterCheckMock).toHaveBeenCalledTimes(2);

    stale.resolve(
      nativeMetadata({
        id: '0.1.41:0.1.42:darwin-aarch64',
        version: '0.1.42',
      }),
    );

    await expect(staleCheck).resolves.toBeNull();
    expect(getUpdaterState().manifest?.version).toBe('0.1.43');
    expect(getUpdaterState().status).toBe('available');
    expect(getUpdaterState().lastCheckedAt).toEqual(expect.any(Number));
  });

  it('downloads through the native command, emits ordered progress and installs the same update id', async () => {
    appUpdaterCheckMock.mockResolvedValue(
      nativeMetadata({ id: 'update-42' }),
    );
    await checkForUpdates(false, false, true);

    const progressSnapshots: number[] = [];
    const unsubscribe = subscribeUpdater((next) => {
      progressSnapshots.push(next.downloadProgress);
    });

    appUpdaterDownloadMock.mockImplementation(async () => {
      emitUpdaterDownloadEvent({
        event: 'Started',
        data: { contentLength: 200 },
      });
      emitUpdaterDownloadEvent({ event: 'Progress', data: { chunkLength: 50 } });
      emitUpdaterDownloadEvent({ event: 'Progress', data: { chunkLength: 50 } });
      emitUpdaterDownloadEvent({ event: 'Finished' });
    });

    const downloaded = await downloadUpdateIfAvailable();

    unsubscribe();

    expect(downloaded).toBe(true);
    expect(listenMock).toHaveBeenCalledWith(
      APP_UPDATER_DOWNLOAD_EVENT,
      expect.any(Function),
    );
    expect(appUpdaterDownloadMock).toHaveBeenCalledWith('update-42');
    expect(Array.from(new Set(progressSnapshots))).toEqual([0, 25, 50, 100]);
    expect(getUpdaterState()).toMatchObject({
      status: 'downloaded',
      downloadProgress: 100,
    });

    const installed = await installDownloadedUpdate();

    expect(installed).toBe(true);
    expect(appUpdaterInstallMock).toHaveBeenCalledWith('update-42');
    expect(relaunchMock).toHaveBeenCalledTimes(1);
    expect(getUpdaterState().status).toBe('installing');
  });

  it('fetches current version release notes through proxy_http_request with 404 tag fallback and localStorage cache', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const release = {
      body: '## Changelog\n- feature A\n- bug fix B',
      published_at: '2026-09-29T10:00:00Z',
      html_url:
        'https://github.com/minbox-projects/one-space/releases/tag/v0.1.41',
    };
    proxyHttpRequestMock
      .mockRejectedValueOnce(new Error('HTTP 404: Not Found'))
      .mockResolvedValueOnce(JSON.stringify(release));

    const result1 = await fetchCurrentVersionReleaseNotes('0.1.41');

    expect(proxyHttpRequestMock).toHaveBeenCalledTimes(2);
    expect(proxyHttpRequestMock).toHaveBeenNthCalledWith(
      1,
      `${RELEASE_API_PREFIX}/releases/tags/v0.1.41`,
      'GET',
      { Accept: 'application/vnd.github+json' },
      null,
    );
    expect(proxyHttpRequestMock).toHaveBeenNthCalledWith(
      2,
      `${RELEASE_API_PREFIX}/releases/tags/0.1.41`,
      'GET',
      { Accept: 'application/vnd.github+json' },
      null,
    );
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(result1).not.toBeNull();
    expect(result1?.version).toBe('0.1.41');
    expect(result1?.body).toBe(release.body);
    expect(result1?.htmlUrl).toBe(release.html_url);

    const result2 = await fetchCurrentVersionReleaseNotes('0.1.41', false);
    expect(proxyHttpRequestMock).toHaveBeenCalledTimes(2);
    expect(result2?.body).toBe(release.body);

    proxyHttpRequestMock.mockResolvedValueOnce(
      JSON.stringify({ ...release, body: 'Updated notes' }),
    );
    const result3 = await fetchCurrentVersionReleaseNotes('0.1.41', true);
    expect(proxyHttpRequestMock).toHaveBeenCalledTimes(3);
    expect(result3?.body).toBe('Updated notes');
  });

  it('checks the GitHub fallback through proxy_http_request without fetch', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    appUpdaterCheckMock.mockResolvedValue(null);
    proxyHttpRequestMock.mockResolvedValue(
      JSON.stringify({
        tag_name: 'v0.1.42',
        body: 'Fallback release notes',
        published_at: '2026-10-02T00:00:00Z',
      }),
    );

    const manifest = await checkForUpdates(false, true, true);

    expect(proxyHttpRequestMock).toHaveBeenCalledWith(
      `${RELEASE_API_PREFIX}/releases/latest`,
      'GET',
      { Accept: 'application/vnd.github+json' },
      null,
    );
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(manifest).toEqual({
      version: '0.1.42',
      currentVersion: '0.1.41',
      body: 'Fallback release notes',
      date: '2026-10-02T00:00:00Z',
    });
    expect(getUpdaterState()).toMatchObject({
      status: 'available',
      source: 'github-fallback',
      installable: false,
      updateAvailable: true,
      notice: 'fallbackCheckNotice',
    });
  });

  it('falls back through proxy_http_request when the native endpoint is invalid', async () => {
    appUpdaterCheckMock.mockRejectedValue(new Error('invalid latest.json body'));
    proxyHttpRequestMock.mockResolvedValue(
      JSON.stringify({ tag_name: 'v0.1.42', body: 'notes' }),
    );

    const manifest = await checkForUpdates(false, true, true);

    expect(manifest?.version).toBe('0.1.42');
    expect(proxyHttpRequestMock).toHaveBeenCalledWith(
      `${RELEASE_API_PREFIX}/releases/latest`,
      'GET',
      { Accept: 'application/vnd.github+json' },
      null,
    );
    expect(getUpdaterState()).toMatchObject({
      status: 'available',
      source: 'github-fallback',
      installable: false,
    });
  });

  it.each([
    ['GitHub API rate limit exceeded', 'RateLimitExceeded'],
    ['invalid latest.json payload', 'EndpointInvalid'],
    ['network connection reset', 'NetworkError'],
    ['signature verification failed', 'SignatureOrInstallError'],
    ['unexpected failure', 'Unknown'],
  ])('maps "%s" to the %s error code', async (message, expectedCode) => {
    appUpdaterCheckMock.mockRejectedValue(new Error(message));

    const manifest = await checkForUpdates(true, false, true);

    expect(manifest).toBeNull();
    expect(getUpdaterState()).toMatchObject({
      status: 'error',
      checking: false,
      error: null,
      errorCode: expectedCode,
    });
  });

  it('surfaces a rate-limit notice for non-silent checks', async () => {
    appUpdaterCheckMock.mockRejectedValue(
      new Error('GitHub API rate limit exceeded'),
    );

    await checkForUpdates(false, false, true);

    expect(getUpdaterState()).toMatchObject({
      status: 'error',
      error: 'rateLimitNotice',
      errorCode: 'RateLimitExceeded',
    });
  });
});
