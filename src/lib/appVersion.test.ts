import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { isBetaVersion, useAppVersion } from './appVersion';

const { getVersionMock } = vi.hoisted(() => ({
  getVersionMock: vi.fn(async () => '0.1.58-beta.2'),
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: getVersionMock,
}));

describe('appVersion', () => {
  beforeEach(() => {
    getVersionMock.mockReset();
    getVersionMock.mockResolvedValue('0.1.58-beta.2');
  });

  it('detects well-formed beta versions', () => {
    expect(isBetaVersion('0.1.58-beta.2')).toBe(true);
    expect(isBetaVersion('1.0.0-beta.10')).toBe(true);
  });

  it('rejects stable, malformed and missing versions', () => {
    expect(isBetaVersion('0.1.57')).toBe(false);
    expect(isBetaVersion('0.1.58-beta')).toBe(false);
    expect(isBetaVersion('0.1.58-beta.x')).toBe(false);
    expect(isBetaVersion('beta.2')).toBe(false);
    expect(isBetaVersion('')).toBe(false);
    expect(isBetaVersion(null)).toBe(false);
    expect(isBetaVersion(undefined)).toBe(false);
  });

  it('returns null when getVersion rejects', async () => {
    getVersionMock.mockRejectedValue(new Error('boom'));
    const { result } = renderHook(() => useAppVersion());
    await new Promise((r) => setTimeout(r, 10));
    expect(result.current).toBeNull();
  });
});
