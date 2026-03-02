import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Download, Copy, Check, Terminal, Code, Info } from 'lucide-react';

export function CliInstallation() {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const [message, setMessage] = useState({ type: '', text: '' });

  const examples = [
    {
      id: 'claude',
      title: 'Claude Code',
      desc: 'Start a new AI session using Claude Code in the current directory.',
      cmd: 'onespace ai claude'
    },
    {
      id: 'gemini',
      title: 'Gemini',
      desc: 'Start a session using Google Gemini.',
      cmd: 'onespace ai gemini my_project'
    },
    {
      id: 'codex',
      title: 'Codex / OpenAI',
      desc: 'Connect using OpenAI-compatible CLI.',
      cmd: 'onespace ai codex project_name'
    },
    {
      id: 'opencode',
      title: 'OpenCode',
      desc: 'Start an OpenCode AI session with custom arguments.',
      cmd: 'onespace ai opencode --fast'
    }
  ];

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

  return (
    <div className="max-w-4xl mx-auto space-y-8 animate-in fade-in duration-500">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">{t('cliInstallation', 'CLI Installation & Usage')}</h2>
          <p className="text-muted-foreground mt-1">
            Install the <code>onespace</code> command-line tool to bridge your terminal with AI environments.
          </p>
        </div>
        <button
          onClick={handleInstall}
          disabled={loading}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-6 py-2.5 rounded-lg flex items-center gap-2 font-semibold shadow-sm transition-all disabled:opacity-50"
        >
          {loading ? <Terminal className="w-5 h-5 animate-pulse" /> : <Download className="w-5 h-5" />}
          {t('installNow', 'Install / Update CLI')}
        </button>
      </div>

      {message.text && (
        <div className={`p-4 rounded-lg border flex items-center gap-3 ${
          message.type === 'error' ? 'bg-destructive/10 border-destructive/20 text-destructive' : 'bg-primary/10 border-primary/20 text-primary'
        }`}>
          <Info className="w-5 h-5" />
          <span className="font-medium">{message.text}</span>
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {examples.map((ex) => (
          <div key={ex.id} className="bg-card border rounded-xl p-5 shadow-sm hover:shadow-md transition-shadow group">
            <div className="flex items-center justify-between mb-3">
              <h3 className="font-bold flex items-center gap-2 text-lg">
                <Code className="w-5 h-5 text-primary" />
                {ex.title}
              </h3>
              <span className="text-[10px] uppercase bg-muted px-2 py-0.5 rounded font-bold tracking-widest opacity-50">Example</span>
            </div>
            <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
              {ex.desc}
            </p>
            <div className="relative flex items-center bg-muted/50 rounded-lg p-3 group-hover:bg-muted transition-colors border border-transparent group-hover:border-primary/20">
              <code className="text-xs font-mono font-medium flex-1 truncate pr-8">{ex.cmd}</code>
              <button
                onClick={() => handleCopy(ex.cmd)}
                className="absolute right-2 p-1.5 hover:bg-background rounded-md transition-colors text-muted-foreground hover:text-primary"
                title="Copy command"
              >
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
          Ensure <code>~/.local/bin</code> is in your system <code>PATH</code>. You can add it to your <code>.zshrc</code> or <code>.bashrc</code>:
        </p>
        <div className="mt-3 bg-background border rounded p-3 font-mono text-xs">
          export PATH="$HOME/.local/bin:$PATH"
        </div>
      </div>
    </div>
  );
}
