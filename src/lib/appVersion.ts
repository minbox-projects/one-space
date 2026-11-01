import { useEffect, useState } from 'react';
import { getVersion } from '@tauri-apps/api/app';

const BETA_VERSION_PATTERN = /^\d+\.\d+\.\d+-beta\.\d+$/;

export function isBetaVersion(version: string | null | undefined): boolean {
  if (typeof version !== 'string') return false;
  return BETA_VERSION_PATTERN.test(version.trim());
}

export function useAppVersion(): string | null {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    const loadVersion = async () => {
      try {
        const resolved = await getVersion();
        if (!cancelled) setVersion(resolved ?? null);
      } catch {
        if (!cancelled) setVersion(null);
      }
    };

    void loadVersion();

    return () => {
      cancelled = true;
    };
  }, []);

  return version;
}
