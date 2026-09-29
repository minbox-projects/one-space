import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { AboutModal } from './AboutModal';
import * as updaterModule from '../lib/updater';

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn(async () => '0.1.41'),
}));

vi.mock('@tauri-apps/plugin-updater', () => ({
  check: vi.fn(async () => null),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'get_storage_config') {
      return { auto_update_enabled: true, update_check_interval_minutes: 120 };
    }
    return {};
  }),
}));

vi.mock('@/lib/externalActions', () => ({
  openExternalUrl: vi.fn(async () => undefined),
}));

describe('AboutModal component', () => {
  beforeEach(() => {
    updaterModule.__resetUpdaterForTest();
    vi.clearAllMocks();
  });

  it('renders nothing when open is false', () => {
    const { container } = render(<AboutModal open={false} onClose={vi.fn()} />);
    expect(container.firstChild).toBeNull();
  });

  it('actively triggers update check and displays current version release notes upon opening', async () => {
    const checkForUpdatesSpy = vi.fn(async () => null);
    const fetchNotesSpy = vi.fn(async () => ({
      version: '0.1.41',
      body: '### Features\n- Added modern about modal\n- Optimized updater check',
      publishedAt: '2026-09-29T10:00:00Z',
      htmlUrl: 'https://github.com/minbox-projects/one-space/releases/tag/v0.1.41',
    }));

    render(
      <AboutModal
        open={true}
        onClose={vi.fn()}
        autoCheckOnOpen={false}
        checkForUpdates={checkForUpdatesSpy}
        fetchReleaseNotes={fetchNotesSpy}
      />,
    );

    // Must have actively triggered check for updates on open even if autoCheckOnOpen is false
    await waitFor(() => {
      expect(checkForUpdatesSpy).toHaveBeenCalledTimes(1);
    });

    // Displays OneSpace header and current version
    await waitFor(() => {
      expect(screen.getByText('OneSpace')).toBeInTheDocument();
      expect(screen.getAllByText(/v0\.1\.41/)[0]).toBeInTheDocument();
    });

    // Fetches current version notes and displays the content
    await waitFor(() => {
      expect(fetchNotesSpy).toHaveBeenCalledWith('0.1.41');
      expect(screen.getByText('Added modern about modal')).toBeInTheDocument();
      expect(screen.getByText('Optimized updater check')).toBeInTheDocument();
    });

    // Displays auto-update status and platform info
    expect(screen.getByText('120m')).toBeInTheDocument();
  });

  it('displays update available banner and switches tabs when new version is detected', async () => {
    const useUpdaterSpy = vi.spyOn(updaterModule, 'useUpdater').mockReturnValue({
      status: 'available',
      checking: false,
      updateAvailable: true,
      installable: true,
      source: 'tauri-updater',
      error: null,
      errorCode: null,
      notice: null,
      manifest: {
        version: '0.1.42',
        body: '### Release v0.1.42\n- Amazing new capability',
        date: '2026-09-30T00:00:00Z',
      },
      downloadProgress: 0,
      lastCheckedAt: Date.now(),
      checkForUpdates: vi.fn(async () => null),
      downloadUpdateIfAvailable: vi.fn(async () => true),
      installDownloadedUpdate: vi.fn(async () => true),
      installUpdate: vi.fn(async () => true),
    });

    const fetchNotesSpy = vi.fn(async () => ({
      version: '0.1.41',
      body: 'Current notes',
    }));

    try {
      render(
        <AboutModal
          open={true}
          onClose={vi.fn()}
          fetchReleaseNotes={fetchNotesSpy}
        />,
      );

      // Should display latest version tab and banner
      await waitFor(() => {
        expect(screen.getByText(/Amazing new capability/i)).toBeInTheDocument();
      });

      // Click tab to view current version notes
      const currentTabBtn = screen.getByRole('button', {
        name: /当前版本更新明细|Current Version/i,
      });
      fireEvent.click(currentTabBtn);

      await waitFor(() => {
        expect(screen.getByText('Current notes')).toBeInTheDocument();
      });
    } finally {
      useUpdaterSpy.mockRestore();
    }
  });

  it('triggers manual recheck on button click', async () => {
    const checkForUpdatesSpy = vi.fn(async () => null);
    const fetchNotesSpy = vi.fn(async () => ({
      version: '0.1.41',
      body: 'Notes',
    }));

    render(
      <AboutModal
        open={true}
        onClose={vi.fn()}
        checkForUpdates={checkForUpdatesSpy}
        fetchReleaseNotes={fetchNotesSpy}
      />,
    );

    await waitFor(() => {
      expect(screen.getAllByText(/v0\.1\.41/)[0]).toBeInTheDocument();
    });

    // Initial check on mount
    await waitFor(() => {
      expect(checkForUpdatesSpy).toHaveBeenCalledTimes(1);
    });

    // Find and click the recheck button
    const recheckBtn = screen.getByRole('button', { name: /重新检查|Check Again/i });
    fireEvent.click(recheckBtn);

    await waitFor(() => {
      // Must be called with force = true on manual click
      expect(checkForUpdatesSpy).toHaveBeenCalledWith(false, true, true);
    });
  });
});
