import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Mail as MailIcon, Inbox, PenSquare, Send, RefreshCw, Key, LogOut, Loader2, ShieldCheck } from 'lucide-react';
import { formatDistanceToNow } from 'date-fns';
import { invoke } from '@tauri-apps/api/core';
import { 
  getValidAccessToken, 
  saveGmailTokens, 
  saveGmailConfig, 
  getGmailConfig, 
  clearGmailSession
} from '../lib/gmail';

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
  
  const [isConnected, setIsConnected] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // Config state
  const [clientId, setClientId] = useState('');
  const [clientSecret, setClientSecret] = useState('');
  
  const [activeView, setActiveView] = useState<'inbox' | 'compose'>('inbox');
  const [emails, setEmails] = useState<Email[]>([]);
  
  // Compose state
  const [to, setTo] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');

  useEffect(() => {
    checkConnection();
  }, []);

  const checkConnection = async () => {
    const config = getGmailConfig();
    if (config) {
      setClientId(config.clientId);
      setClientSecret(config.clientSecret);
      const token = await getValidAccessToken();
      if (token) {
        setIsConnected(true);
        fetchEmails();
      }
    }
  };

  const handleConnect = async () => {
    if (!clientId || !clientSecret) {
      setError("Client ID and Secret are required.");
      return;
    }
    
    setLoading(true);
    setError(null);
    
    try {
      const scope = "https://www.googleapis.com/auth/gmail.modify";
      
      // 1. Start OAuth flow and get Authorization Code
      const result: { code: string, redirect_uri: string } = await invoke('start_google_oauth', { 
        clientId, 
        scope 
      });
      
      // 2. Exchange code for tokens
      const tokenResponse: string = await invoke('exchange_google_token', {
        code: result.code,
        clientId,
        clientSecret,
        redirectUri: result.redirect_uri
      });

      const tokens = JSON.parse(tokenResponse);
      
      // 3. Save everything
      saveGmailConfig({ clientId, clientSecret });
      saveGmailTokens(tokens);
      
      setIsConnected(true);
      fetchEmails();
    } catch (err: any) {
      console.error(err);
      setError("Authentication failed. " + (typeof err === 'string' ? err : err.message || "Unknown error"));
    } finally {
      setLoading(false);
    }
  };

  const handleDisconnect = () => {
    clearGmailSession();
    setIsConnected(false);
    setEmails([]);
    // Keep clientId/secret in state for easy re-login, but could clear them if desired
  };

  const fetchEmails = async () => {
    setLoading(true);
    try {
      const token = await getValidAccessToken();
      if (!token) {
        setIsConnected(false);
        return;
      }

      // 1. List messages
      const listRes = await fetch('https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults=15', {
        headers: { 'Authorization': `Bearer ${token}` }
      });
      
      if (!listRes.ok) throw new Error("Failed to list messages");
      
      const listData = await listRes.json();
      if (!listData.messages) {
        setEmails([]);
        return;
      }

      // 2. Fetch details for each message (in parallel)
      const detailsPromises = listData.messages.map(async (msg: { id: string }) => {
        const detailRes = await fetch(`https://gmail.googleapis.com/gmail/v1/users/me/messages/${msg.id}`, {
          headers: { 'Authorization': `Bearer ${token}` }
        });
        return detailRes.json();
      });

      const details = await Promise.all(detailsPromises);
      
      const parsedEmails: Email[] = details.map((d: any) => {
        const headers = d.payload.headers;
        const from = headers.find((h: any) => h.name === 'From')?.value || 'Unknown';
        const subject = headers.find((h: any) => h.name === 'Subject')?.value || '(No Subject)';
        const date = headers.find((h: any) => h.name === 'Date')?.value || new Date().toISOString();
        const isRead = !d.labelIds.includes('UNREAD');
        
        return {
          id: d.id,
          from,
          subject,
          snippet: d.snippet,
          date,
          isRead
        };
      });

      setEmails(parsedEmails);
    } catch (err) {
      console.error(err);
      setError("Failed to fetch emails.");
    } finally {
      setLoading(false);
    }
  };

  const handleSend = async () => {
    if (!to || !subject) return;
    setLoading(true);
    
    try {
      const token = await getValidAccessToken();
      if (!token) throw new Error("No access token");

      // Construct raw email
      const emailContent = [
        `To: ${to}`,
        `Subject: ${subject}`,
        'Content-Type: text/plain; charset=utf-8',
        '',
        body
      ].join('\n');

      const encodedEmail = btoa(unescape(encodeURIComponent(emailContent)))
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=+$/, '');

      const res = await fetch('https://gmail.googleapis.com/gmail/v1/users/me/messages/send', {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          raw: encodedEmail
        })
      });

      if (!res.ok) throw new Error("Failed to send");

      alert('Email sent successfully!');
      setTo('');
      setSubject('');
      setBody('');
      setActiveView('inbox');
      fetchEmails(); // Refresh sent items?
    } catch (err) {
      console.error(err);
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
          <div className="bg-yellow-50 dark:bg-yellow-900/20 p-3 rounded-md text-xs text-yellow-800 dark:text-yellow-200 border border-yellow-200 dark:border-yellow-800">
             OAuth requires a Google Cloud Project. 
             <a href="https://console.cloud.google.com/apis/credentials" target="_blank" className="underline ml-1 font-bold">
               Get Credentials
             </a>
          </div>

          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Client ID</label>
            <div className="relative">
              <ShieldCheck className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
              <input 
                type="text" 
                placeholder="Google Client ID" 
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>
          </div>

          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Client Secret</label>
            <div className="relative">
              <Key className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
              <input 
                type="password" 
                placeholder="Google Client Secret" 
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>
          </div>

          {error && <p className="text-sm text-destructive break-all">{error}</p>}

          <button 
            onClick={handleConnect}
            disabled={!clientId || !clientSecret || loading}
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
            Gmail
          </h2>
          <p className="text-sm text-muted-foreground mt-1 flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-green-500"></span>
            Connected via OAuth
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
                          {(() => {
                            try {
                              return formatDistanceToNow(new Date(email.date), { addSuffix: true });
                            } catch (e) {
                              return email.date;
                            }
                          })()}
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