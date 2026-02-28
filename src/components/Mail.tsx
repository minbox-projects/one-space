import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Mail as MailIcon, Inbox, PenSquare, Send, RefreshCw, Key, AtSign, Loader2, LogOut } from 'lucide-react';
import { formatDistanceToNow } from 'date-fns';

interface Email {
  id: string;
  from: string;
  subject: string;
  snippet: string;
  date: string;
  isRead: boolean;
}

export function Mail() {
  const { t } = useTranslation();
  
  const [emailAddress, setEmailAddress] = useState('');
  const [appPassword, setAppPassword] = useState('');
  const [isConnected, setIsConnected] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const [activeView, setActiveView] = useState<'inbox' | 'compose'>('inbox');
  const [emails, setEmails] = useState<Email[]>([]);
  
  // Compose state
  const [to, setTo] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');

  useEffect(() => {
    const savedEmail = localStorage.getItem('onespace_gmail_address');
    const savedPass = localStorage.getItem('onespace_gmail_app_password');
    if (savedEmail && savedPass) {
      setEmailAddress(savedEmail);
      setAppPassword(savedPass);
      setIsConnected(true);
      fetchEmails();
    }
  }, []);

  const handleConnect = async () => {
    if (!emailAddress || !appPassword) return;
    setLoading(true);
    setError(null);
    
    try {
      // MOCK: Verify IMAP/SMTP connection
      await new Promise(r => setTimeout(r, 1200));
      
      localStorage.setItem('onespace_gmail_address', emailAddress);
      localStorage.setItem('onespace_gmail_app_password', appPassword);
      setIsConnected(true);
      fetchEmails();
    } catch (err: any) {
      setError("Authentication failed. Check your app password.");
    } finally {
      setLoading(false);
    }
  };

  const handleDisconnect = () => {
    localStorage.removeItem('onespace_gmail_address');
    localStorage.removeItem('onespace_gmail_app_password');
    setEmailAddress('');
    setAppPassword('');
    setIsConnected(false);
    setEmails([]);
  };

  const fetchEmails = async () => {
    setLoading(true);
    
    try {
      // MOCK: Fetch from IMAP
      await new Promise(r => setTimeout(r, 1000));
      
      const mockEmails: Email[] = [
        {
          id: '1',
          from: 'GitHub <noreply@github.com>',
          subject: '[minbox-projects/one-space] Run failed: tests',
          snippet: 'Run failed for tests on main branch. Please check the logs...',
          date: new Date().toISOString(),
          isRead: false
        },
        {
          id: '2',
          from: 'Google Security <no-reply@accounts.google.com>',
          subject: 'Security alert',
          snippet: 'Your app password was created successfully...',
          date: new Date(Date.now() - 3600000).toISOString(),
          isRead: true
        },
        {
          id: '3',
          from: 'Vercel <notifications@vercel.com>',
          subject: 'Deployment successful',
          snippet: 'Your recent deployment for onespace-app has completed successfully.',
          date: new Date(Date.now() - 86400000).toISOString(),
          isRead: true
        }
      ];
      
      setEmails(mockEmails);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  const handleSend = async () => {
    if (!to || !subject) return;
    setLoading(true);
    
    try {
      // MOCK: Send via SMTP
      await new Promise(r => setTimeout(r, 1500));
      alert('Email sent successfully!');
      setTo('');
      setSubject('');
      setBody('');
      setActiveView('inbox');
    } catch (err) {
      alert('Failed to send email.');
    } finally {
      setLoading(false);
    }
  };

  if (!isConnected) {
    return (
      <div className="flex flex-col items-center justify-center h-full max-w-md mx-auto space-y-6">
        <div className="bg-red-500/10 p-4 rounded-full">
          <MailIcon className="w-12 h-12 text-red-500" />
        </div>
        <div className="text-center space-y-2">
          <h2 className="text-2xl font-bold tracking-tight">{t('connectGmail')}</h2>
          <p className="text-muted-foreground">{t('manageMail')}</p>
        </div>

        <div className="w-full bg-card border rounded-xl p-6 shadow-sm space-y-4">
          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('gmailEmailAddress')}</label>
            <div className="relative">
              <AtSign className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
              <input 
                type="email" 
                placeholder={t('gmailEmailAddressPlaceholder')} 
                value={emailAddress}
                onChange={(e) => setEmailAddress(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>
          </div>

          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t('gmailAppPassword')}</label>
            <div className="relative">
              <Key className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
              <input 
                type="password" 
                placeholder={t('gmailAppPasswordPlaceholder')} 
                value={appPassword}
                onChange={(e) => setAppPassword(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>
            <p className="text-xs text-muted-foreground/70 text-right mt-1">
              <a href="https://myaccount.google.com/apppasswords" target="_blank" rel="noreferrer" className="hover:underline hover:text-primary transition-colors">
                {t('howToGetAppPassword')}
              </a>
            </p>
          </div>

          {error && <p className="text-sm text-destructive">{error}</p>}

          <button 
            onClick={handleConnect}
            disabled={!emailAddress || !appPassword || loading}
            className="w-full bg-red-600 text-white hover:bg-red-700 h-10 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
          >
            {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : <MailIcon className="w-4 h-4" />}
            {t('saveToken')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full space-y-4">
      <div className="flex items-center justify-between shrink-0">
        <div>
          <h2 className="text-xl font-bold tracking-tight flex items-center gap-2">
            <MailIcon className="w-5 h-5 text-red-500" />
            {emailAddress}
          </h2>
          <p className="text-sm text-muted-foreground mt-1 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-green-500"></span>
            Connected via IMAP/SMTP
          </p>
        </div>
        
        <div className="flex items-center gap-2">
          <div className="flex bg-muted/50 p-1 rounded-lg mr-2">
            <button 
              onClick={() => setActiveView('inbox')}
              className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${activeView === 'inbox' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <Inbox className="w-4 h-4 inline-block mr-1.5" />
              {t('inbox')}
            </button>
            <button 
              onClick={() => setActiveView('compose')}
              className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors ${activeView === 'compose' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
            >
              <PenSquare className="w-4 h-4 inline-block mr-1.5" />
              {t('compose')}
            </button>
          </div>
          
          <button
            onClick={handleDisconnect}
            className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 p-2 rounded-md transition-colors"
            title={t('disconnect')}
          >
            <LogOut className="w-4 h-4" />
          </button>
        </div>
      </div>

      <div className="flex-1 bg-card border rounded-xl shadow-sm flex flex-col overflow-hidden relative">
        {activeView === 'inbox' ? (
          <>
            {/* Inbox Header */}
            <div className="h-12 border-b bg-muted/10 flex items-center px-4 justify-between">
              <span className="text-sm font-medium">{t('inbox')}</span>
              <button 
                onClick={fetchEmails}
                className="text-muted-foreground hover:text-primary transition-colors p-1"
                title={t('refresh')}
              >
                <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
              </button>
            </div>
            
            {/* Email List */}
            <div className="flex-1 overflow-y-auto">
              {loading && emails.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
                  <Loader2 className="w-8 h-8 animate-spin mb-2" />
                  <p>{t('loading')}</p>
                </div>
              ) : emails.length === 0 ? (
                <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
                  <Inbox className="w-12 h-12 mb-3 opacity-20" />
                  <p>{t('noEmails')}</p>
                </div>
              ) : (
                <div className="divide-y divide-border/50">
                  {emails.map(email => (
                    <div key={email.id} className={`p-4 hover:bg-muted/30 cursor-pointer transition-colors ${!email.isRead ? 'bg-primary/5' : ''}`}>
                      <div className="flex justify-between items-start mb-1">
                        <span className={`font-medium ${!email.isRead ? 'text-foreground' : 'text-foreground/80'}`}>
                          {email.from}
                        </span>
                        <span className="text-xs text-muted-foreground">
                          {formatDistanceToNow(new Date(email.date), { addSuffix: true })}
                        </span>
                      </div>
                      <h4 className={`text-sm mb-1 ${!email.isRead ? 'font-bold' : 'font-medium'}`}>{email.subject}</h4>
                      <p className="text-xs text-muted-foreground truncate">{email.snippet}</p>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        ) : (
          <div className="flex flex-col h-full p-6 gap-4">
            <input 
              type="email" 
              placeholder={t('to')}
              value={to}
              onChange={(e) => setTo(e.target.value)}
              className="flex w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
            <input 
              type="text" 
              placeholder={t('subject')}
              value={subject}
              onChange={(e) => setSubject(e.target.value)}
              className="flex w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring font-medium"
            />
            <textarea 
              placeholder={t('body')}
              value={body}
              onChange={(e) => setBody(e.target.value)}
              className="flex-1 w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-none font-mono"
            />
            <div className="flex justify-end pt-2">
              <button 
                onClick={handleSend}
                disabled={!to || !subject || loading}
                className="bg-red-600 text-white hover:bg-red-700 px-6 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50 flex items-center gap-2"
              >
                {loading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Send className="w-4 h-4" />}
                {t('send')}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}