import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  normalizeVersion,
  isVersionGreater,
  fetchCurrentVersionReleaseNotes,
  checkForUpdates,
  __resetUpdaterForTest,
} from './updater';

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn(async () => '0.1.41'),
}));

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(async () => null),
}));

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(async () => undefined),
}));

describe('updater module', () => {
  beforeEach(() => {
    __resetUpdaterForTest();
    vi.clearAllMocks();
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

  it('correctly compares version precedence', () => {
    expect(isVersionGreater('0.1.42', '0.1.41')).toBe(true);
    expect(isVersionGreater('0.2.0', '0.1.99')).toBe(true);
    expect(isVersionGreater('1.0.0', '0.9.9')).toBe(true);
    expect(isVersionGreater('0.1.41', '0.1.41')).toBe(false);
    expect(isVersionGreater('0.1.40', '0.1.41')).toBe(false);
  });

  it('fetches current version release notes with localStorage cache', async () => {
    const mockRelease = {
      body: '## Changelog\n- feature A\n- bug fix B',
      published_at: '2026-09-29T10:00:00Z',
      html_url: 'https://github.com/minbox-projects/one-space/releases/tag/v0.1.41',
    };

    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
      ok: true,
      json: async () => mockRelease,
    } as Response);

    // First fetch: hits API and saves to cache
    const result1 = await fetchCurrentVersionReleaseNotes('0.1.41');
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(result1).not.toBeNull();
    expect(result1?.version).toBe('0.1.41');
    expect(result1?.body).toBe(mockRelease.body);

    // Second fetch: should hit localStorage cache without calling fetch
    const result2 = await fetchCurrentVersionReleaseNotes('0.1.41', false);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(result2?.body).toBe(mockRelease.body);

    // Third fetch with force = true: should bypass cache
    fetchSpy.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ ...mockRelease, body: 'Updated notes' }),
    } as Response);
    const result3 = await fetchCurrentVersionReleaseNotes('0.1.41', true);
    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(result3?.body).toBe('Updated notes');
  });

  it('deduplicates concurrent check requests', async () => {
    const { check } = await import('@tauri-apps/plugin-updater');
    let resolveCheck: (val: any) => void;
    const checkPromise = new Promise((resolve) => {
      resolveCheck = resolve;
    });
    vi.mocked(check).mockImplementationOnce(() => checkPromise as any);

    // Trigger two checks concurrently
    const p1 = checkForUpdates(false, false, true);
    const p2 = checkForUpdates(false, false, true);

    // They should share the exact same promise instance
    expect(p1).toBe(p2);

    resolveCheck!(null);
    const [res1, res2] = await Promise.all([p1, p2]);
    expect(res1).toBeNull();
    expect(res2).toBeNull();
    expect(check).toHaveBeenCalledTimes(1);
  });

  it('throttles non-forced calls within the throttle interval', async () => {
    const { check } = await import('@tauri-apps/plugin-updater');
    vi.mocked(check).mockResolvedValue(null);

    // First check (force = true)
    await checkForUpdates(false, false, true);
    expect(check).toHaveBeenCalledTimes(1);

    // Second check within 30s with force = false should be throttled
    await checkForUpdates(false, false, false);
    expect(check).toHaveBeenCalledTimes(1);

    // Third check with force = true should execute
    await checkForUpdates(false, false, true);
    expect(check).toHaveBeenCalledTimes(2);
  });
});
