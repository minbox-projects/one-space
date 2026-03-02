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
  Keyboard
} from 'lucide-react';

export function Documentation() {
  const { t } = useTranslation();
  const [activeSection, setActiveSection] = useState('getting-started');
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const [message, setMessage] = useState({ type: '', text: '' });

  // Handle auto-scroll to CLI if requested via navigation
  useEffect(() => {
    const section = window.location.hash.replace('#', '');
    if (section) {
      setActiveSection(section);
      const el = document.getElementById(section);
      el?.scrollIntoView({ behavior: 'smooth' });
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
    { id: 'getting-started', name: t('gettingStarted', 'Getting Started'), icon: BookOpen },
    { id: 'cli', name: t('cliInstallation', 'CLI Installation'), icon: Terminal },
    { id: 'ai-sessions', name: t('aiSessionsDocs', 'AI Terminal Sessions'), icon: Zap },
    { id: 'ai-env', name: t('aiEnvDocs', 'AI Environments'), icon: Cpu },
    { id: 'shortcuts', name: t('shortcuts', 'Global Shortcuts'), icon: Keyboard },
    { id: 'ssh', name: t('sshManagement', 'SSH Management'), icon: Server },
  ];

  return (
    <div className="flex h-full gap-8 max-w-6xl mx-auto p-2">
      {/* Mini Sidebar */}
      <div className="w-56 shrink-0 space-y-1 sticky top-0 h-fit">
        <div className="px-3 py-4">
          <h3 className="text-sm font-bold uppercase tracking-widest text-muted-foreground/60">{t('usageDocs', 'Documentation')}</h3>
        </div>
        {sections.map(s => (
          <button
            key={s.id}
            onClick={() => {
              setActiveSection(s.id);
              document.getElementById(s.id)?.scrollIntoView({ behavior: 'smooth' });
            }}
            className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-all ${
              activeSection === s.id ? 'bg-primary/10 text-primary font-semibold' : 'hover:bg-muted text-muted-foreground hover:text-foreground'
            }`}
          >
            <s.icon className="w-4 h-4" />
            {s.name}
          </button>
        ))}
      </div>

      {/* Main Content */}
      <div className="flex-1 space-y-16 pb-20 overflow-y-auto pr-4 scroll-smooth">
        
        {/* Getting Started */}
        <section id="getting-started" className="space-y-6 pt-4">
          <div className="space-y-2">
            <h2 className="text-3xl font-bold tracking-tight">{t('philosophy', 'OneSpace Philosophy')}</h2>
            <p className="text-lg text-muted-foreground leading-relaxed">
              {t('philosophyDesc', 'OneSpace is your unified portal for high-precision digital workflows.')}
            </p>
          </div>
          
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="bg-card border rounded-xl p-6 space-y-3">
              <div className="p-2 bg-primary/10 rounded-lg w-fit"><ShieldCheck className="w-6 h-6 text-primary" /></div>
              <h4 className="font-bold text-lg">{t('secureByDefault', 'Secure by Default')}</h4>
              <p className="text-sm text-muted-foreground leading-relaxed">{t('secureByDefaultDesc', 'All your credentials are stored locally.')}</p>
            </div>
            <div className="bg-card border rounded-xl p-6 space-y-3">
              <div className="p-2 bg-primary/10 rounded-lg w-fit"><Zap className="w-6 h-6 text-primary" /></div>
              <h4 className="font-bold text-lg">{t('instantConnectivity', 'Instant Connectivity')}</h4>
              <p className="text-sm text-muted-foreground leading-relaxed">{t('instantConnectivityDesc', 'Access terminal assistants anywhere.')}</p>
            </div>
          </div>
        </section>

        {/* CLI Section */}
        <section id="cli" className="space-y-8 scroll-mt-10">
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <h2 className="text-3xl font-bold tracking-tight">{t('cliInstallation', 'CLI Installation')}</h2>
              <p className="text-muted-foreground">{t('cliInstallationDesc', 'Bridge your terminal with OneSpace AI environments.')}</p>
            </div>
            <button
              onClick={handleInstall}
              disabled={loading}
              className="bg-primary text-primary-foreground hover:bg-primary/90 px-6 py-2.5 rounded-lg flex items-center gap-2 font-semibold shadow-sm transition-all disabled:opacity-50"
            >
              {loading ? <Terminal className="w-5 h-5 animate-pulse" /> : <Download className="w-5 h-5" />}
              {t('installNow', 'Install CLI')}
            </button>
          </div>

          {message.text && (
            <div className={`p-4 rounded-lg border flex items-center gap-3 animate-in fade-in zoom-in-95 ${
              message.type === 'error' ? 'bg-destructive/10 border-destructive/20 text-destructive' : 'bg-primary/10 border-primary/20 text-primary'
            }`}>
              <Info className="w-5 h-5" />
              <span className="font-medium">{message.text}</span>
            </div>
          )}

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {[
              { id: 'claude', title: 'Claude Code', cmd: 'onespace ai claude', desc: 'Launch Claude Code in the current path.' },
              { id: 'gemini', title: 'Gemini', cmd: 'onespace ai gemini', desc: 'Connect to Google Gemini CLI.' },
              { id: 'codex', title: 'Codex / OpenAI', cmd: 'onespace ai codex session_name', desc: 'OpenAI compatible session manager.' },
              { id: 'opencode', title: 'OpenCode', cmd: 'onespace ai opencode', desc: 'Standard OpenCode AI terminal.' },
            ].map(ex => (
              <div key={ex.id} className="bg-card border rounded-xl p-5 shadow-sm hover:shadow-md transition-shadow group">
                <div className="flex items-center justify-between mb-3">
                  <h3 className="font-bold flex items-center gap-2">
                    <Code className="w-4 h-4 text-primary" />
                    {ex.title}
                  </h3>
                </div>
                <p className="text-xs text-muted-foreground mb-4 leading-relaxed">{ex.desc}</p>
                <div className="relative flex items-center bg-muted/50 rounded-lg p-3 group-hover:bg-muted transition-colors border border-transparent group-hover:border-primary/20">
                  <code className="text-xs font-mono font-medium flex-1 truncate pr-8">{ex.cmd}</code>
                  <button onClick={() => handleCopy(ex.cmd)} className="absolute right-2 p-1.5 hover:bg-background rounded-md transition-colors text-muted-foreground hover:text-primary">
                    {copied === ex.cmd ? <Check className="w-4 h-4 text-green-500" /> : <Copy className="w-4 h-4" />}
                  </button>
                </div>
              </div>
            ))}
          </div>

          <div className="bg-muted/30 border border-dashed rounded-xl p-6">
            <h4 className="font-semibold flex items-center gap-2 mb-3">
              <Info className="w-4 h-4 text-primary" />
              {t('importantNote', 'Configuration Tip')}
            </h4>
            <p className="text-sm text-muted-foreground leading-relaxed">
              {t('pathTip', 'Ensure {{path}} is in your system PATH. Add this to your .zshrc or .bashrc:', { path: '~/.local/bin' })}
            </p>
            <div className="mt-3 bg-background border rounded p-3 font-mono text-xs">
              export PATH="$HOME/.local/bin:$PATH"
            </div>
          </div>
        </section>

        {/* Shortcuts Section */}
        <section id="shortcuts" className="space-y-6 scroll-mt-10">
          <h2 className="text-3xl font-bold tracking-tight">{t('shortcuts', 'Global Shortcuts')}</h2>
          <div className="prose prose-sm prose-neutral dark:prose-invert max-w-none">
            <p>{t('shortcutsIntro', 'OneSpace allows you to trigger its core functions from any application using customizable keyboard shortcuts.')}</p>
            <ul className="grid grid-cols-1 md:grid-cols-2 gap-4 list-none p-0">
              <li className="bg-muted/30 p-4 rounded-xl border m-0">
                <div className="font-bold mb-1 flex items-center gap-2"><Keyboard className="w-4 h-4" /> {t('toggleMainWindow', 'Main Window Toggle')}</div>
                <div className="text-xs text-muted-foreground">{t('toggleMainWindowDesc', 'Quickly show or hide the OneSpace dashboard.')} {t('default', 'Default')}: <code>Alt + Space</code></div>
              </li>
              <li className="bg-muted/30 p-4 rounded-xl border m-0">
                <div className="font-bold mb-1 flex items-center gap-2"><Terminal className="w-4 h-4" /> {t('toggleQuickAi', 'Quick AI Session')}</div>
                <div className="text-xs text-muted-foreground">{t('toggleQuickAiDesc', 'Open the Spotlight-style AI command bar.')} {t('default', 'Default')}: <code>Alt + Shift + A</code></div>
              </li>
            </ul>
          </div>
        </section>

      </div>
    </div>
  );
}
