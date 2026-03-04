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

interface GmailTokensInput extends Partial<GmailTokens> {
  expires_in?: number;
}

type GmailLogoutReason = 'manual_logout' | 'expired' | 'not_logged_in';

interface GmailLoginState {
  status: 'logged_in' | 'logged_out';
  expiry_date?: number;
  reason?: GmailLogoutReason;
  updated_at: number;
}

export interface GmailSessionStatus {
  authenticated: boolean;
  reason?: GmailLogoutReason;
}

const TOKEN_KEY = 'onespace_gmail_tokens';
const CONFIG_KEY = 'onespace_gmail_config';
const EMAIL_KEY = 'onespace_gmail_user_email';
const LOGIN_STATE_KEY = 'onespace_gmail_login_state';

const saveLoginState = async (state: GmailLoginState) => {
  await invoke('save_secret', { key: LOGIN_STATE_KEY, value: JSON.stringify(state) });
};

const getLoginState = async (): Promise<GmailLoginState | null> => {
  const str: string | null = await invoke('get_secret', { key: LOGIN_STATE_KEY });
  return str ? JSON.parse(str) : null;
};

const markLoggedIn = async (expiryDate?: number) => {
  const current = await getLoginState();
  if (current?.status === 'logged_in' && current.expiry_date === expiryDate) {
    return;
  }
  await saveLoginState({
    status: 'logged_in',
    expiry_date: expiryDate,
    updated_at: Date.now(),
  });
};

const markLoggedOut = async (reason: GmailLogoutReason) => {
  const current = await getLoginState();
  if (current?.status === 'logged_out' && current.reason === reason) {
    return;
  }
  await saveLoginState({
    status: 'logged_out',
    reason,
    updated_at: Date.now(),
  });
};

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

export const saveGmailTokens = async (tokens: GmailTokensInput) => {
  // Calculate expiry if not present (usually expires_in is seconds)
  let expiry = tokens.expiry_date ?? 0;
  if (!expiry && tokens.expires_in) {
    expiry = Date.now() + tokens.expires_in * 1000;
  }
  
  const tokenData: GmailTokens = {
    access_token: tokens.access_token || '',
    refresh_token: tokens.refresh_token || '',
    token_type: tokens.token_type || '',
    scope: tokens.scope || '',
    expiry_date: expiry
  };
  await invoke('save_secret', { key: TOKEN_KEY, value: JSON.stringify(tokenData) });
  await markLoggedIn(tokenData.expiry_date);
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
  await markLoggedOut('manual_logout');
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
  if (!tokens) {
    const state = await getLoginState();
    if (!state || state.status !== 'logged_out') {
      await markLoggedOut('not_logged_in');
    }
    return null;
  }

  // Buffer of 60 seconds
  if (Date.now() > tokens.expiry_date - 60000) {
    const refreshed = await refreshGmailToken();
    if (!refreshed) {
      await markLoggedOut('expired');
      return null;
    }
    return refreshed;
  }
  return tokens.access_token;
};

export const getGmailSessionStatus = async (): Promise<GmailSessionStatus> => {
  const token = await getValidAccessToken();
  if (token) {
    return { authenticated: true };
  }

  const state = await getLoginState();
  if (state?.status === 'logged_out' && state.reason) {
    return { authenticated: false, reason: state.reason };
  }
  return { authenticated: false, reason: 'not_logged_in' };
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
  } catch (e: unknown) {
    if (String(e).includes('401')) {
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
