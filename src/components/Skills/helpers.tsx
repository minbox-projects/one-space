import type { ComponentType } from 'react';
import { Sparkles, Wrench, Shield, Cpu, BookOpen } from 'lucide-react';
import { skillModelOptions } from '../skillsModelOptions';
import type { InstalledSkillTarget, ModelType } from './types';

export const SKILLS_AUTO_UPDATED_EVENT = 'onespace:skills-auto-updated';

export const modelTabs: { id: ModelType; label: string }[] = [
  { id: 'claude', label: 'Claude' },
  { id: 'antigravity', label: 'Antigravity' },
  { id: 'codex', label: 'Codex' },
  { id: 'opencode', label: 'OpenCode' },
];

export const allModels: ModelType[] = ['claude', 'antigravity', 'codex', 'opencode'];

export const modelIconMap: Record<ModelType, ComponentType<{ className?: string }>> = skillModelOptions.reduce(
  (acc, item) => {
    acc[item.id] = item.Icon;
    return acc;
  },
  {} as Record<ModelType, ComponentType<{ className?: string }>>
);

export const iconPool = [Sparkles, Wrench, Shield, Cpu, BookOpen];
export const TAB_LOADING_MIN_MS = 200;

export function formatTs(ts?: number) {
  if (!ts) return '--';
  return new Date(ts * 1000).toLocaleString();
}

export function formatInstalledTarget(target: InstalledSkillTarget) {
  const scopeLabel = target.scope === 'project' ? 'Project' : 'Global';
  if (target.project_root) {
    return `${target.model} · ${scopeLabel} · ${target.project_root}`;
  }
  return `${target.model} · ${scopeLabel}`;
}

export function errorContainsCode(error: unknown, code: string) {
  return String(error ?? '').includes(code);
}

export function renderDiffDocument(markdown: string, changedLines: number[]) {
  const normalized = markdown.replace(/\r\n/g, '\n');
  const lines = normalized.length > 0 ? normalized.split('\n') : [''];
  const changedSet = new Set((changedLines || []).map((n) => Number(n)));

  return (
    <div className="rounded-md border overflow-hidden">
      <div className="max-h-[46vh] overflow-auto font-mono text-[11px] leading-5">
        {lines.map((line, idx) => {
          const lineNumber = idx + 1;
          const changed = changedSet.has(lineNumber);
          return (
            <div
              key={`diff-line-${lineNumber}`}
              className={`grid grid-cols-[56px,1fr] ${changed ? 'bg-amber-50/80' : 'bg-background'}`}
            >
              <div
                className={`border-r px-2 py-1 text-right select-none ${
                  changed
                    ? 'text-amber-700 bg-amber-100/80 border-amber-200'
                    : 'text-muted-foreground bg-muted/30 border-border'
                }`}
              >
                {lineNumber}
              </div>
              <pre
                className={`px-3 py-1 whitespace-pre-wrap break-words ${
                  changed ? 'text-amber-900' : 'text-foreground'
                }`}
              >
                {line || ' '}
              </pre>
            </div>
          );
        })}
      </div>
    </div>
  );
}
