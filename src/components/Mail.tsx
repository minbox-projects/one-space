import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Mail as MailIcon, Inbox, PenSquare, Send, RefreshCw, Key, LogOut, Loader2, ShieldCheck, ChevronLeft, Paperclip, Download } from 'lucide-react';
import { formatDistanceToNow } from 'date-fns';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { 
  getValidAccessToken, 
  saveGmailTokens, 
  saveGmailConfig, 
  getGmailConfig, 
  clearGmailSession,
  getGmailProfile,
  saveUserEmail,
  getUserEmail
} from '../lib/gmail';

interface Attachment {
  id: string;
  filename: string;
  mimeType: string;
  size: number;
}

interface Email {
  id: string;
  from: string;
  subject: string;
  snippet: string;
  date: string;
  isRead: boolean;
  htmlBody?: string;
  textBody?: string;
  attachments?: Attachment[];
}

export function Mail() {
  const { t } = useTranslation();
  
  const [isConnected, setIsConnected] = useState<boolean | null>(null);
  const [userEmail, setUserEmail] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // Config state
  const [clientId, setClientId] = useState('');
  const [clientSecret, setClientSecret] = useState('');
  
  const [activeView, setActiveView] = useState<'inbox' | 'compose' | 'detail'>('inbox');
  const [emails, setEmails] = useState<Email[]>([]);
  const [selectedEmail, setSelectedEmail] = useState<Email | null>(null);
  const [nextPageToken, setNextPageToken] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(true);
  
  // Compose state
  const [to, setTo] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');

  useEffect(() => {
    checkConnection();
  }, []);

  const checkConnection = async () => {
    const config = await getGmailConfig();
    if (config) {
      setClientId(config.clientId);
      setClientSecret(config.clientSecret);
      const token = await getValidAccessToken();
      if (token) {
        setIsConnected(true);
        // Load stored email if available
        const storedEmail = await getUserEmail();
        if (storedEmail) {
          setUserEmail(storedEmail);
        } else {
          // Fetch if not stored
          const profile = await getGmailProfile();
          if (profile) {
            setUserEmail(profile.emailAddress);
            await saveUserEmail(profile.emailAddress);
          }
        }
        fetchEmails();
      } else {
        setIsConnected(false);
      }
    } else {
      setIsConnected(false);
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
      
      const result: { code: string, redirect_uri: string } = await invoke('start_google_oauth', { 
        clientId, 
        scope 
      });
      
      const tokenResponse: string = await invoke('exchange_google_token', {
        code: result.code,
        clientId,
        clientSecret,
        redirectUri: result.redirect_uri
      });
 
      const tokens = JSON.parse(tokenResponse);
      
      await saveGmailConfig({ clientId, clientSecret });
      await saveGmailTokens(tokens);
      
      // Fetch user profile to get email address
      const profile = await getGmailProfile();
      if (profile) {
        setUserEmail(profile.emailAddress);
        await saveUserEmail(profile.emailAddress);
      }
      
      setIsConnected(true);
      fetchEmails();
    } catch (err: any) {
      console.error(err);
      setError("Authentication failed. " + (typeof err === 'string' ? err : err.message || "Unknown error"));
    } finally {
      setLoading(false);
    }
  };

  const handleDisconnect = async () => {
    await clearGmailSession();
    setIsConnected(false);
    setUserEmail(null);
    setEmails([]);
    setSelectedEmail(null);
    setActiveView('inbox');
  };

  const decodeBase64Utf8 = (base64: string) => {
    try {
      const binaryString = atob(base64.replace(/-/g, '+').replace(/_/g, '/'));
      const bytes = new Uint8Array(binaryString.length);
      for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }
      return new TextDecoder('utf-8').decode(bytes);
    } catch (e) {
      console.error("Decoding error", e);
      return "[Decoding Error]";
    }
  };

  const parseMessagePart = (part: any, result: { html?: string, text?: string, attachments: Attachment[] }) => {
    if (part.mimeType === 'text/plain' && part.body?.data) {
      result.text = decodeBase64Utf8(part.body.data);
    } else if (part.mimeType === 'text/html' && part.body?.data) {
      result.html = decodeBase64Utf8(part.body.data);
    } else if (part.filename && part.filename.length > 0) {
      result.attachments.push({
        id: part.body?.attachmentId || '',
        filename: part.filename,
        mimeType: part.mimeType,
        size: part.body?.size || 0
      });
    }

    if (part.parts) {
      part.parts.forEach((p: any) => parseMessagePart(p, result));
    }
  };

  const proxyRequestJson = async <T,>(
    url: string,
    token: string,
    method: 'GET' | 'POST' = 'GET',
    payload?: unknown
  ): Promise<T> => {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${token}`,
    };
    let bodyStr: string | null = null;

    if (payload !== undefined) {
      headers['Content-Type'] = 'application/json';
      bodyStr = JSON.stringify(payload);
    }

    const resText = await invoke<string>('proxy_http_request', {
      url,
      method,
      headers,
      body: bodyStr,
    });
    return JSON.parse(resText) as T;
  };

  const fetchEmailDetails = async (emailId: string) => {
    setLoading(true);
    try {
      const token = await getValidAccessToken();
      if (!token) return;

      const data = await proxyRequestJson<any>(
        `https://gmail.googleapis.com/gmail/v1/users/me/messages/${emailId}`,
        token,
      );

      const result = { html: '', text: '', attachments: [] as Attachment[] };
      if (data.payload) {
        parseMessagePart(data.payload, result);
      }

      const headers = data.payload.headers;
      const emailDetail: Email = {
        id: data.id,
        from: headers.find((h: any) => h.name === 'From')?.value || 'Unknown',
        subject: headers.find((h: any) => h.name === 'Subject')?.value || '(No Subject)',
        snippet: data.snippet,
        date: headers.find((h: any) => h.name === 'Date')?.value || new Date().toISOString(),
        isRead: !data.labelIds.includes('UNREAD'),
        htmlBody: result.html,
        textBody: result.text,
        attachments: result.attachments
      };

      setSelectedEmail(emailDetail);
      setActiveView('detail');

      // Mark as read if it was unread
      if (!emailDetail.isRead) {
        await proxyRequestJson<any>(
          `https://gmail.googleapis.com/gmail/v1/users/me/messages/${emailId}/modify`,
          token,
          'POST',
          { removeLabelIds: ['UNREAD'] }
        );
        // Update local state to reflect read status
        setEmails(prev => prev.map(e => e.id === emailId ? { ...e, isRead: true } : e));
        // Emit event to refresh unread count in sidebar
        emit('refresh-mail-count').catch(console.error);
      }
    } catch (err) {
      console.error(err);
      setError("Failed to fetch email details.");
    } finally {
      setLoading(false);
    }
  };

  const fetchEmails = async (isLoadMore = false) => {
    if (isLoadMore) {
      if (loadingMore || !nextPageToken) return;
      setLoadingMore(true);
    } else {
      if (loading) return;
      setLoading(true);
    }
    
    try {
      const token = await getValidAccessToken();
      if (!token) {
        setIsConnected(false);
        return;
      }

      let url = 'https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults=15';
      if (isLoadMore && nextPageToken) {
        url += `&pageToken=${nextPageToken}`;
      }

      const listData = await proxyRequestJson<any>(url, token);
      setNextPageToken(listData.nextPageToken || null);
      setHasMore(!!listData.nextPageToken);

      if (!listData.messages) {
        if (!isLoadMore) setEmails([]);
        return;
      }

      const detailsPromises = listData.messages.map(async (msg: { id: string }) => {
        return proxyRequestJson<any>(
          `https://gmail.googleapis.com/gmail/v1/users/me/messages/${msg.id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date`,
          token,
        );
      });

      const details = await Promise.all(detailsPromises);
      
      const parsedEmails: Email[] = details.map((d: any) => {
        const headers = d.payload?.headers || [];
        const from = headers.find((h: any) => h.name === 'From')?.value || 'Unknown';
        const subject = headers.find((h: any) => h.name === 'Subject')?.value || '(No Subject)';
        const date = headers.find((h: any) => h.name === 'Date')?.value || new Date().toISOString();
        const isRead = !d.labelIds?.includes('UNREAD');
        
        return {
          id: d.id,
          from,
          subject,
          snippet: d.snippet || '',
          date,
          isRead
        };
      });

      if (isLoadMore) {
        setEmails(prev => [...prev, ...parsedEmails]);
      } else {
        setEmails(parsedEmails);
      }
    } catch (err) {
      console.error(err);
      setError("Failed to fetch emails.");
    } finally {
      setLoading(false);
      setLoadingMore(false);
    }
  };

  const handleSend = async () => {
    if (!to || !subject) return;
    setLoading(true);
    
    try {
      const token = await getValidAccessToken();
      if (!token) throw new Error("No access token");

      const utf8Subject = btoa(unescape(encodeURIComponent(subject)));
      const encodedSubject = `=?utf-8?B?${utf8Subject}?=`;
      const encodedBody = btoa(unescape(encodeURIComponent(body)));

      const emailContent = [
        `From: me`,
        `To: ${to}`,
        `Subject: ${encodedSubject}`,
        'MIME-Version: 1.0',
        'Content-Type: text/plain; charset=utf-8',
        'Content-Transfer-Encoding: base64',
        '',
        encodedBody
      ].join('\r\n');

      const encodedEmail = btoa(unescape(encodeURIComponent(emailContent)))
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=+$/, '');

      await proxyRequestJson<any>(
        'https://gmail.googleapis.com/gmail/v1/users/me/messages/send',
        token,
        'POST',
        { raw: encodedEmail }
      );

      alert(t('emailSentSuccess', 'Email sent successfully!'));
      setTo('');
      setSubject('');
      setBody('');
      setActiveView('inbox');
      fetchEmails();
    } catch (err: any) {
      console.error(err);
      alert(t('emailSendFailed', 'Failed to send email: ') + (err.message || "Unknown error"));
    } finally {
      setLoading(false);
    }
  };

  const downloadAttachment = async (attachmentId: string, filename: string) => {
    if (!selectedEmail) return;
    try {
      const token = await getValidAccessToken();
      if (!token) {
        throw new Error("No access token");
      }
      const data = await proxyRequestJson<any>(
        `https://gmail.googleapis.com/gmail/v1/users/me/messages/${selectedEmail.id}/attachments/${attachmentId}`,
        token
      );
      const base64 = data.data.replace(/-/g, '+').replace(/_/g, '/');
      const blob = await (await fetch(`data:application/octet-stream;base64,${base64}`)).blob();
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      a.click();
    } catch (err) {
      console.error("Download failed", err);
      alert(t('attachmentDownloadFailed', 'Failed to download attachment'));
    }
  };

  if (isConnected === null) {
    return (
      <div className="flex flex-col items-center justify-center h-full space-y-4">
        <Loader2 className="w-8 h-8 animate-spin text-primary" />
        <p className="text-sm text-muted-foreground">{t('checkingConnection', 'Checking connection...')}</p>
      </div>
    );
  }

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
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("googleClientId", "Client ID")}</label>
            <div className="relative">
              <ShieldCheck className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
              <input 
                type="text" 
                placeholder={t('googleClientIdPlaceholder', 'Google Client ID')} 
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                className="flex h-10 w-full rounded-md border border-input bg-background pl-9 pr-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
              />
            </div>
          </div>

          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{t("googleClientSecret", "Client Secret")}</label>
            <div className="relative">
              <Key className="w-4 h-4 absolute left-3 top-3 text-muted-foreground" />
              <input 
                type="password" 
                placeholder={t('googleClientSecretPlaceholder', 'Google Client Secret')} 
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
        <div className="flex items-center gap-3">
          {(activeView === 'compose' || activeView === 'detail') && (
            <button 
              onClick={() => {
                setActiveView('inbox');
                setSelectedEmail(null);
              }}
              className="p-2 hover:bg-muted rounded-full transition-colors"
            >
              <ChevronLeft className="w-5 h-5" />
            </button>
          )}
          <div>
            <h2 className="text-xl font-bold tracking-tight flex items-center gap-2">
              <MailIcon className="w-5 h-5 text-red-500" />
              {userEmail || 'Gmail'}
            </h2>
            <p className="text-sm text-muted-foreground flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-green-500"></span>
              OAuth
            </p>
          </div>
        </div>
        
        <div className="flex items-center gap-2">
          {activeView !== 'compose' && (
            <button 
              onClick={() => setActiveView('compose')}
              className="bg-primary text-primary-foreground px-3 py-1.5 rounded-md text-sm font-medium flex items-center gap-2 shadow-sm hover:opacity-90 transition-opacity"
            >
              <PenSquare className="w-4 h-4" />
              {t('compose')}
            </button>
          )}
          
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
            <div className="h-12 border-b bg-muted/10 flex items-center px-4 justify-between">
              <span className="text-sm font-medium">{t('inbox')}</span>
              <button 
                onClick={() => fetchEmails(false)}
                className="text-muted-foreground hover:text-primary transition-colors p-1"
                title={t('refresh')}
              >
                <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} />
              </button>
            </div>
            
            <div className="flex-1 overflow-y-auto custom-scrollbar">
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
                    <div 
                      key={email.id} 
                      onClick={() => fetchEmailDetails(email.id)}
                      className={`p-4 hover:bg-muted/50 cursor-pointer transition-colors relative ${!email.isRead ? 'bg-primary/5' : 'opacity-80'}`}
                    >
                      {!email.isRead && (
                        <div className="absolute left-2 top-1/2 -translate-y-1/2 w-1.5 h-1.5 rounded-full bg-primary" />
                      )}
                      <div className="flex justify-between items-start mb-1 pl-1">
                        <span className={`truncate mr-2 ${!email.isRead ? 'font-bold text-foreground' : 'font-medium text-foreground/80'}`}>
                          {email.from}
                        </span>
                        <span className={`text-xs whitespace-nowrap ${!email.isRead ? 'text-foreground font-medium' : 'text-muted-foreground'}`}>
                          {(() => {
                            try {
                              return formatDistanceToNow(new Date(email.date), { addSuffix: true });
                            } catch (e) {
                              return email.date;
                            }
                          })()}
                        </span>
                      </div>
                      <h4 className={`text-sm mb-1 pl-1 ${!email.isRead ? 'font-bold text-foreground' : 'font-normal text-foreground/70'}`}>{email.subject}</h4>
                      <p className="text-xs text-muted-foreground truncate pl-1">{email.snippet}</p>
                    </div>
                  ))}
                  
                  {/* Load More Button */}
                  {hasMore && (
                    <div className="p-4 flex justify-center">
                      <button 
                        onClick={() => fetchEmails(true)}
                        disabled={loadingMore}
                        className="text-sm font-medium text-primary hover:underline flex items-center gap-2 disabled:opacity-50"
                      >
                        {loadingMore ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
                        {loadingMore ? t('loading') : t('loadMore') || 'Load More'}
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
          </>
        ) : activeView === 'detail' && selectedEmail ? (
          <div className="flex flex-col h-full overflow-hidden">
            <div className="p-6 border-b shrink-0">
              <h3 className="text-xl font-bold mb-4">{selectedEmail.subject}</h3>
              <div className="flex justify-between items-center text-sm">
                <div>
                  <span className="font-semibold">{selectedEmail.from}</span>
                </div>
                <div className="text-muted-foreground">
                  {selectedEmail.date}
                </div>
              </div>
            </div>
            <div className="flex-1 overflow-y-auto p-6 bg-white dark:bg-zinc-950">
              {selectedEmail.htmlBody ? (
                <iframe 
                  srcDoc={selectedEmail.htmlBody} 
                  className="w-full h-full border-none"
                  title="Email Content"
                  sandbox="allow-popups allow-popups-to-escape-sandbox allow-same-origin"
                />
              ) : (
                <pre className="whitespace-pre-wrap font-sans text-sm">
                  {selectedEmail.textBody}
                </pre>
              )}
            </div>
            {selectedEmail.attachments && selectedEmail.attachments.length > 0 && (
              <div className="p-4 border-t bg-muted/20 shrink-0">
                <div className="flex items-center gap-2 mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <Paperclip className="w-3.5 h-3.5" />
                  {selectedEmail.attachments.length} Attachments
                </div>
                <div className="flex flex-wrap gap-2">
                  {selectedEmail.attachments.map(att => (
                    <div key={att.id} className="flex items-center gap-3 p-2 border rounded-lg bg-card text-sm group hover:border-primary transition-colors cursor-pointer" onClick={() => downloadAttachment(att.id, att.filename)}>
                      <div className="w-8 h-8 rounded bg-muted flex items-center justify-center">
                        <Paperclip className="w-4 h-4" />
                      </div>
                      <div className="flex flex-col min-w-0 max-w-[200px]">
                        <span className="font-medium truncate">{att.filename}</span>
                        <span className="text-[10px] text-muted-foreground">{(att.size / 1024).toFixed(1)} KB</span>
                      </div>
                      <Download className="w-4 h-4 text-muted-foreground group-hover:text-primary" />
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
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
