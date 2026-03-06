import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { 
  BookOpen, 
  Terminal, 
  Server, 
  Sparkles,
  Download, 
  Info,
  ArrowLeft
} from 'lucide-react';
import usageDoc from '../../docs/USAGE.md?raw';
import cliDoc from '../../docs/CLI.md?raw';
import skillsDoc from '../../docs/SKILLS.md?raw';
import mcpDoc from '../../docs/MCP.md?raw';

export function Documentation() {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });

  // Handle direct navigation to sections via hash or state
  useEffect(() => {
    const section = window.location.hash.replace('#', '');
    if (section) {
      setActiveSection(section);
      window.location.hash = ''; // Clear hash after reading
    }
  }, []);

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

  const sections = [
    { 
      id: 'usage', 
      name: t('docsUsageGuide', 'Usage Manual'), 
      icon: BookOpen, 
      summary: t('docsUsageGuideSummary', 'Complete OneSpace user manual and feature map.') 
    },
    { 
      id: 'cli', 
      name: t('docsCliGuide', 'CLI Guide'), 
      icon: Terminal, 
      summary: t('docsCliGuideSummary', 'Install and use onespace CLI in terminal workflows.') 
    },
    { 
      id: 'skills', 
      name: t('docsSkillsGuide', 'Skills Guide'), 
      icon: Sparkles, 
      summary: t('docsSkillsGuideSummary', 'Install, sync, import, and update Skills by model.') 
    },
    { 
      id: 'mcp', 
      name: t('docsMcpGuide', 'MCP Guide'), 
      icon: Server, 
      summary: t('docsMcpGuideSummary', 'Configure MCP servers, model switches, and import/export.') 
    },
  ];

  const docsBySection: Record<string, string> = {
    usage: usageDoc,
    cli: cliDoc,
    skills: skillsDoc,
    mcp: mcpDoc,
  };

  if (activeSection) {
    return (
      <div className="flex flex-col h-full animate-in fade-in slide-in-from-right-4 duration-300 overflow-hidden">
        {/* Detail Header */}
        <div className="flex items-center gap-4 p-4 border-b bg-muted/20 shrink-0">
          <button 
            onClick={() => setActiveSection(null)}
            className="p-2 hover:bg-muted rounded-full transition-colors text-muted-foreground hover:text-foreground"
            title={t('backToDocs')}
          >
            <ArrowLeft className="w-5 h-5" />
          </button>
          <div className="flex items-center gap-2 font-bold text-lg">
            {sections.find(s => s.id === activeSection)?.icon && (
              <div className="text-primary">
                {(() => {
                  const Icon = sections.find(s => s.id === activeSection)?.icon;
                  return Icon ? <Icon className="w-5 h-5" /> : null;
                })()}
              </div>
            )}
            {sections.find(s => s.id === activeSection)?.name}
          </div>
        </div>

        {/* Detail Content */}
        <div className="flex-1 overflow-y-auto p-6 md:p-8 max-w-5xl">
          {activeSection === 'cli' && (
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
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {docsBySection[activeSection] || usageDoc}
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

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {sections.map((s) => (
            <button
              key={s.id}
              onClick={() => setActiveSection(s.id)}
              className="flex flex-col text-left p-6 bg-card border rounded-3xl hover:border-primary/50 hover:shadow-xl hover:shadow-primary/5 transition-all duration-300 group"
            >
              <div className="p-3 bg-primary/10 rounded-2xl w-fit mb-5 group-hover:scale-110 transition-transform duration-300">
                <s.icon className="w-6 h-6 text-primary" />
              </div>
              <h3 className="text-xl font-bold mb-2 group-hover:text-primary transition-colors">{s.name}</h3>
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
      </div>
    </div>
  );
}
