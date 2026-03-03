import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { useState } from 'react';

export function useUpdater() {
  const [checking, setChecking] = useState(false);
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [manifest, setManifest] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);

  const checkForUpdates = async (silent = false) => {
    try {
      setChecking(true);
      setError(null);
      
      const update = await check();
      
      if (update) {
        setUpdateAvailable(true);
        setManifest(update);
        return update;
      } else {
        setUpdateAvailable(false);
        return null;
      }
    } catch (e: any) {
      console.error('Failed to check for updates:', e);
      const errorMsg = e.toString();
      // 如果是 404 或网络错误，给出友好提示
      const friendlyError = errorMsg.includes('404') || errorMsg.includes('network')
        ? '更新服务未配置，请稍后再试'
        : errorMsg;
      if (!silent) setError(friendlyError);
      return null;
    } finally {
      setChecking(false);
    }
  };

  const installUpdate = async () => {
    if (!manifest) return;
    
    try {
      // 下载并安装
      await manifest.downloadAndInstall();
      // 重启应用以应用更新
      await relaunch();
    } catch (e: any) {
      console.error('Failed to install update:', e);
      setError(e.toString());
    }
  };

  return {
    checking,
    updateAvailable,
    manifest,
    error,
    checkForUpdates,
    installUpdate,
  };
}
