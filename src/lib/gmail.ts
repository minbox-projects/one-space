import { invoke } from '@tauri-apps/api/core';

export interface GmailTokens {
  access_token: string;
  refresh_token: string;
  expiry_date: number;
  token_type: string;
  scope: string;
}

export interface GmailConfig {
  clientId: string;
  clientSecret: string;
}

const TOKEN_KEY = 'onespace_gmail_tokens';
const CONFIG_KEY = 'onespace_gmail_config';
const EMAIL_KEY = 'onespace_gmail_user_email';

export const saveUserEmail = async (email: string) => {
  await invoke('save_secret', { key: EMAIL_KEY, value: email });
};

export const getUserEmail = async (): Promise<string | null> => {
  return await invoke('get_secret', { key: EMAIL_KEY });
};

export const getGmailProfile = async (): Promise<{ emailAddress: string } | null> => {
  const token = await getValidAccessToken();
  if (!token) return null;

  try {
    const resText = await invoke<string>('proxy_http_request', {
      url: 'https://gmail.googleapis.com/gmail/v1/users/me/profile',
      method: 'GET',
      headers: { 'Authorization': `Bearer ${token}` },
      body: null,
    });
    const res = JSON.parse(resText);
    return res;
  } catch (e) {
    console.error("Failed to fetch Gmail profile", e);
    return null;
  }
};

export const saveGmailTokens = async (tokens: any) => {
  // Calculate expiry if not present (usually expires_in is seconds)
  let expiry = tokens.expiry_date;
  if (!expiry && tokens.expires_in) {
    expiry = Date.now() + tokens.expires_in * 1000;
  }
  
  const tokenData: GmailTokens = {
    ...tokens,
    expiry_date: expiry
  };
  await invoke('save_secret', { key: TOKEN_KEY, value: JSON.stringify(tokenData) });
};

export const getGmailTokens = async (): Promise<GmailTokens | null> => {
  const str: string | null = await invoke('get_secret', { key: TOKEN_KEY });
  return str ? JSON.parse(str) : null;
};

export const saveGmailConfig = async (config: GmailConfig) => {
  await invoke('save_secret', { key: CONFIG_KEY, value: JSON.stringify(config) });
};

export const getGmailConfig = async (): Promise<GmailConfig | null> => {
  const str: string | null = await invoke('get_secret', { key: CONFIG_KEY });
  return str ? JSON.parse(str) : null;
};

export const clearGmailSession = async () => {
  await invoke('delete_secret', { key: TOKEN_KEY });
  await invoke('delete_secret', { key: EMAIL_KEY });
};

export const refreshGmailToken = async (): Promise<string | null> => {
  const tokens = await getGmailTokens();
  const config = await getGmailConfig();
  
  if (!tokens?.refresh_token || !config?.clientId || !config?.clientSecret) {
    return null;
  }

  try {
    const response: string = await invoke('refresh_google_token', {
      refreshToken: tokens.refresh_token,
      clientId: config.clientId,
      clientSecret: config.clientSecret
    });
    
    const newTokens = JSON.parse(response);
    // Merge with old tokens to keep refresh_token if not returned
    const updatedTokens = {
      ...tokens,
      ...newTokens,
      refresh_token: newTokens.refresh_token || tokens.refresh_token
    };
    
    await saveGmailTokens(updatedTokens);
    return updatedTokens.access_token;
  } catch (e) {
    console.error("Failed to refresh token", e);
    return null;
  }
};

export const getValidAccessToken = async (): Promise<string | null> => {
  const tokens = await getGmailTokens();
  if (!tokens) return null;

  // Buffer of 60 seconds
  if (Date.now() > tokens.expiry_date - 60000) {
    return await refreshGmailToken();
  }
  
  return tokens.access_token;
};

export const getUnreadEmailCount = async (): Promise<number> => {
  const token = await getValidAccessToken();
  if (!token) return 0;

  try {
    const resText = await invoke<string>('proxy_http_request', {
      url: 'https://gmail.googleapis.com/gmail/v1/users/me/labels/UNREAD',
      method: 'GET',
      headers: { 'Authorization': `Bearer ${token}` },
      body: null,
    });

    const data = JSON.parse(resText);
    return data.messagesUnread || 0;
  } catch (e: any) {
    if (e.toString().includes('401')) {
      const newToken = await refreshGmailToken();
      if (!newToken) return 0;
      try {
        const retryResText = await invoke<string>('proxy_http_request', {
          url: 'https://gmail.googleapis.com/gmail/v1/users/me/labels/UNREAD',
          method: 'GET',
          headers: { 'Authorization': `Bearer ${newToken}` },
          body: null,
        });
        const data = JSON.parse(retryResText);
        return data.messagesUnread || 0;
      } catch (retryError) {
        console.error("Failed to fetch unread count after retry", retryError);
        return 0;
      }
    }
    console.error("Failed to fetch unread count", e);
    return 0;
  }
};
