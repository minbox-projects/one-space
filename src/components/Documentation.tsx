import { isValidElement, useState, useEffect, type ElementType, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { 
  BarChart3,
  BookOpen, 
  Bot,
  Cloud,
  Code2,
  Terminal, 
  FolderOpen,
  Gamepad2,
  HelpCircle,
  Mail,
  Newspaper,
  NotebookPen,
  Rocket,
  Route,
  Server, 
  Sparkles,
  Waypoints,
  Download, 
  Info,
  ArrowLeft
} from 'lucide-react';
import usageDoc from '../../docs/USAGE.md?raw';
import cliDoc from '../../docs/CLI.md?raw';
import skillsDoc from '../../docs/SKILLS.md?raw';
import mcpDoc from '../../docs/MCP.md?raw';

type DocId = 'usage' | 'cli' | 'skills' | 'mcp';
type ActiveDoc = { docId: DocId; anchor?: string };
type DocEntry = {
  id: DocId;
  name: string;
  icon: ElementType;
  content: string;
};
type ModuleEntry = {
  id: string;
  name: string;
  summary: string;
  icon: ElementType;
  docId: DocId;
  anchor?: string;
};
type ModuleGroup = {
  id: string;
  title: string;
  items: ModuleEntry[];
};

function headingText(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(headingText).join('');
  if (isValidElement<{ children?: ReactNode }>(node)) return headingText(node.props.children);
  return '';
}

function slugifyHeading(value: ReactNode) {
  return headingText(value)
    .trim()
    .toLowerCase()
    .replace(/[`"'“”‘’]/g, '')
    .replace(/[^\p{Letter}\p{Number}\s-]/gu, '')
    .replace(/\s+/g, '-');
}

export function Documentation() {
  const { t } = useTranslation();
  const [activeDoc, setActiveDoc] = useState<ActiveDoc | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });

  // Handle direct navigation to sections via hash or state
  useEffect(() => {
    const section = window.location.hash.replace('#', '');
    if (section) {
      setActiveDoc({ docId: section as DocId });
      window.location.hash = ''; // Clear hash after reading
    }
  }, []);

  useEffect(() => {
    if (!activeDoc?.anchor) return;
    const frame = window.requestAnimationFrame(() => {
      document.getElementById(activeDoc.anchor || '')?.scrollIntoView({
        block: 'start',
        behavior: 'smooth',
      });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activeDoc]);

  const handleInstall = async () => {
    try {
      setLoading(true);
      await invoke('install_cli');
      setMessage({ type: 'success', text: t('cliInstalled', 'CLI tool installed to ~/.local/bin/onespace') });
    } catch (err: unknown) {
      setMessage({ type: 'error', text: String(err) });
    } finally {
      setLoading(false);
    }
  };

  const docEntries: DocEntry[] = [
    { 
      id: 'usage', 
      name: t('docsUsageGuide', 'Usage Manual'), 
      icon: BookOpen, 
      content: usageDoc,
    },
    { 
      id: 'cli', 
      name: t('docsCliGuide', 'CLI Guide'), 
      icon: Terminal, 
      content: cliDoc,
    },
    { 
      id: 'skills', 
      name: t('docsSkillsGuide', 'Skills & Subagents Guide'), 
      icon: Sparkles, 
      content: skillsDoc,
    },
    { 
      id: 'mcp', 
      name: t('docsMcpGuide', 'MCP Guide'), 
      icon: Server, 
      content: mcpDoc,
    },
  ];

  const moduleGroups: ModuleGroup[] = [
    {
      id: 'core',
      title: t('docsGroupCore', 'Core'),
      items: [
        {
          id: 'usage-overview',
          name: t('docsUsageGuide', 'Usage Manual'),
          summary: t('docsUsageGuideSummary', 'Complete OneSpace user manual and feature map.'),
          icon: BookOpen,
          docId: 'usage',
        },
        {
          id: 'launcher',
          name: t('launcher', 'Launcher'),
          summary: t('docsLauncherSummary', 'Launch apps, scripts, URLs, folders, and internal OneSpace pages.'),
          icon: Rocket,
          docId: 'usage',
          anchor: '14-launcher-与-more-tools',
        },
        {
          id: 'workspaces',
          name: t('workspaces', 'Workspaces'),
          summary: t('docsWorkspacesSummary', 'Organize project sessions, MCP, Skills, and Subagents around a workspace.'),
          icon: FolderOpen,
          docId: 'usage',
          anchor: '7-workspaces',
        },
        {
          id: 'ai-sessions',
          name: t('aiSessions', 'AI Sessions'),
          summary: t('docsAiSessionsSummary', 'Create, resume, rename, and organize native terminal AI sessions.'),
          icon: Terminal,
          docId: 'usage',
          anchor: '5-ai-sessions',
        },
        {
          id: 'workflows',
          name: t('workflowPresets', 'Workflow Presets'),
          summary: t('docsWorkflowsSummary', 'Bundle directories, tools, environments, MCP, Skills, and prompts.'),
          icon: Waypoints,
          docId: 'usage',
          anchor: '6-workflow-presets',
        },
      ],
    },
    {
      id: 'ai',
      title: t('docsGroupAi', 'AI Capabilities'),
      items: [
        {
          id: 'ai-environments',
          name: t('cliEnvironments', 'AI Terminal Environments'),
          summary: t('docsAiEnvironmentsSummary', 'Manage Claude, Codex, Gemini, and OpenCode providers and active CLI config.'),
          icon: Sparkles,
          docId: 'usage',
          anchor: '4-ai-environments',
        },
        {
          id: 'ai-workspace',
          name: t('aiWorkspaceTitle', 'AI Workspace'),
          summary: t('docsAiWorkspaceSummary', 'Use in-app AI conversations, assistant presets, and Quick Assistant.'),
          icon: Bot,
          docId: 'usage',
          anchor: '8-ai-workspace',
        },
        {
          id: 'ai-usage',
          name: t('aiUsageStatsMenu', 'AI Usage Stats'),
          summary: t('docsAiUsageSummary', 'Review token usage derived from local CLI session history.'),
          icon: BarChart3,
          docId: 'usage',
          anchor: '9-ai-usage-stats',
        },
        {
          id: 'skills',
          name: t('docsSkillsGuide', 'Skills & Subagents Guide'),
          summary: t('docsSkillsGuideSummary', 'Manage Skills and Subagents across models, scopes, sources, and updates.'),
          icon: Sparkles,
          docId: 'skills',
        },
        {
          id: 'mcp',
          name: t('docsMcpGuide', 'MCP Guide'),
          summary: t('docsMcpGuideSummary', 'Configure MCP servers, model switches, and import/export.'),
          icon: Server,
          docId: 'mcp',
        },
      ],
    },
    {
      id: 'tools',
      title: t('docsGroupTools', 'Tools & Integrations'),
      items: [
        {
          id: 'ssh',
          name: t('sshManagement', 'SSH Management'),
          summary: t('docsSshSummary', 'Open SSH servers and manage local, remote, or dynamic SSH tunnels.'),
          icon: Server,
          docId: 'usage',
          anchor: '12-ssh-servers-与-ssh-tunnels',
        },
        {
          id: 'protocol-router',
          name: t('protocolRouter', 'Protocol Router'),
          summary: t('docsProtocolRouterSummary', 'Expose and inspect local protocol routes for AI providers.'),
          icon: Route,
          docId: 'usage',
          anchor: '13-protocol-router',
        },
        {
          id: 'snippets-bookmarks-notes',
          name: t('docsContentTools', 'Snippets, Bookmarks, Notes'),
          summary: t('docsContentToolsSummary', 'Keep local snippets, saved links, project paths, and Markdown notes searchable.'),
          icon: NotebookPen,
          docId: 'usage',
          anchor: '16-snippetsbookmarksnotes',
        },
        {
          id: 'ai-news',
          name: t('aiNews', 'AI News'),
          summary: t('docsAiNewsSummary', 'Fetch AI news from configured RSS sources with local keyword filtering.'),
          icon: Newspaper,
          docId: 'usage',
          anchor: '17-ai-news',
        },
        {
          id: 'mail',
          name: t('mail', 'Mail'),
          summary: t('docsMailSummary', 'Connect Gmail with OAuth to read, reply, and download attachments.'),
          icon: Mail,
          docId: 'usage',
          anchor: '18-mail',
        },
        {
          id: 'cloud-drive',
          name: t('cloudDrive', 'Cloud Drive'),
          summary: t('docsCloudDriveSummary', 'Understand the current experimental Aliyun Cloud Drive browser state.'),
          icon: Cloud,
          docId: 'usage',
          anchor: '19-cloud-drive',
        },
      ],
    },
    {
      id: 'settings',
      title: t('docsGroupSettings', 'Settings & Help'),
      items: [
        {
          id: 'cli',
          name: t('docsCliGuide', 'CLI Guide'),
          summary: t('docsCliGuideSummary', 'Install and use onespace CLI in terminal workflows.'),
          icon: Terminal,
          docId: 'cli',
        },
        {
          id: 'settings',
          name: t('settings', 'Settings'),
          summary: t('docsSettingsSummary', 'Configure storage, news, proxy, shortcuts, terminal commands, appearance, and security.'),
          icon: Code2,
          docId: 'usage',
          anchor: '21-settings',
        },
        {
          id: 'fish-pond',
          name: t('fishPond', 'Fish Pond'),
          summary: t('docsFishPondSummary', 'Find the built-in CyberMuyu, Snake, Tetris, Sudoku, Minesweeper, and Wordle games.'),
          icon: Gamepad2,
          docId: 'usage',
          anchor: '20-fish-pond',
        },
        {
          id: 'faq',
          name: t('faq', 'FAQ'),
          summary: t('docsFaqSummary', 'Troubleshoot common CLI, environment, AI News, Cloud Drive, and macOS issues.'),
          icon: HelpCircle,
          docId: 'usage',
          anchor: '24-常见问题',
        },
      ],
    },
  ];

  const currentDoc = activeDoc ? docEntries.find((entry) => entry.id === activeDoc.docId) || docEntries[0] : null;
  const CurrentIcon = currentDoc?.icon;

  if (activeDoc && currentDoc) {
    return (
      <div className="flex flex-col h-full animate-in fade-in slide-in-from-right-4 duration-300 overflow-hidden">
        {/* Detail Header */}
        <div className="flex items-center gap-4 p-4 border-b bg-muted/20 shrink-0">
          <button 
            onClick={() => setActiveDoc(null)}
            className="p-2 hover:bg-muted rounded-full transition-colors text-muted-foreground hover:text-foreground"
            title={t('backToDocs')}
          >
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div className="flex items-center gap-2 font-bold text-lg">
            {CurrentIcon && (
              <CurrentIcon className="w-5 h-5 text-primary" />
            )}
            {currentDoc.name}
          </div>
        </div>

        {/* Detail Content */}
        <div className="flex-1 overflow-y-auto p-6 md:p-8 max-w-5xl">
          {activeDoc.docId === 'cli' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between flex-wrap gap-4">
                <h2 className="text-3xl font-bold tracking-tight">{t('docsCliGuide', 'CLI Guide')}</h2>
                <button
                  onClick={handleInstall}
                  disabled={loading}
                  className="bg-primary text-primary-foreground hover:bg-primary/90 px-6 py-2.5 rounded-xl flex items-center gap-2 font-bold shadow-lg shadow-primary/20 transition-all disabled:opacity-50"
                >
                  {loading ? <Terminal className="w-5 h-5 animate-pulse" /> : <Download className="w-5 h-5" />}
                  {t('installNow', 'Install CLI')}
                </button>
              </div>

              {message.text && (
                <div className={`p-4 rounded-xl border flex items-center gap-3 animate-in fade-in zoom-in-95 ${
                  message.type === 'error' ? 'bg-destructive/10 border-destructive/20 text-destructive' : 'bg-primary/10 border-primary/20 text-primary'
                }`}>
                  <Info className="w-5 h-5" />
                  <span className="font-medium">{message.text}</span>
                </div>
              )}
            </div>
          )}

          <div className="prose prose-sm dark:prose-invert max-w-none border rounded-2xl bg-card p-6">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={{
                h1: ({ children, ...props }) => <h1 id={slugifyHeading(children)} {...props}>{children}</h1>,
                h2: ({ children, ...props }) => <h2 id={slugifyHeading(children)} {...props}>{children}</h2>,
                h3: ({ children, ...props }) => <h3 id={slugifyHeading(children)} {...props}>{children}</h3>,
              }}
            >
              {currentDoc.content}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-6 md:p-10 animate-in fade-in duration-500">
      <div className="max-w-6xl mx-auto space-y-12">
        <div className="space-y-1">
          <h2 className="text-3xl font-extrabold tracking-tight">{t('usageDocs', 'Documentation')}</h2>
          <p className="text-muted-foreground">{t('docsMenuDesc', 'The content here is rendered from markdown files in the docs directory.')}</p>
        </div>

        <div className="space-y-10">
          {moduleGroups.map((group) => (
            <section key={group.id} className="space-y-4">
              <h3 className="text-sm font-semibold uppercase text-muted-foreground">
                {group.title}
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
                {group.items.map((s) => (
                  <button
                    key={s.id}
                    onClick={() => setActiveDoc({ docId: s.docId, anchor: s.anchor })}
                    className="flex min-h-44 flex-col text-left p-5 bg-card border rounded-lg hover:border-primary/50 hover:shadow-lg hover:shadow-primary/5 transition-all duration-300 group"
                  >
                    <div className="p-2.5 bg-primary/10 rounded-lg w-fit mb-4 group-hover:scale-105 transition-transform duration-300">
                      <s.icon className="w-5 h-5 text-primary" />
                    </div>
                    <h4 className="text-base font-bold mb-2 group-hover:text-primary transition-colors">{s.name}</h4>
                    <p className="text-sm text-muted-foreground leading-relaxed flex-1">
                      {s.summary}
                    </p>
                    <div className="mt-4 flex items-center gap-2 text-primary font-bold text-xs">
                      {t('learnMore', 'Learn More')}
                      <ArrowLeft className="w-3.5 h-3.5 rotate-180 group-hover:translate-x-1 transition-transform" />
                    </div>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
