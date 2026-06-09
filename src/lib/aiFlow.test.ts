import { describe, expect, it } from 'vitest';
import { invokeMock } from '@/test/mocks/tauri';
import {
  aiFlowConfigSave,
  aiFlowLaunchAction,
  aiFlowLaunchPreview,
  aiFlowPlanContentGet,
  aiFlowProjectsList,
  aiFlowQueueCreate,
  aiFlowFormatError,
} from './aiFlow';

describe('aiFlow API wrapper', () => {
  it('passes extra project path to projects list', async () => {
    invokeMock.mockResolvedValue({ ok: true, data: [], meta: { schema_version: 1, revision: 1 } });

    await aiFlowProjectsList('/tmp/project');

    expect(invokeMock).toHaveBeenCalledWith('ai_flow_projects_list', {
      extraPath: '/tmp/project',
    });
  });

  it('saves config through the expected command payload', async () => {
    invokeMock.mockResolvedValue({
      ok: true,
      data: { path: '/tmp/project/.ai-flow/rule.yaml' },
      meta: { schema_version: 1, revision: 1 },
    });

    await aiFlowConfigSave({
      scope: 'project_rule',
      project_root: '/tmp/project',
      format: 'yaml',
      content: 'version: 1\n',
    });

    expect(invokeMock).toHaveBeenCalledWith('ai_flow_config_save', {
      input: {
        scope: 'project_rule',
        project_root: '/tmp/project',
        format: 'yaml',
        content: 'version: 1\n',
      },
    });
  });

  it('reads plan content through the expected command payload', async () => {
    invokeMock.mockResolvedValue({
      ok: true,
      data: { plan_path: '/tmp/project/docs/plan-a.md', content: '# Plan A', exists: true },
      meta: { schema_version: 1, revision: 1 },
    });

    await aiFlowPlanContentGet('/tmp/project', 'plan-a');

    expect(invokeMock).toHaveBeenCalledWith('ai_flow_plan_content_get', {
      projectRoot: '/tmp/project',
      planSlug: 'plan-a',
    });
  });

  it('launches queue resume with explicit slug', async () => {
    invokeMock.mockResolvedValue({ ok: true, data: {}, meta: { schema_version: 1, revision: 1 } });

    await aiFlowLaunchAction({
      tool: 'codex',
      action: 'resume',
      slug: 'queue-1',
      project_root: '/tmp/project',
    });

    expect(invokeMock).toHaveBeenCalledWith('ai_flow_launch_action', {
      input: {
        tool: 'codex',
        action: 'resume',
        slug: 'queue-1',
        project_root: '/tmp/project',
      },
    });
  });

  it('previews launch permissions before starting a session', async () => {
    invokeMock.mockResolvedValue({
      ok: true,
      data: {
        tool: 'claude',
        permission_confirmation_required: true,
        prompt: '/ai-flow-plan-coding 20260609-plan',
      },
      meta: { schema_version: 1, revision: 1 },
    });

    await aiFlowLaunchPreview({
      tool: 'claude',
      action: 'coding',
      slug: '20260609-plan',
      project_root: '/tmp/project',
    });

    expect(invokeMock).toHaveBeenCalledWith('ai_flow_launch_preview', {
      input: {
        tool: 'claude',
        action: 'coding',
        slug: '20260609-plan',
        project_root: '/tmp/project',
      },
    });
  });

  it('creates queue with explicit ordered plan slugs', async () => {
    invokeMock.mockResolvedValue({
      ok: true,
      data: { queue_slug: 'queue-a', state_path: '/tmp/project/.ai-flow/orchestrations/state/queue-a.json', log: '' },
      meta: { schema_version: 1, revision: 1 },
    });

    await aiFlowQueueCreate({
      project_root: '/tmp/project',
      queue_slug: 'queue-a',
      plan_slugs: ['20260609-a', '20260609-b'],
    });

    expect(invokeMock).toHaveBeenCalledWith('ai_flow_queue_create', {
      input: {
        project_root: '/tmp/project',
        queue_slug: 'queue-a',
        plan_slugs: ['20260609-a', '20260609-b'],
      },
    });
  });

  it('formats structured command errors with code', () => {
    expect(aiFlowFormatError({ code: 'AI_FLOW_SLUG_REQUIRED', message: 'slug is required' }))
      .toBe('[AI_FLOW_SLUG_REQUIRED] slug is required');
  });
});
