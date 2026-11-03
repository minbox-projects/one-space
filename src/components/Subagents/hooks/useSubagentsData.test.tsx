import type { ReactNode } from 'react';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ConfirmDialogProvider } from '@/components/ConfirmDialogProvider';
import { resetTauriMocks } from '@/test/mocks/tauri';
import { useSubagentsData } from './useSubagentsData';

const subagentMocks = vi.hoisted(() => ({
  subagentsListCatalog: vi.fn(),
  subagentsListInstalled: vi.fn(),
  subagentsRepoList: vi.fn(),
  subagentsSyncLocal: vi.fn(),
  subagentsSyncNow: vi.fn(),
  subagentsSyncStatusGet: vi.fn(),
}));

vi.mock('@/lib/subagents', () => subagentMocks);

function wrapper({ children }: { children: ReactNode }) {
  return <ConfirmDialogProvider>{children}</ConfirmDialogProvider>;
}

describe('useSubagentsData', () => {
  beforeEach(() => {
    resetTauriMocks();
    Object.values(subagentMocks).forEach((mock) => mock.mockReset());
    subagentMocks.subagentsSyncLocal.mockResolvedValue({ data: [] });
    subagentMocks.subagentsSyncNow.mockResolvedValue(undefined);
    subagentMocks.subagentsListCatalog.mockResolvedValue({ data: [] });
    subagentMocks.subagentsRepoList.mockResolvedValue({ data: [] });
    subagentMocks.subagentsSyncStatusGet.mockResolvedValue({ data: null });
    subagentMocks.subagentsListInstalled.mockResolvedValue({
      data: [
        {
          id: 'claude-helper',
          model: 'claude',
          name: 'Claude Helper',
          description: 'Claude',
          source_id: 'local',
          source_rel_path: 'claude-helper',
          installed_at: 1,
          has_update: false,
          icon_seed: 'a',
          scope: 'global',
        },
        {
          id: 'opencode-helper',
          model: 'opencode',
          name: 'OpenCode Helper',
          description: 'OpenCode',
          source_id: 'local',
          source_rel_path: 'opencode-helper',
          installed_at: 2,
          has_update: false,
          icon_seed: 'b',
          scope: 'global',
        },
      ],
    });
  });

  it('loads installed subagents for every model group', async () => {
    const { result } = renderHook(() => useSubagentsData({ isVisible: true }), {
      wrapper,
    });

    await waitFor(() => {
      expect(result.current.initialLoadDone).toBe(true);
    });

    expect(subagentMocks.subagentsListInstalled).toHaveBeenCalledWith({
      model: null,
      scope: 'global',
      project_root: null,
    });
    expect(result.current.installedByModel.claude.map((item) => item.id)).toEqual([
      'claude-helper',
    ]);
    expect(result.current.installedByModel.opencode.map((item) => item.id)).toEqual([
      'opencode-helper',
    ]);
    expect(result.current.installedCounts.opencode).toBe(1);
  });
});
