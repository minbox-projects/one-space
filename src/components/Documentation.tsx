import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { 
  BookOpen, 
  Terminal, 
  Cpu, 
  Server, 
  Code, 
  Download, 
  Copy, 
  Check, 
  Info,
  ShieldCheck,
  Zap,
  Keyboard,
  ArrowLeft
} from 'lucide-react';

export function Documentation() {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const [message, setMessage] = useState({ type: '', text: '' });

  // Handle direct navigation to sections via hash or state
  useEffect(() => {
    const section = window.location.hash.replace('#', '');
    if (section) {
      setActiveSection(section);
      window.location.hash = ''; // Clear hash after reading
    }
  }, []);

  const handleCopy = (cmd: string) => {
    navigator.clipboard.writeText(cmd);
    setCopied(cmd);
    setTimeout(() => setCopied(null), 2000);
  };

  const handleInstall = async () => {
    try {
      setLoading(true);
      await invoke('install_cli');
      setMessage({ type: 'success', text: t('cliInstalled', 'CLI tool installed to ~/.local/bin/onespace') });
    } catch (err: any) {
      setMessage({ type: 'error', text: err.toString() });
    } finally {
      setLoading(false);
    }
  };

  const sections = [
    { 
      id: 'getting-started', 
      name: t('gettingStarted', 'Getting Started'), 
      icon: BookOpen, 
      summary: t('gettingStartedSummary', 'Learn the core philosophy and basic usage of OneSpace.') 
    },
    { 
      id: 'cli', 
      name: t('cliInstallation', 'CLI Installation'), 
      icon: Terminal, 
      summary: t('cliSummary', 'Bridge your terminal with AI. Installation and command examples.') 
    },
    { 
      id: 'ai-sessions', 
      name: t('aiSessionsDocs', 'AI Terminal Sessions'), 
      icon: Zap, 
      summary: t('aiSessionsSummary', 'Manage and attach to your terminal-based AI assistants.') 
    },
    { 
      id: 'ai-env', 
      name: t('aiEnvDocs', 'AI Environments'), 
      icon: Cpu, 
      summary: t('aiEnvSummary', 'Configure API keys, models, and endpoints for AI tools.') 
    },
    { 
      id: 'shortcuts', 
      name: t('shortcuts', 'Global Shortcuts'), 
      icon: Keyboard, 
      summary: t('shortcutsSummary', 'Master the global hotkeys to trigger OneSpace from anywhere.') 
    },
    { 
      id: 'ssh', 
      name: t('sshManagement', 'SSH Management'), 
      icon: Server, 
      summary: t('sshSummary', 'Quickly connect to your remote servers via SSH.') 
    },
  ];

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
        <div className="flex-1 overflow-y-auto p-6 md:p-10 max-w-5xl">
          {activeSection === 'getting-started' && (
            <div className="space-y-10">
              <div className="space-y-4">
                <h2 className="text-4xl font-bold tracking-tight">{t('philosophy', 'OneSpace Philosophy')}</h2>
                <p className="text-xl text-muted-foreground leading-relaxed">
                  {t('philosophyDesc', 'OneSpace is your unified portal for high-precision digital workflows.')}
                </p>
              </div>
              
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div className="bg-card border rounded-2xl p-8 space-y-4 shadow-sm">
                  <div className="p-3 bg-primary/10 rounded-xl w-fit"><ShieldCheck className="w-8 h-8 text-primary" /></div>
                  <h4 className="font-bold text-xl">{t('secureByDefault', 'Secure by Default')}</h4>
                  <p className="text-muted-foreground leading-relaxed">{t('secureByDefaultDesc', 'All your credentials are stored locally.')}</p>
                </div>
                <div className="bg-card border rounded-2xl p-8 space-y-4 shadow-sm">
                  <div className="p-3 bg-primary/10 rounded-xl w-fit"><Zap className="w-8 h-8 text-primary" /></div>
                  <h4 className="font-bold text-xl">{t('instantConnectivity', 'Instant Connectivity')}</h4>
                  <p className="text-muted-foreground leading-relaxed">{t('instantConnectivityDesc', 'Access terminal assistants anywhere.')}</p>
                </div>
              </div>
            </div>
          )}

          {activeSection === 'cli' && (
            <div className="space-y-10">
              <div className="flex items-center justify-between flex-wrap gap-4">
                <div className="space-y-2">
                  <h2 className="text-4xl font-bold tracking-tight">{t('cliInstallation', 'CLI Installation')}</h2>
                  <p className="text-xl text-muted-foreground">{t('cliInstallationDesc', 'Bridge your terminal with OneSpace AI environments.')}</p>
                </div>
                <button
                  onClick={handleInstall}
                  disabled={loading}
                  className="bg-primary text-primary-foreground hover:bg-primary/90 px-8 py-3 rounded-xl flex items-center gap-2 font-bold shadow-lg shadow-primary/20 transition-all disabled:opacity-50"
                >
                  {loading ? <Terminal className="w-6 h-6 animate-pulse" /> : <Download className="w-6 h-6" />}
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

              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                {[
                  { id: 'claude', title: 'Claude Code', cmd: 'onespace ai claude', desc: 'Launch Claude Code in the current path.' },
                  { id: 'gemini', title: 'Gemini', cmd: 'onespace ai gemini', desc: 'Connect to Google Gemini CLI.' },
                  { id: 'codex', title: 'Codex / OpenAI', cmd: 'onespace ai codex session_name', desc: 'OpenAI compatible session manager.' },
                  { id: 'opencode', title: 'OpenCode', cmd: 'onespace ai opencode', desc: 'Standard OpenCode AI terminal.' },
                ].map(ex => (
                  <div key={ex.id} className="bg-card border rounded-2xl p-6 shadow-sm hover:shadow-md transition-shadow group">
                    <div className="flex items-center justify-between mb-4">
                      <h3 className="font-bold flex items-center gap-2 text-lg">
                        <Code className="w-5 h-5 text-primary" />
                        {ex.title}
                      </h3>
                    </div>
                    <p className="text-sm text-muted-foreground mb-6 leading-relaxed">{ex.desc}</p>
                    <div className="relative flex items-center bg-muted/50 rounded-xl p-4 group-hover:bg-muted transition-colors border border-transparent group-hover:border-primary/20">
                      <code className="text-xs font-mono font-medium flex-1 truncate pr-10">{ex.cmd}</code>
                      <button onClick={() => handleCopy(ex.cmd)} className="absolute right-3 p-2 hover:bg-background rounded-lg transition-colors text-muted-foreground hover:text-primary">
                        {copied === ex.cmd ? <Check className="w-5 h-5 text-green-500" /> : <Copy className="w-5 h-5" />}
                      </button>
                    </div>
                  </div>
                ))}
              </div>

              <div className="bg-muted/30 border border-dashed rounded-2xl p-8">
                <h4 className="font-bold text-lg flex items-center gap-2 mb-4">
                  <Info className="w-5 h-5 text-primary" />
                  {t('importantNote', 'Configuration Tip')}
                </h4>
                <p className="text-muted-foreground leading-relaxed mb-4">
                  {t('pathTip', 'Ensure {{path}} is in your system PATH.', { path: '~/.local/bin' })}
                </p>
                <div className="bg-background border rounded-xl p-4 font-mono text-sm shadow-inner">
                  export PATH="$HOME/.local/bin:$PATH"
                </div>
              </div>
            </div>
          )}

          {activeSection === 'shortcuts' && (
            <div className="space-y-10">
              <h2 className="text-4xl font-bold tracking-tight">{t('shortcuts', 'Global Shortcuts')}</h2>
              <div className="space-y-8">
                <p className="text-xl text-muted-foreground">{t('shortcutsIntro', 'OneSpace allows you to trigger its core functions from any application.')}</p>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                  <div className="bg-muted/30 p-8 rounded-2xl border flex flex-col justify-between space-y-4">
                    <div className="space-y-2">
                      <div className="font-bold text-xl flex items-center gap-2 text-primary"><Keyboard className="w-6 h-6" /> {t('toggleMainWindow', 'Main Window')}</div>
                      <p className="text-muted-foreground leading-relaxed">{t('toggleMainWindowDesc', 'Quickly show or hide the dashboard.')}</p>
                    </div>
                    <div className="text-sm font-bold bg-background w-fit px-4 py-2 rounded-lg border shadow-sm">
                      {t('default', 'Default')}: <code className="text-primary">Alt + Space</code>
                    </div>
                  </div>
                  <div className="bg-muted/30 p-8 rounded-2xl border flex flex-col justify-between space-y-4">
                    <div className="space-y-2">
                      <div className="font-bold text-xl flex items-center gap-2 text-primary"><Terminal className="w-6 h-6" /> {t('toggleQuickAi', 'Quick AI Session')}</div>
                      <p className="text-muted-foreground leading-relaxed">{t('toggleQuickAiDesc', 'Open the Spotlight-style AI command bar.')}</p>
                    </div>
                    <div className="text-sm font-bold bg-background w-fit px-4 py-2 rounded-lg border shadow-sm">
                      {t('default', 'Default')}: <code className="text-primary">Alt + Shift + A</code>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeSection === 'ai-sessions' && (
            <div className="space-y-6">
              <h2 className="text-4xl font-bold tracking-tight">{t('aiSessionsDocs')}</h2>
              <p className="text-xl text-muted-foreground leading-relaxed">{t('aiSessionsSummary')}</p>
              <div className="p-8 bg-muted/20 border border-dashed rounded-2xl text-center">
                <Info className="w-10 h-10 text-muted-foreground/30 mx-auto mb-4" />
                <p className="text-muted-foreground">Detailed guide for session management is coming soon.</p>
              </div>
            </div>
          )}

          {activeSection === 'ai-env' && (
            <div className="space-y-6">
              <h2 className="text-4xl font-bold tracking-tight">{t('aiEnvDocs')}</h2>
              <p className="text-xl text-muted-foreground leading-relaxed">{t('aiEnvSummary')}</p>
              <div className="p-8 bg-muted/20 border border-dashed rounded-2xl text-center">
                <Info className="w-10 h-10 text-muted-foreground/30 mx-auto mb-4" />
                <p className="text-muted-foreground">Detailed guide for environments is coming soon.</p>
              </div>
            </div>
          )}

          {activeSection === 'ssh' && (
            <div className="space-y-6">
              <h2 className="text-4xl font-bold tracking-tight">{t('sshManagement')}</h2>
              <p className="text-xl text-muted-foreground leading-relaxed">{t('sshSummary')}</p>
              <div className="p-8 bg-muted/20 border border-dashed rounded-2xl text-center">
                <Info className="w-10 h-10 text-muted-foreground/30 mx-auto mb-4" />
                <p className="text-muted-foreground">Detailed guide for SSH servers is coming soon.</p>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-6 md:p-10 animate-in fade-in duration-500">
      <div className="max-w-6xl mx-auto space-y-12">
        <div className="space-y-2">
          <h2 className="text-4xl font-extrabold tracking-tight">{t('usageDocs', 'Documentation')}</h2>
          <p className="text-xl text-muted-foreground">Everything you need to know about OneSpace.</p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {sections.map((s) => (
            <button
              key={s.id}
              onClick={() => setActiveSection(s.id)}
              className="flex flex-col text-left p-8 bg-card border rounded-3xl hover:border-primary/50 hover:shadow-xl hover:shadow-primary/5 transition-all duration-300 group"
            >
              <div className="p-4 bg-primary/10 rounded-2xl w-fit mb-6 group-hover:scale-110 transition-transform duration-300">
                <s.icon className="w-8 h-8 text-primary" />
              </div>
              <h3 className="text-2xl font-bold mb-3 group-hover:text-primary transition-colors">{s.name}</h3>
              <p className="text-muted-foreground leading-relaxed flex-1">
                {s.summary}
              </p>
              <div className="mt-6 flex items-center gap-2 text-primary font-bold text-sm">
                {t('learnMore', 'Learn More')}
                <ArrowLeft className="w-4 h-4 rotate-180 group-hover:translate-x-1 transition-transform" />
              </div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
