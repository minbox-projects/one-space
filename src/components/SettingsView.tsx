import { useState, useEffect, useMemo, useRef, type ChangeEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { 
  Save, 
  RefreshCw, 
  Undo2,
  HardDrive, 
  Palette, 
  Keyboard as KeyboardIcon, 
  Terminal, 
  FolderOpen, 
  Zap, 
  CircleDot, 
  User, 
  Lock, 
  Key, 
  ShieldCheck, 
  Eye, 
  EyeOff, 
  ChevronLeft,
  Settings as SettingsIcon,
  CheckCircle2,
  AlertCircle,
  Command,
  Monitor,
  Moon,
  Sun,
  Globe,
  PlugZap,
  Sparkles,
  Bot,
  Copy,
  Download,
  Upload,
  Plus,
  Trash2,
  X,
  Newspaper
} from 'lucide-react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { useTheme } from './ThemeProvider';
import { skillModelOptions } from './skillsModelOptions';
import { Switch } from '@/components/ui/switch';

interface SyncPolicy {
  providers: boolean;
  mcp: boolean;
  content: boolean;
  workflow_presets: boolean;
  skills_sources: boolean;
  skills_repository: boolean;
  subagents_sources: boolean;
  subagents_repository: boolean;
  ai_news: boolean;
}

interface StorageConfig {
  storage_type: 'local' | 'git' | 'icloud';
  git_url?: string;
  auth_method?: 'http' | 'ssh';
  http_username?: string;
  http_token?: string;
  ssh_key_path?: string;
  main_shortcut?: string;
  quick_ai_shortcut?: string;
  default_ai_dir?: string;
  default_ai_model?: 'claude' | 'gemini' | 'codex' | 'opencode';
  ai_terminal_app?: string;
  ai_model_launch_commands?: AiModelLaunchCommands;
  ai_sessions_history_days?: number;
  language?: string;
  local_storage_path?: string;
  icloud_storage_path?: string;
  proxy?: ProxyConfig;
  launch_at_login?: boolean;
  auto_update_enabled?: boolean;
  update_check_interval_minutes?: number;
  update_last_checked_at?: number;
  skills_sync_enabled?: boolean;
  skills_sync_interval_minutes?: number;
  skills_new_badge_hours?: number;
  skills_last_synced_at?: number;
  skills_sources?: SkillSourceConfig[];
  subagents_sync_enabled?: boolean;
  subagents_sync_interval_minutes?: number;
  subagents_new_badge_hours?: number;
  subagents_last_synced_at?: number;
  subagents_sources?: SkillSourceConfig[];
  ai_news_enabled?: boolean;
  ai_news_sync_interval_minutes?: number;
  ai_news_retention_days?: number;
  ai_news_retention_max_items?: number;
  ai_news_keywords?: string;
  ai_news_last_synced_at?: number;
  sync_policy?: SyncPolicy;
}

type AiModelId = 'claude' | 'gemini' | 'codex' | 'opencode';

interface AiModelLaunchCommands {
  claude?: string;
  gemini?: string;
  codex?: string;
  opencode?: string;
}

interface SkillSourceConfig {
  id: string;
  name: string;
  repo_url: string;
  branch?: string;
  base_dir?: string;
  enabled: boolean;
  default_models?: string[];
}

interface SkillSourceValidation {
  id?: string;
  repo_url?: string;
  base_dir?: string;
  default_models?: string;
}

const DEFAULT_SKILL_SOURCE_MODELS = ['claude', 'gemini', 'codex', 'opencode'] as const;

function normalizeSkillSourcesForUi(
  sources: StorageConfig['skills_sources'],
): SkillSourceConfig[] {
  const validModelIds = new Set<string>(skillModelOptions.map((item) => item.id));
  const safeSources = Array.isArray(sources) ? sources : [];
  return safeSources
    .filter((source): source is SkillSourceConfig => !!source && typeof source === 'object')
    .map((source) => {
      const normalizedModels = Array.isArray(source.default_models)
        ? source.default_models
            .map((m) => String(m).trim())
            .filter((m) => validModelIds.has(m))
        : [];
      return {
        id: String(source.id || '').trim(),
        name: String(source.name || ''),
        repo_url: String(source.repo_url || '').trim(),
        branch: String(source.branch || 'main').trim() || 'main',
        base_dir: String(source.base_dir || '/').trim() || '/',
        enabled: source.enabled !== false,
        default_models: normalizedModels.length > 0 ? normalizedModels : [...DEFAULT_SKILL_SOURCE_MODELS],
      };
    });
}

interface ProxyConfig {
  proxy_enabled: boolean;
  proxy_type: 'http' | 'https' | 'socks5';
  proxy_host: string;
  proxy_port: number;
  proxy_username?: string;
  proxy_password?: string;
  check_interval: number;
}

interface ProxyStatus {
  is_available: boolean;
  latency_ms: number;
  message: string;
  proxy_type: string;
  proxy_host: string;
}

type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };

interface SkillsSourceSyncState {
  source_id: string;
  last_synced_at?: number;
  last_status: string;
  last_error?: string;
}

interface SkillsSyncState {
  status: string;
  last_error?: string;
  last_sync_at?: number;
  sources: SkillsSourceSyncState[];
}

interface SubagentSourceDiagnoseSkippedSample {
  rel_path: string;
  reason: string;
}

interface SubagentSourceDiagnoseResult {
  source_id: string;
  scan_root: string;
  last_commit_sha?: string;
  total_entries: number;
  accepted_entries: number;
  skipped_entries: number;
  skipped_missing_frontmatter: number;
  skipped_missing_name: number;
  skipped_invalid_name: number;
  skipped_read_error: number;
  skipped_other: number;
  skipped_samples: SubagentSourceDiagnoseSkippedSample[];
}

type SettingsTab = 'storage' | 'news' | 'general' | 'updates' | 'skills' | 'subagents' | 'proxy' | 'shortcuts' | 'ai' | 'appearance' | 'security';

const SETTINGS_TABS: SettingsTab[] = ['storage', 'news', 'general', 'updates', 'skills', 'subagents', 'proxy', 'shortcuts', 'ai', 'appearance', 'security'];

const DEFAULT_PROXY_CONFIG: ProxyConfig = {
  proxy_enabled: false,
  proxy_type: 'socks5',
  proxy_host: '',
  proxy_port: 1080,
  proxy_username: '',
  proxy_password: '',
  check_interval: 15,
};

const DEFAULT_AI_MODEL_LAUNCH_COMMANDS: Required<AiModelLaunchCommands> = {
  claude: 'claude --session-id {session_id}',
  gemini: 'gemini',
  codex: 'codex',
  opencode: 'opencode',
};

function normalizeAiModelLaunchCommandsForUi(
  commands?: AiModelLaunchCommands,
): Required<AiModelLaunchCommands> {
  return {
    claude: typeof commands?.claude === 'string' ? commands.claude : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.claude,
    gemini: typeof commands?.gemini === 'string' ? commands.gemini : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.gemini,
    codex: typeof commands?.codex === 'string' ? commands.codex : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.codex,
    opencode: typeof commands?.opencode === 'string' ? commands.opencode : DEFAULT_AI_MODEL_LAUNCH_COMMANDS.opencode,
  };
}

const DEFAULT_SYNC_POLICY: SyncPolicy = {
  providers: true,
  mcp: true,
  content: true,
  workflow_presets: true,
  skills_sources: true,
  skills_repository: false,
  subagents_sources: true,
  subagents_repository: false,
  ai_news: false,
};

const AI_NEWS_GNEWS_SECRET_KEY = 'onespace_ai_news_gnews_apikey';
const AI_NEWS_NEWSAPI_SECRET_KEY = 'onespace_ai_news_newsapi_apikey';
const DEFAULT_AI_NEWS_KEYWORDS =
  'artificial intelligence, generative AI, LLM, large language model, OpenAI, Anthropic, Gemini';

type NewsRetentionPreset = '7d_200' | '30d_500' | '90d_1000' | 'custom';

function detectNewsRetentionPreset(days?: number, maxItems?: number): NewsRetentionPreset {
  if (days === 7 && maxItems === 200) return '7d_200';
  if (days === 30 && maxItems === 500) return '30d_500';
  if (days === 90 && maxItems === 1000) return '90d_1000';
  return 'custom';
}

function normalizeSyncPolicyForUi(policy?: Partial<SyncPolicy>): SyncPolicy {
  return {
    ...DEFAULT_SYNC_POLICY,
    ...(policy || {}),
  };
}

function isSettingsTab(value: string): value is SettingsTab {
  return (SETTINGS_TABS as string[]).includes(value);
}

const MD5_SHIFT_AMOUNTS = [
  7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
  5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
  4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
  6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const MD5_K = Array.from({ length: 64 }, (_, i) =>
  Math.floor(Math.abs(Math.sin(i + 1)) * 2 ** 32) >>> 0,
);

const leftRotate = (value: number, amount: number) =>
  ((value << amount) | (value >>> (32 - amount))) >>> 0;

const toHexLE = (word: number) =>
  [word & 0xff, (word >>> 8) & 0xff, (word >>> 16) & 0xff, (word >>> 24) & 0xff]
    .map((v) => v.toString(16).padStart(2, '0'))
    .join('');

function md5Hex(input: string): string {
  const bytes = Array.from(new TextEncoder().encode(input));
  const bitLen = bytes.length * 8;
  const bitLenLow = bitLen >>> 0;
  const bitLenHigh = Math.floor(bitLen / 2 ** 32) >>> 0;
  bytes.push(0x80);
  while (bytes.length % 64 !== 56) {
    bytes.push(0);
  }
  for (let i = 0; i < 4; i += 1) {
    bytes.push((bitLenLow >>> (8 * i)) & 0xff);
  }
  for (let i = 0; i < 4; i += 1) {
    bytes.push((bitLenHigh >>> (8 * i)) & 0xff);
  }

  let a0 = 0x67452301;
  let b0 = 0xefcdab89;
  let c0 = 0x98badcfe;
  let d0 = 0x10325476;

  for (let offset = 0; offset < bytes.length; offset += 64) {
    const m = new Array<number>(16).fill(0);
    for (let i = 0; i < 16; i += 1) {
      const j = offset + i * 4;
      m[i] =
        (bytes[j] as number) |
        ((bytes[j + 1] as number) << 8) |
        ((bytes[j + 2] as number) << 16) |
        ((bytes[j + 3] as number) << 24);
    }

    let a = a0;
    let b = b0;
    let c = c0;
    let d = d0;

    for (let i = 0; i < 64; i += 1) {
      let f = 0;
      let g = 0;

      if (i < 16) {
        f = (b & c) | (~b & d);
        g = i;
      } else if (i < 32) {
        f = (d & b) | (~d & c);
        g = (5 * i + 1) % 16;
      } else if (i < 48) {
        f = b ^ c ^ d;
        g = (3 * i + 5) % 16;
      } else {
        f = c ^ (b | ~d);
        g = (7 * i) % 16;
      }

      const temp = d;
      d = c;
      c = b;
      const mixed = (a + f + MD5_K[i] + m[g]) >>> 0;
      b = (b + leftRotate(mixed, MD5_SHIFT_AMOUNTS[i])) >>> 0;
      a = temp;
    }

    a0 = (a0 + a) >>> 0;
    b0 = (b0 + b) >>> 0;
    c0 = (c0 + c) >>> 0;
    d0 = (d0 + d) >>> 0;
  }

  return `${toHexLE(a0)}${toHexLE(b0)}${toHexLE(c0)}${toHexLE(d0)}`;
}

function generateRandomMd5String(): string {
  const seed = `${crypto.randomUUID()}-${Date.now()}-${Math.random()}-${Math.random()}`;
  const raw = md5Hex(seed);
  return `${raw.slice(0, 8)}-${raw.slice(8, 12)}-${raw.slice(12, 16)}-${raw.slice(16, 20)}-${raw.slice(20)}`;
}

function normalizeConfigForUi(cfg: StorageConfig, fallbackTerminalApp: string): StorageConfig {
  return {
    ...cfg,
    storage_type: cfg.storage_type || 'local',
    auth_method: cfg.auth_method || 'http',
    main_shortcut: cfg.main_shortcut || 'Alt+Space',
    quick_ai_shortcut: cfg.quick_ai_shortcut || 'Alt+Shift+A',
    default_ai_model: cfg.default_ai_model || 'claude',
    ai_terminal_app: cfg.ai_terminal_app || fallbackTerminalApp,
    ai_model_launch_commands: normalizeAiModelLaunchCommandsForUi(cfg.ai_model_launch_commands),
    launch_at_login: cfg.launch_at_login ?? false,
    auto_update_enabled: cfg.auto_update_enabled ?? false,
    update_check_interval_minutes: cfg.update_check_interval_minutes ?? 360,
    skills_sync_enabled: cfg.skills_sync_enabled ?? true,
    skills_sync_interval_minutes: cfg.skills_sync_interval_minutes ?? 60,
    skills_new_badge_hours: cfg.skills_new_badge_hours ?? 72,
    skills_sources: normalizeSkillSourcesForUi(cfg.skills_sources),
    subagents_sync_enabled: cfg.subagents_sync_enabled ?? true,
    subagents_sync_interval_minutes: cfg.subagents_sync_interval_minutes ?? 60,
    subagents_new_badge_hours: cfg.subagents_new_badge_hours ?? 72,
    subagents_sources: normalizeSkillSourcesForUi(cfg.subagents_sources),
    ai_news_enabled: cfg.ai_news_enabled ?? false,
    ai_news_sync_interval_minutes: cfg.ai_news_sync_interval_minutes ?? 60,
    ai_news_retention_days: cfg.ai_news_retention_days ?? 90,
    ai_news_retention_max_items: cfg.ai_news_retention_max_items ?? 1000,
    ai_news_keywords: (cfg.ai_news_keywords && cfg.ai_news_keywords.trim()) || DEFAULT_AI_NEWS_KEYWORDS,
    sync_policy: normalizeSyncPolicyForUi(cfg.sync_policy),
  };
}

function normalizeProxyConfigForUi(proxy?: ProxyConfig): ProxyConfig {
  return {
    ...DEFAULT_PROXY_CONFIG,
    ...(proxy || {}),
    proxy_username: proxy?.proxy_username || '',
    proxy_password: proxy?.proxy_password || '',
  };
}

export function SettingsView({ initialTab = 'storage', onBack }: { initialTab?: string, onBack: () => void }) {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const [activeTab, setActiveTab] = useState<SettingsTab>(isSettingsTab(initialTab) ? initialTab : 'storage');
  const [config, setConfig] = useState<StorageConfig>({ storage_type: 'local' });
  const [savedConfig, setSavedConfig] = useState<StorageConfig>({ storage_type: 'local' });
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  const [newsApiKeys, setNewsApiKeys] = useState({ gnews: '', newsapi: '' });
  const [savedNewsApiKeys, setSavedNewsApiKeys] = useState({ gnews: '', newsapi: '' });
  const [newsRetentionPreset, setNewsRetentionPreset] = useState<NewsRetentionPreset>('90d_1000');
  
  // Shortcut Recording States
  const [recordingField, setRecordingField] = useState<'main' | 'quick' | null>(null);

  // Security States
  const [masterPassword, setMasterPassword] = useState('');
  const [showPass, setShowPass] = useState(false);
  const [newPass, setNewPass] = useState('');
  const [confirmNewPass, setConfirmNewPass] = useState('');
  const [showNewPass, setShowNewPass] = useState(true);
  const [showConfirmNewPass, setShowConfirmNewPass] = useState(true);
  const [changingPass, setChangingPass] = useState(false);

  // Proxy States
  const [proxyConfig, setProxyConfig] = useState<ProxyConfig>(DEFAULT_PROXY_CONFIG);
  const [savedProxyConfig, setSavedProxyConfig] = useState<ProxyConfig>(DEFAULT_PROXY_CONFIG);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [testingProxy, setTestingProxy] = useState(false);
  const [authEnabled, setAuthEnabled] = useState(false);
  const [newSkillSource, setNewSkillSource] = useState<SkillSourceConfig>({
    id: '',
    name: '',
    repo_url: '',
    branch: 'main',
    base_dir: '/',
    enabled: true,
    default_models: ['claude', 'gemini', 'codex', 'opencode'],
  });
  const [newSourceValidation, setNewSourceValidation] = useState<SkillSourceValidation>({});
  const skillsImportInputRef = useRef<HTMLInputElement | null>(null);
  const [showAddSkillSourceModal, setShowAddSkillSourceModal] = useState(false);
  const [skillsSyncState, setSkillsSyncState] = useState<SkillsSyncState | null>(null);
  const [skillsSyncNowLoading, setSkillsSyncNowLoading] = useState(false);
  const [newSubagentSource, setNewSubagentSource] = useState<SkillSourceConfig>({
    id: '',
    name: '',
    repo_url: '',
    branch: 'main',
    base_dir: '/',
    enabled: true,
    default_models: ['claude', 'gemini', 'codex', 'opencode'],
  });
  const [newSubagentSourceValidation, setNewSubagentSourceValidation] = useState<SkillSourceValidation>({});
  const subagentsImportInputRef = useRef<HTMLInputElement | null>(null);
  const [showAddSubagentSourceModal, setShowAddSubagentSourceModal] = useState(false);
  const [subagentsSyncState, setSubagentsSyncState] = useState<SkillsSyncState | null>(null);
  const [subagentsSyncNowLoading, setSubagentsSyncNowLoading] = useState(false);
  const [subagentSourceDiagnosing, setSubagentSourceDiagnosing] = useState<Record<string, boolean>>({});
  const [subagentSourceDiagnostics, setSubagentSourceDiagnostics] = useState<Record<string, SubagentSourceDiagnoseResult>>({});

  useEffect(() => {
    loadConfig();
  }, []);

  useEffect(() => {
    if (isSettingsTab(initialTab)) {
      setActiveTab(initialTab);
    }
  }, [initialTab]);

  useEffect(() => {
    if (activeTab === 'security') {
      loadMasterPassword();
    }
  }, [activeTab]);

  const loadSkillsSyncState = async () => {
    try {
      const resp = await invoke<ApiResp<SkillsSyncState>>('skills_sync_status_get');
      setSkillsSyncState(resp.data || null);
    } catch (e) {
      console.error(e);
    }
  };

  const loadSubagentsSyncState = async () => {
    try {
      const resp = await invoke<ApiResp<SkillsSyncState>>('subagents_sync_status_get');
      setSubagentsSyncState(resp.data || null);
    } catch (e) {
      console.error(e);
    }
  };

  const getSubagentDiagnoseReasonLabel = (reason: string) => {
    const key = `subagentsDiagnoseReason_${reason}`;
    switch (reason) {
      case 'missing_frontmatter':
        return t(key, 'Missing frontmatter block');
      case 'missing_name':
        return t(key, 'Missing frontmatter name');
      case 'invalid_name':
        return t(key, 'Invalid frontmatter name');
      case 'read_error':
        return t(key, 'Failed to read markdown');
      default:
        return t(key, reason);
    }
  };

  const getAutostartEnabled = async (): Promise<boolean | null> => {
    try {
      return await invoke<boolean>('plugin:autostart|is_enabled');
    } catch (e) {
      console.error(e);
      return null;
    }
  };

  const setAutostartEnabled = async (enabled: boolean) => {
    if (enabled) {
      await invoke('plugin:autostart|enable');
      return;
    }
    await invoke('plugin:autostart|disable');
  };

  const loadMasterPassword = async () => {
    try {
      const pass = await invoke<string>('get_master_password');
      setMasterPassword(pass);
    } catch (e) {
      console.error(e);
    }
  };

  const handleChangeMasterPassword = async () => {
    if (!newPass || !confirmNewPass) return;
    if (newPass !== confirmNewPass) {
      setMessage({ type: 'error', text: t('passwordNotMatch', 'Passwords do not match.') });
      return;
    }
    if (!masterPassword) {
      setMessage({ type: 'error', text: t('setMasterPassword', 'Please set a master password.') });
      return;
    }
    setLoading(true);
    try {
      await invoke('change_master_password', { oldPass: masterPassword, newPass });
      setMasterPassword(newPass);
      setNewPass('');
      setConfirmNewPass('');
      setShowNewPass(true);
      setShowConfirmNewPass(true);
      setChangingPass(false);
      setMessage({ type: 'success', text: t('passwordChanged', 'Master password changed successfully!') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleGenerateMd5Password = () => {
    const generated = generateRandomMd5String();
    setNewPass(generated);
    setConfirmNewPass(generated);
    setShowNewPass(true);
    setShowConfirmNewPass(true);
    setMessage({ type: 'success', text: t('md5PasswordGenerated', 'Generated and filled into both password fields.') });
    setTimeout(() => setMessage({ type: '', text: '' }), 2000);
  };

  // Handle keyboard events while recording
  useEffect(() => {
    if (!recordingField) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const modifiers = [];
      if (e.ctrlKey) modifiers.push('Control');
      if (e.altKey) modifiers.push('Alt');
      if (e.shiftKey) modifiers.push('Shift');
      if (e.metaKey) modifiers.push('Command');

      const key = e.key === ' ' ? 'Space' : e.key;
      const isModifierOnly = ['Control', 'Alt', 'Shift', 'Meta'].includes(e.key);
      
      if (!isModifierOnly) {
        let finalShortcut = '';
        if (modifiers.length > 0) {
          finalShortcut = modifiers.join('+') + '+' + key.charAt(0).toUpperCase() + key.slice(1);
        } else {
          finalShortcut = key.charAt(0).toUpperCase() + key.slice(1);
        }

        if (recordingField === 'main') {
          setConfig(prev => ({ ...prev, main_shortcut: finalShortcut }));
        } else {
          setConfig(prev => ({ ...prev, quick_ai_shortcut: finalShortcut }));
        }
        setRecordingField(null);
      }
    };

    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [recordingField]);

  const loadConfig = async () => {
    try {
      const cfg = await invoke<StorageConfig>('get_storage_config');
      const autostartEnabled = await getAutostartEnabled();
      const normalized = normalizeConfigForUi(
        {
          ...cfg,
          launch_at_login: autostartEnabled ?? (cfg.launch_at_login ?? false),
        },
        t('aiTerminalAppPlaceholder', '终端'),
      );
      const normalizedProxy = normalizeProxyConfigForUi(cfg.proxy);
      setConfig(normalized);
      setSavedConfig(normalized);
      setProxyConfig(normalizedProxy);
      setSavedProxyConfig(normalizedProxy);
      setNewsRetentionPreset(
        detectNewsRetentionPreset(
          normalized.ai_news_retention_days,
          normalized.ai_news_retention_max_items,
        ),
      );
      const loadedKeys = await loadNewsApiKeys();
      setNewsApiKeys(loadedKeys);
      setSavedNewsApiKeys(loadedKeys);
      // Enable auth switch if username or password is set
      setAuthEnabled(!!(normalizedProxy.proxy_username || normalizedProxy.proxy_password));
      await loadSkillsSyncState();
      await loadSubagentsSyncState();
    } catch (e) {
      console.error(e);
    }
  };

  const loadNewsApiKeys = async () => {
    const [gnewsKey, newsapiKey] = await Promise.all([
      invoke<string | null>('get_secret', { key: AI_NEWS_GNEWS_SECRET_KEY }),
      invoke<string | null>('get_secret', { key: AI_NEWS_NEWSAPI_SECRET_KEY }),
    ]);
    return {
      gnews: gnewsKey || '',
      newsapi: newsapiKey || '',
    };
  };

  const persistNewsApiKey = async (secretKey: string, value: string) => {
    const next = value.trim();
    if (!next) {
      await invoke('delete_secret', { key: secretKey });
      return;
    }
    await invoke('save_secret', { key: secretKey, value: next });
  };

  const resetNewSkillSourceForm = () => {
    setNewSkillSource({
      id: '',
      name: '',
      repo_url: '',
      branch: 'main',
      base_dir: '/',
      enabled: true,
      default_models: ['claude', 'gemini', 'codex', 'opencode'],
    });
    setNewSourceValidation({});
  };

  const resetNewSubagentSourceForm = () => {
    setNewSubagentSource({
      id: '',
      name: '',
      repo_url: '',
      branch: 'main',
      base_dir: '/',
      enabled: true,
      default_models: ['claude', 'gemini', 'codex', 'opencode'],
    });
    setNewSubagentSourceValidation({});
  };

  const addSkillSource = () => {
    const validation = validateSkillSource(newSkillSource, config.skills_sources || []);
    setNewSourceValidation(validation);
    if (Object.keys(validation).length > 0) {
      setMessage({ type: 'error', text: t('sourceValidationFailed', 'Source validation failed. Please fix highlighted fields.') });
      return false;
    }
    setConfig(prev => ({
      ...prev,
      skills_sources: [...(prev.skills_sources || []).filter(s => s.id !== newSkillSource.id), { ...newSkillSource }],
    }));
    resetNewSkillSourceForm();
    return true;
  };

  const addSubagentSource = () => {
    const validation = validateSkillSource(newSubagentSource, config.subagents_sources || []);
    setNewSubagentSourceValidation(validation);
    if (Object.keys(validation).length > 0) {
      setMessage({ type: 'error', text: t('sourceValidationFailed', 'Source validation failed. Please fix highlighted fields.') });
      return false;
    }
    setConfig(prev => ({
      ...prev,
      subagents_sources: [...(prev.subagents_sources || []).filter(s => s.id !== newSubagentSource.id), { ...newSubagentSource }],
    }));
    resetNewSubagentSourceForm();
    return true;
  };

  const removeSkillSource = (id: string) => {
    setConfig(prev => ({ ...prev, skills_sources: (prev.skills_sources || []).filter(s => s.id !== id) }));
  };

  const removeSubagentSource = (id: string) => {
    setConfig(prev => ({ ...prev, subagents_sources: (prev.subagents_sources || []).filter(s => s.id !== id) }));
  };

  const updateSkillSource = (id: string, patch: Partial<SkillSourceConfig>) => {
    setConfig(prev => ({
      ...prev,
      skills_sources: (prev.skills_sources || []).map((s) => {
        if (s.id !== id) return s;
        const next = { ...s, ...patch };
        return {
          ...next,
          id: next.id.trim(),
        };
      }),
    }));
  };

  const updateSubagentSource = (id: string, patch: Partial<SkillSourceConfig>) => {
    setConfig(prev => ({
      ...prev,
      subagents_sources: (prev.subagents_sources || []).map((s) => {
        if (s.id !== id) return s;
        const next = { ...s, ...patch };
        return {
          ...next,
          id: next.id.trim(),
        };
      }),
    }));
  };

  const validateRepoUrl = (url: string) => {
    const v = url.trim();
    return /^https:\/\/.+\.git$/i.test(v) || /^git@.+:.+\.git$/i.test(v);
  };

  const validateBaseDir = (v: string) => {
    const value = (v || '/').trim();
    if (!value.startsWith('/')) return false;
    if (value.includes('..')) return false;
    return true;
  };

  const validateSkillSource = (source: SkillSourceConfig, existing: SkillSourceConfig[]) => {
    const errs: SkillSourceValidation = {};
    const id = source.id.trim();
    if (!id) {
      errs.id = t('sourceIdRequired', 'Source ID is required.');
    } else if (!/^[a-zA-Z0-9._-]+$/.test(id)) {
      errs.id = t('sourceIdInvalid', 'Source ID can only contain letters, numbers, dot, underscore, and dash.');
    } else if (existing.some((s) => s.id === id)) {
      errs.id = t('sourceIdDuplicate', 'Source ID already exists.');
    }
    if (!validateRepoUrl(source.repo_url || '')) {
      errs.repo_url = t('sourceRepoInvalid', 'Repo URL must be https://...git or git@...:...git.');
    }
    if (!validateBaseDir(source.base_dir || '/')) {
      errs.base_dir = t('sourceBaseDirInvalid', 'Base directory must start with / and cannot contain ..');
    }
    const selectedModels = (source.default_models || []).filter((m) =>
      skillModelOptions.some((opt) => opt.id === m),
    );
    if (selectedModels.length === 0) {
      errs.default_models = t('sourceModelsRequired', 'Select at least one model.');
    }
    return errs;
  };

  const toggleNewSkillSourceModel = (modelId: string) => {
    setNewSkillSource((prev) => {
      const current = prev.default_models || [];
      const exists = current.includes(modelId);
      return {
        ...prev,
        default_models: exists ? current.filter((m) => m !== modelId) : [...current, modelId],
      };
    });
  };

  const toggleNewSubagentSourceModel = (modelId: string) => {
    setNewSubagentSource((prev) => {
      const current = prev.default_models || [];
      const exists = current.includes(modelId);
      return {
        ...prev,
        default_models: exists ? current.filter((m) => m !== modelId) : [...current, modelId],
      };
    });
  };

  const normalizeSkillSourceForSyncCompare = (source: Partial<SkillSourceConfig>) => {
    const validModelIds = new Set<string>(skillModelOptions.map((item) => item.id));
    const models = Array.from(
      new Set(
        (source.default_models || [])
          .map((m) => String(m).trim())
          .filter((m) => validModelIds.has(m))
      )
    ).sort();
    return {
      id: String(source.id || '').trim(),
      enabled: source.enabled !== false,
      repo_url: String(source.repo_url || '').trim(),
      branch: String(source.branch || 'main').trim() || 'main',
      base_dir: String(source.base_dir || '/').trim() || '/',
      default_models: models,
    };
  };

  const normalizeSkillSourcesForSyncCompare = (sources: SkillSourceConfig[] = []) =>
    sources
      .map((source) => normalizeSkillSourceForSyncCompare(source))
      .sort((a, b) => {
        const aKey = `${a.id}|${a.repo_url}|${a.branch}|${a.base_dir}|${a.enabled}|${a.default_models.join(',')}`;
        const bKey = `${b.id}|${b.repo_url}|${b.branch}|${b.base_dir}|${b.enabled}|${b.default_models.join(',')}`;
        return aKey.localeCompare(bKey);
      });

  const normalizeProxyForCompare = (proxy: ProxyConfig) => ({
    proxy_enabled: !!proxy.proxy_enabled,
    proxy_type: proxy.proxy_type || 'socks5',
    proxy_host: proxy.proxy_host || '',
    proxy_port: Number(proxy.proxy_port) || 0,
    proxy_username: proxy.proxy_username || '',
    proxy_password: proxy.proxy_password ? '__set__' : '',
    check_interval: Number(proxy.check_interval) || 15,
  });

  const getTabSnapshot = (
    tab: SettingsTab,
    cfg: StorageConfig,
    proxy: ProxyConfig,
    newsKeys: { gnews: string; newsapi: string },
  ) => {
    switch (tab) {
      case 'storage':
        {
          const policy = normalizeSyncPolicyForUi(cfg.sync_policy);
          return {
            storage_type: cfg.storage_type,
            git_url: cfg.git_url || '',
            auth_method: cfg.auth_method || 'http',
            http_username: cfg.http_username || '',
            http_token: cfg.http_token || '',
            ssh_key_path: cfg.ssh_key_path || '',
            local_storage_path: cfg.local_storage_path || '',
            icloud_storage_path: cfg.icloud_storage_path || '',
            sync_policy: {
              providers: policy.providers,
              mcp: policy.mcp,
              content: policy.content,
              workflow_presets: policy.workflow_presets,
              skills_sources: policy.skills_sources,
              skills_repository: policy.skills_repository,
              subagents_sources: policy.subagents_sources,
              subagents_repository: policy.subagents_repository,
              ai_news: policy.ai_news,
            },
          };
        }
      case 'news':
        return {
          ai_news_enabled: !!cfg.ai_news_enabled,
          ai_news_sync_interval_minutes: cfg.ai_news_sync_interval_minutes ?? 60,
          ai_news_retention_days: cfg.ai_news_retention_days ?? 90,
          ai_news_retention_max_items: cfg.ai_news_retention_max_items ?? 1000,
          ai_news_keywords: (cfg.ai_news_keywords || '').trim(),
          gnews_api_key: newsKeys.gnews,
          newsapi_api_key: newsKeys.newsapi,
        };
      case 'updates':
        return {
          auto_update_enabled: !!cfg.auto_update_enabled,
          update_check_interval_minutes: cfg.update_check_interval_minutes ?? 360,
        };
      case 'general':
        return {
          launch_at_login: !!cfg.launch_at_login,
        };
      case 'skills':
        return {
          skills_sync_enabled: !!cfg.skills_sync_enabled,
          skills_sync_interval_minutes: cfg.skills_sync_interval_minutes ?? 60,
          skills_new_badge_hours: cfg.skills_new_badge_hours ?? 72,
          skills_sources: normalizeSkillSourcesForSyncCompare(cfg.skills_sources || []),
        };
      case 'subagents':
        return {
          subagents_sync_enabled: !!cfg.subagents_sync_enabled,
          subagents_sync_interval_minutes: cfg.subagents_sync_interval_minutes ?? 60,
          subagents_new_badge_hours: cfg.subagents_new_badge_hours ?? 72,
          subagents_sources: normalizeSkillSourcesForSyncCompare(cfg.subagents_sources || []),
        };
      case 'proxy':
        return normalizeProxyForCompare(proxy);
      case 'shortcuts':
        return {
          main_shortcut: cfg.main_shortcut || '',
          quick_ai_shortcut: cfg.quick_ai_shortcut || '',
        };
      case 'ai':
        return {
          default_ai_model: cfg.default_ai_model || 'claude',
          ai_terminal_app: cfg.ai_terminal_app || '',
          default_ai_dir: cfg.default_ai_dir || '',
          ai_model_launch_commands: normalizeAiModelLaunchCommandsForUi(cfg.ai_model_launch_commands),
          ai_sessions_history_days: cfg.ai_sessions_history_days ?? 30,
        };
      case 'appearance':
        return {
          language: cfg.language || '',
        };
      case 'security':
      default:
        return null;
    }
  };

  const buildPayloadForTab = (
    tab: SettingsTab,
    draftCfg: StorageConfig,
    draftProxy: ProxyConfig,
    baseCfg: StorageConfig,
  ): StorageConfig => {
    const next: StorageConfig = {
      ...baseCfg,
      skills_sources: [...(baseCfg.skills_sources || [])],
      subagents_sources: [...(baseCfg.subagents_sources || [])],
    };

    switch (tab) {
      case 'storage':
        next.storage_type = draftCfg.storage_type;
        next.git_url = draftCfg.git_url;
        next.auth_method = draftCfg.auth_method;
        next.http_username = draftCfg.http_username;
        next.http_token = draftCfg.http_token;
        next.ssh_key_path = draftCfg.ssh_key_path;
        next.local_storage_path = draftCfg.local_storage_path;
        next.icloud_storage_path = draftCfg.icloud_storage_path;
        next.sync_policy = normalizeSyncPolicyForUi(draftCfg.sync_policy);
        break;
      case 'news': {
        next.ai_news_enabled = draftCfg.ai_news_enabled;
        next.ai_news_sync_interval_minutes = draftCfg.ai_news_sync_interval_minutes;
        next.ai_news_retention_days = draftCfg.ai_news_retention_days;
        next.ai_news_retention_max_items = draftCfg.ai_news_retention_max_items;
        next.ai_news_keywords = draftCfg.ai_news_keywords;
        break;
      }
      case 'updates':
        next.auto_update_enabled = draftCfg.auto_update_enabled;
        next.update_check_interval_minutes = draftCfg.update_check_interval_minutes;
        break;
      case 'general':
        next.launch_at_login = draftCfg.launch_at_login;
        break;
      case 'skills':
        next.skills_sync_enabled = draftCfg.skills_sync_enabled;
        next.skills_sync_interval_minutes = draftCfg.skills_sync_interval_minutes;
        next.skills_new_badge_hours = draftCfg.skills_new_badge_hours;
        next.skills_sources = [...(draftCfg.skills_sources || [])];
        break;
      case 'subagents':
        next.subagents_sync_enabled = draftCfg.subagents_sync_enabled;
        next.subagents_sync_interval_minutes = draftCfg.subagents_sync_interval_minutes;
        next.subagents_new_badge_hours = draftCfg.subagents_new_badge_hours;
        next.subagents_sources = [...(draftCfg.subagents_sources || [])];
        break;
      case 'proxy':
        next.proxy = { ...draftProxy };
        break;
      case 'shortcuts':
        next.main_shortcut = draftCfg.main_shortcut;
        next.quick_ai_shortcut = draftCfg.quick_ai_shortcut;
        break;
      case 'ai':
        next.default_ai_model = draftCfg.default_ai_model;
        next.ai_terminal_app = draftCfg.ai_terminal_app;
        next.default_ai_dir = draftCfg.default_ai_dir;
        next.ai_model_launch_commands = normalizeAiModelLaunchCommandsForUi(draftCfg.ai_model_launch_commands);
        next.ai_sessions_history_days = draftCfg.ai_sessions_history_days;
        break;
      case 'appearance':
        next.language = draftCfg.language;
        break;
      case 'security':
      default:
        break;
    }

    return next;
  };

  const syncDraftWithLatestForTab = (
    tab: SettingsTab,
    latestCfg: StorageConfig,
    latestProxy: ProxyConfig,
  ) => {
    if (tab === 'proxy') {
      setProxyConfig(latestProxy);
      setAuthEnabled(!!(latestProxy.proxy_username || latestProxy.proxy_password));
      return;
    }

    if (tab === 'news') {
      setNewsRetentionPreset(
        detectNewsRetentionPreset(
          latestCfg.ai_news_retention_days,
          latestCfg.ai_news_retention_max_items,
        ),
      );
    }

    setConfig((prev) => {
      const next = { ...prev };
      switch (tab) {
        case 'storage':
          next.storage_type = latestCfg.storage_type;
          next.git_url = latestCfg.git_url;
          next.auth_method = latestCfg.auth_method;
          next.http_username = latestCfg.http_username;
          next.http_token = latestCfg.http_token;
          next.ssh_key_path = latestCfg.ssh_key_path;
          next.local_storage_path = latestCfg.local_storage_path;
          next.icloud_storage_path = latestCfg.icloud_storage_path;
          next.sync_policy = normalizeSyncPolicyForUi(latestCfg.sync_policy);
          break;
        case 'news':
          next.ai_news_enabled = latestCfg.ai_news_enabled;
          next.ai_news_sync_interval_minutes = latestCfg.ai_news_sync_interval_minutes;
          next.ai_news_retention_days = latestCfg.ai_news_retention_days;
          next.ai_news_retention_max_items = latestCfg.ai_news_retention_max_items;
          next.ai_news_keywords = latestCfg.ai_news_keywords;
          break;
        case 'updates':
          next.auto_update_enabled = latestCfg.auto_update_enabled;
          next.update_check_interval_minutes = latestCfg.update_check_interval_minutes;
          break;
        case 'general':
          next.launch_at_login = latestCfg.launch_at_login;
          break;
        case 'skills':
          next.skills_sync_enabled = latestCfg.skills_sync_enabled;
          next.skills_sync_interval_minutes = latestCfg.skills_sync_interval_minutes;
          next.skills_new_badge_hours = latestCfg.skills_new_badge_hours;
          next.skills_sources = [...(latestCfg.skills_sources || [])];
          break;
        case 'subagents':
          next.subagents_sync_enabled = latestCfg.subagents_sync_enabled;
          next.subagents_sync_interval_minutes = latestCfg.subagents_sync_interval_minutes;
          next.subagents_new_badge_hours = latestCfg.subagents_new_badge_hours;
          next.subagents_sources = [...(latestCfg.subagents_sources || [])];
          break;
        case 'shortcuts':
          next.main_shortcut = latestCfg.main_shortcut;
          next.quick_ai_shortcut = latestCfg.quick_ai_shortcut;
          break;
        case 'ai':
          next.default_ai_model = latestCfg.default_ai_model;
          next.ai_terminal_app = latestCfg.ai_terminal_app;
          next.default_ai_dir = latestCfg.default_ai_dir;
          next.ai_model_launch_commands = normalizeAiModelLaunchCommandsForUi(latestCfg.ai_model_launch_commands);
          next.ai_sessions_history_days = latestCfg.ai_sessions_history_days;
          break;
        case 'appearance':
          next.language = latestCfg.language;
          break;
        case 'security':
        default:
          break;
      }
      return next;
    });
  };

  const tabDirtyMap = useMemo<Record<SettingsTab, boolean>>(() => {
    const next = {} as Record<SettingsTab, boolean>;
    SETTINGS_TABS.forEach((tab) => {
      if (tab === 'security') {
        next[tab] = false;
        return;
      }
      const current = getTabSnapshot(tab, config, proxyConfig, newsApiKeys);
      const saved = getTabSnapshot(tab, savedConfig, savedProxyConfig, savedNewsApiKeys);
      next[tab] = JSON.stringify(current) !== JSON.stringify(saved);
    });
    return next;
  }, [config, proxyConfig, savedConfig, savedProxyConfig, newsApiKeys, savedNewsApiKeys]);

  const currentTabDirty = tabDirtyMap[activeTab];
  const hasOtherTabDrafts = SETTINGS_TABS.some((tab) => tab !== activeTab && tabDirtyMap[tab]);

  const saveConfig = async () => {
    if (activeTab === 'security') return;
    setLoading(true);
    setMessage({ type: '', text: '' });
    try {
      const otherTabsDirtyBeforeSave = SETTINGS_TABS.some((tab) => tab !== activeTab && tabDirtyMap[tab]);
      const baseRaw = await invoke<StorageConfig>('get_storage_config');
      const baseConfig = normalizeConfigForUi(baseRaw, t('aiTerminalAppPlaceholder', '终端'));
      const payload = buildPayloadForTab(activeTab, config, proxyConfig, baseConfig);

      await invoke('save_storage_config', { config: payload });

      if (activeTab === 'news') {
        await Promise.all([
          persistNewsApiKey(AI_NEWS_GNEWS_SECRET_KEY, newsApiKeys.gnews),
          persistNewsApiKey(AI_NEWS_NEWSAPI_SECRET_KEY, newsApiKeys.newsapi),
        ]);
      }

      if (activeTab === 'general') {
        const desiredAutostart = !!payload.launch_at_login;
        const currentAutostart = await getAutostartEnabled();
        if (currentAutostart === null || currentAutostart !== desiredAutostart) {
          await setAutostartEnabled(desiredAutostart);
        }
      }

      if (activeTab === 'shortcuts') {
        await invoke('update_shortcuts', {
          main: config.main_shortcut,
          quick: config.quick_ai_shortcut,
        });
      }

      if (activeTab === 'appearance' && config.language && config.language !== baseConfig.language) {
        await invoke('update_tray_menu', { lang: config.language });
      }

      if (activeTab === 'ai') {
        await emit('refresh-counts').catch(console.error);
      }

      const latestRaw = await invoke<StorageConfig>('get_storage_config');
      const latestAutostart = await getAutostartEnabled();
      const latestConfig = normalizeConfigForUi(
        {
          ...latestRaw,
          launch_at_login: latestAutostart ?? (latestRaw.launch_at_login ?? false),
        },
        t('aiTerminalAppPlaceholder', '终端'),
      );
      const latestProxy = normalizeProxyConfigForUi(latestRaw.proxy);
      setSavedConfig(latestConfig);
      setSavedProxyConfig(latestProxy);
      if (activeTab === 'news') {
        const latestKeys = await loadNewsApiKeys();
        setNewsApiKeys(latestKeys);
        setSavedNewsApiKeys(latestKeys);
      }
      syncDraftWithLatestForTab(activeTab, latestConfig, latestProxy);

      const baseText = t('currentSectionSavedSuccess', 'Current section saved.');
      setMessage({
        type: 'success',
        text: otherTabsDirtyBeforeSave
          ? `${baseText} ${t('otherSectionsUnsaved', 'Other sections still have unsaved changes.')}`
          : baseText,
      });
      setTimeout(() => {
        setMessage({ type: '', text: '' });
      }, 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const resetCurrentTab = async () => {
    if (activeTab === 'security') return;
    setLoading(true);
    setMessage({ type: '', text: '' });
    try {
      const otherTabsDirtyBeforeReset = SETTINGS_TABS.some((tab) => tab !== activeTab && tabDirtyMap[tab]);
      const latestRaw = await invoke<StorageConfig>('get_storage_config');
      const latestAutostart = await getAutostartEnabled();
      const latestConfig = normalizeConfigForUi(
        {
          ...latestRaw,
          launch_at_login: latestAutostart ?? (latestRaw.launch_at_login ?? false),
        },
        t('aiTerminalAppPlaceholder', '终端'),
      );
      const latestProxy = normalizeProxyConfigForUi(latestRaw.proxy);
      setSavedConfig(latestConfig);
      setSavedProxyConfig(latestProxy);
      if (activeTab === 'news') {
        const latestKeys = await loadNewsApiKeys();
        setNewsApiKeys(latestKeys);
        setSavedNewsApiKeys(latestKeys);
      }
      syncDraftWithLatestForTab(activeTab, latestConfig, latestProxy);

      const baseText = t('currentSectionResetSuccess', 'Current section has been reset.');
      setMessage({
        type: 'success',
        text: otherTabsDirtyBeforeReset
          ? `${baseText} ${t('otherSectionsUnsaved', 'Other sections still have unsaved changes.')}`
          : baseText,
      });
      setTimeout(() => {
        setMessage({ type: '', text: '' });
      }, 3000);
    } catch (e: any) {
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleSelectDefaultDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setConfig({...config, default_ai_dir: selected});
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  const handleSelectTerminalApp = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        defaultPath: '/Applications',
        filters: [{ name: 'Applications', extensions: ['app'] }],
      });
      if (selected && typeof selected === 'string') {
        const fileName = selected.split('/').pop() || selected;
        const appName = fileName.endsWith('.app') ? fileName.slice(0, -4) : fileName;
        if (appName) {
          setConfig({ ...config, ai_terminal_app: appName });
        }
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  const handleSelectSshKey = async () => {
    try {
      const selected = await open({
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setConfig({...config, ssh_key_path: selected});
      }
    } catch (err: any) {
      console.error(err);
    }
  };

  const handleSelectLocalStoragePath = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setConfig({...config, local_storage_path: selected});
      }
    } catch (err) {
      console.error(err);
    }
  };

  const handleSelectICloudPath = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        if (selected.includes('com~apple~CloudDocs')) {
          setConfig({...config, icloud_storage_path: selected});
        } else {
          setMessage({ type: 'error', text: t('invalidIcloudPath', 'Selected folder must be inside iCloud Drive (com~apple~CloudDocs).') });
        }
      }
    } catch (err) {
      console.error(err);
    }
  };

  const toggleLanguage = async () => {
    const newLang = i18n.language === 'zh' ? 'en' : 'zh';
    await i18n.changeLanguage(newLang);
    setConfig(prev => ({ ...prev, language: newLang }));
  };

  const cycleTheme = () => {
    if (theme === 'system') setTheme('dark');
    else if (theme === 'dark') setTheme('light');
    else setTheme('system');
  };

  const updateSyncPolicy = (key: keyof SyncPolicy, checked: boolean) => {
    setConfig((prev) => ({
      ...prev,
      sync_policy: {
        ...normalizeSyncPolicyForUi(prev.sync_policy),
        [key]: checked,
      },
    }));
  };

  const sidebarItems: { id: SettingsTab; name: string; icon: typeof HardDrive }[] = [
    { id: 'storage', name: t('dataStorageMenu', 'Data Storage'), icon: HardDrive },
    { id: 'news', name: t('newsSettingsMenu', 'News'), icon: Newspaper },
    { id: 'general', name: t('general', 'General'), icon: SettingsIcon },
    { id: 'updates', name: t('updates', 'Updates'), icon: RefreshCw },
    { id: 'skills', name: t('skillsSourcesMenu', 'Skills 源'), icon: Sparkles },
    { id: 'subagents', name: t('subagentsSourcesMenu', 'Subagents 源'), icon: Bot },
    { id: 'proxy', name: t('proxy', 'Network Proxy'), icon: Globe },
    { id: 'shortcuts', name: t('shortcuts', 'Shortcuts'), icon: KeyboardIcon },
    { id: 'ai', name: t('aiSessions', 'AI Terminal'), icon: Terminal },
    { id: 'appearance', name: t('appearance', 'Appearance'), icon: Palette },
    { id: 'security', name: t('security', 'Security'), icon: ShieldCheck },
  ];

  const handleSkillsSyncNow = async () => {
    setSkillsSyncNowLoading(true);
    try {
      await invoke('skills_sync_now');
      await loadSkillsSyncState();
      setMessage({ type: 'success', text: t('syncSuccess', 'Sync successful') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      await loadSkillsSyncState();
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setSkillsSyncNowLoading(false);
    }
  };

  const handleCopySkillSourceRepo = async (repoUrl: string) => {
    try {
      await navigator.clipboard.writeText(repoUrl);
      setMessage({ type: 'success', text: t('copiedToClipboard', 'Copied to clipboard') });
      setTimeout(() => setMessage({ type: '', text: '' }), 1800);
    } catch (e: any) {
      setMessage({ type: 'error', text: e?.toString?.() || String(e) });
    }
  };

  const handleExportSkillSources = async () => {
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      const outputPath = await save({
        defaultPath: `skills-sources-${stamp}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!outputPath || Array.isArray(outputPath)) return;

      await invoke<string>('skills_sources_export_to_path', {
        outputPath,
        skillsSources,
      });
      setMessage({ type: 'success', text: t('skillsSourcesExported', 'Skills sources exported') });
      setTimeout(() => setMessage({ type: '', text: '' }), 1800);
    } catch (e: any) {
      setMessage({ type: 'error', text: e?.toString?.() || String(e) });
    }
  };

  const handleImportSkillSources = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;

    try {
      const rawText = await file.text();
      const parsed = JSON.parse(rawText);
      const inputSources = Array.isArray(parsed)
        ? parsed
        : Array.isArray(parsed?.skills_sources)
          ? parsed.skills_sources
          : Array.isArray(parsed?.sources)
            ? parsed.sources
            : null;

      if (!inputSources) {
        throw new Error(t('invalidSkillsSourcesJson', 'Invalid JSON format. Expected an array or { skills_sources: [] }.'));
      }

      const normalizedSources: SkillSourceConfig[] = inputSources.map((source: any) => ({
        id: String(source?.id ?? '').trim(),
        name: String(source?.name ?? ''),
        repo_url: String(source?.repo_url ?? source?.repoUrl ?? '').trim(),
        branch: String(source?.branch ?? 'main').trim() || 'main',
        base_dir: String(source?.base_dir ?? source?.baseDir ?? '/').trim() || '/',
        enabled: source?.enabled !== false,
        default_models: Array.isArray(source?.default_models)
          ? source.default_models.filter((m: unknown) => typeof m === 'string')
          : ['claude', 'gemini', 'codex', 'opencode'],
      }));

      const duplicateIds = new Set<string>();
      const seenIds = new Set<string>();
      normalizedSources.forEach((source) => {
        if (seenIds.has(source.id)) duplicateIds.add(source.id);
        seenIds.add(source.id);
      });
      if (duplicateIds.size > 0) {
        throw new Error(
          t('skillsImportDuplicateIds', 'Duplicate source IDs in import file: {{ids}}', { ids: Array.from(duplicateIds).join(', ') }),
        );
      }

      for (let i = 0; i < normalizedSources.length; i += 1) {
        const source = normalizedSources[i];
        const validation = validateSkillSource(source, []);
        const errors = Object.values(validation).filter(Boolean);
        if (errors.length > 0) {
          throw new Error(
            t('skillsImportItemInvalid', 'Import item #{{index}} invalid: {{message}}', {
              index: i + 1,
              message: errors.join(' '),
            }),
          );
        }
      }

      setConfig((prev) => ({ ...prev, skills_sources: normalizedSources }));
      setMessage({
        type: 'success',
        text: t('skillsSourcesImported', 'Imported {{count}} skills sources', { count: normalizedSources.length }),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 2200);
    } catch (e: any) {
      setMessage({ type: 'error', text: e?.toString?.() || String(e) });
    }
  };

  const handleSubagentsSyncNow = async () => {
    setSubagentsSyncNowLoading(true);
    try {
      await invoke('subagents_sync_now');
      await loadSubagentsSyncState();
      setMessage({ type: 'success', text: t('syncSuccess', 'Sync successful') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      await loadSubagentsSyncState();
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setSubagentsSyncNowLoading(false);
    }
  };

  const handleDiagnoseSubagentSource = async (sourceId: string) => {
    if (!sourceId) return;
    setSubagentSourceDiagnosing((prev) => ({ ...prev, [sourceId]: true }));
    try {
      const resp = await invoke<ApiResp<SubagentSourceDiagnoseResult>>('subagents_source_diagnose', {
        input: { source_id: sourceId, sync_first: true },
      });
      const result = resp.data;
      setSubagentSourceDiagnostics((prev) => ({ ...prev, [sourceId]: result }));
      setMessage({
        type: 'success',
        text: t(
          'subagentsDiagnoseSummary',
          'Diagnosis done: scanned {{total}}, accepted {{accepted}}, skipped {{skipped}}',
          {
            total: result.total_entries || 0,
            accepted: result.accepted_entries || 0,
            skipped: result.skipped_entries || 0,
          },
        ),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 3200);
    } catch (e: any) {
      setMessage({
        type: 'error',
        text: t('subagentsDiagnoseFailed', 'Subagents source diagnosis failed: {{message}}', {
          message: e?.toString?.() || String(e),
        }),
      });
    } finally {
      setSubagentSourceDiagnosing((prev) => {
        const next = { ...prev };
        delete next[sourceId];
        return next;
      });
    }
  };

  const handleExportSubagentSources = async () => {
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      const outputPath = await save({
        defaultPath: `subagents-sources-${stamp}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!outputPath || Array.isArray(outputPath)) return;

      await invoke<string>('subagents_sources_export_to_path', {
        outputPath,
        subagentsSources,
      });
      setMessage({ type: 'success', text: t('subagentsSourcesExported', 'Subagents sources exported') });
      setTimeout(() => setMessage({ type: '', text: '' }), 1800);
    } catch (e: any) {
      setMessage({ type: 'error', text: e?.toString?.() || String(e) });
    }
  };

  const handleImportSubagentSources = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;

    try {
      const rawText = await file.text();
      const parsed = JSON.parse(rawText);
      const inputSources = Array.isArray(parsed)
        ? parsed
        : Array.isArray(parsed?.subagents_sources)
          ? parsed.subagents_sources
          : Array.isArray(parsed?.sources)
            ? parsed.sources
            : null;

      if (!inputSources) {
        throw new Error(t('invalidSubagentsSourcesJson', 'Invalid JSON format. Expected an array or { subagents_sources: [] }.'));
      }

      const normalizedSources: SkillSourceConfig[] = inputSources.map((source: any) => ({
        id: String(source?.id ?? '').trim(),
        name: String(source?.name ?? ''),
        repo_url: String(source?.repo_url ?? source?.repoUrl ?? '').trim(),
        branch: String(source?.branch ?? 'main').trim() || 'main',
        base_dir: String(source?.base_dir ?? source?.baseDir ?? '/').trim() || '/',
        enabled: source?.enabled !== false,
        default_models: Array.isArray(source?.default_models)
          ? source.default_models.filter((m: unknown) => typeof m === 'string')
          : ['claude', 'gemini', 'codex', 'opencode'],
      }));

      const duplicateIds = new Set<string>();
      const seenIds = new Set<string>();
      normalizedSources.forEach((source) => {
        if (seenIds.has(source.id)) duplicateIds.add(source.id);
        seenIds.add(source.id);
      });
      if (duplicateIds.size > 0) {
        throw new Error(
          t('subagentsImportDuplicateIds', 'Duplicate source IDs in import file: {{ids}}', { ids: Array.from(duplicateIds).join(', ') }),
        );
      }

      for (let i = 0; i < normalizedSources.length; i += 1) {
        const source = normalizedSources[i];
        const validation = validateSkillSource(source, []);
        const errors = Object.values(validation).filter(Boolean);
        if (errors.length > 0) {
          throw new Error(
            t('subagentsImportItemInvalid', 'Import item #{{index}} invalid: {{message}}', {
              index: i + 1,
              message: errors.join(' '),
            }),
          );
        }
      }

      setConfig((prev) => ({ ...prev, subagents_sources: normalizedSources }));
      setMessage({
        type: 'success',
        text: t('subagentsSourcesImported', 'Imported {{count}} subagents sources', { count: normalizedSources.length }),
      });
      setTimeout(() => setMessage({ type: '', text: '' }), 2200);
    } catch (e: any) {
      setMessage({ type: 'error', text: e?.toString?.() || String(e) });
    }
  };

  const ThemeIcon = theme === 'system' ? Monitor : theme === 'dark' ? Moon : Sun;
  const skillsSources = config.skills_sources || [];
  const subagentsSources = config.subagents_sources || [];
  const skillsSyncSourceMap = new Map((skillsSyncState?.sources || []).map((s) => [s.source_id, s]));
  const subagentsSyncSourceMap = new Map((subagentsSyncState?.sources || []).map((s) => [s.source_id, s]));
  const enabledSkillsSources = skillsSources.filter((s) => s.enabled).length;
  const enabledSubagentsSources = subagentsSources.filter((s) => s.enabled).length;
  const lastSkillsSyncText = config.skills_last_synced_at
    ? new Date(config.skills_last_synced_at * 1000).toLocaleString()
    : t('never', 'Never');
  const lastSubagentsSyncText = config.subagents_last_synced_at
    ? new Date(config.subagents_last_synced_at * 1000).toLocaleString()
    : t('never', 'Never');
  const lastAiNewsSyncText = config.ai_news_last_synced_at
    ? new Date(config.ai_news_last_synced_at * 1000).toLocaleString()
    : t('never', 'Never');
  const formatSyncTs = (ts?: number) => (ts ? new Date(ts * 1000).toLocaleString() : t('never', 'Never'));
  const syncScopeConfigurable = config.storage_type !== 'local';
  const syncScopeItems: {
    key: keyof SyncPolicy;
    titleKey: string;
    titleFallback: string;
    descKey: string;
    descFallback: string;
  }[] = [
    {
      key: 'providers',
      titleKey: 'syncScopeProviders',
      titleFallback: 'AI Environments',
      descKey: 'syncScopeProvidersDesc',
      descFallback: 'Sync provider profiles, active selections, runtime policy, and profile history. API keys and other sensitive fields remain local.',
    },
    {
      key: 'mcp',
      titleKey: 'syncScopeMcp',
      titleFallback: 'MCP Servers',
      descKey: 'syncScopeMcpDesc',
      descFallback: 'Sync MCP server definitions and provider links. Sensitive env/header values are synced as placeholders, and local model switch state is excluded.',
    },
    {
      key: 'content',
      titleKey: 'syncScopeContent',
      titleFallback: 'Content Data',
      descKey: 'syncScopeContentDesc',
      descFallback: 'Sync encrypted notes, bookmarks, and snippets only.',
    },
    {
      key: 'workflow_presets',
      titleKey: 'syncScopeWorkflowPresets',
      titleFallback: 'Workflow Presets',
      descKey: 'syncScopeWorkflowPresetsDesc',
      descFallback: 'Sync workflow preset definitions (workflow_presets.json), excluding workflow run history.',
    },
    {
      key: 'skills_sources',
      titleKey: 'syncScopeSkillsSources',
      titleFallback: 'Skills Sources',
      descKey: 'syncScopeSkillsSourcesDesc',
      descFallback: 'Sync shared-profile data for Skills sources. This file is shared with Subagents sources and sync settings.',
    },
    {
      key: 'skills_repository',
      titleKey: 'syncScopeSkillsRepository',
      titleFallback: 'Skills Repository',
      descKey: 'syncScopeSkillsRepositoryDesc',
      descFallback: 'Sync data/skills repository snapshots and metadata (repository, index baselines, sync state), excluding local install records and remote cache.',
    },
    {
      key: 'subagents_sources',
      titleKey: 'syncScopeSubagentsSources',
      titleFallback: 'Subagents Sources',
      descKey: 'syncScopeSubagentsSourcesDesc',
      descFallback: 'Sync shared-profile data for Subagents sources. This file is shared with Skills sources and sync settings.',
    },
    {
      key: 'subagents_repository',
      titleKey: 'syncScopeSubagentsRepository',
      titleFallback: 'Subagents Repository',
      descKey: 'syncScopeSubagentsRepositoryDesc',
      descFallback: 'Sync data/subagents repository snapshots and metadata (repository, index baselines, sync state), excluding local install records and remote cache.',
    },
    {
      key: 'ai_news',
      titleKey: 'syncScopeAiNews',
      titleFallback: 'AI News',
      descKey: 'syncScopeAiNewsDesc',
      descFallback: 'When enabled, OneSpace syncs plaintext AI news records across devices only after a fetch adds new items. API keys remain local and encrypted.',
    },
  ];

  return (
    <div className="flex h-full flex-col bg-background animate-in fade-in slide-in-from-right-4 duration-300">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b shrink-0 bg-card/30 backdrop-blur-sm sticky top-0 z-10">
        <div className="flex items-center gap-4">
          <button 
            onClick={onBack}
            className="p-2 rounded-full hover:bg-muted text-muted-foreground transition-all active:scale-95"
          >
            <ChevronLeft className="w-5 h-5" />
          </button>
          <div className="flex items-center gap-2">
            <SettingsIcon className="w-5 h-5 text-primary" />
            <h1 className="text-xl font-bold tracking-tight">{t('settings')}</h1>
          </div>
        </div>
        
        <div className="flex items-center gap-2">
          {message.text && (
            <div className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium animate-in zoom-in-95 ${
              message.type === 'error' ? 'bg-destructive/10 text-destructive border border-destructive/20' : 'bg-green-500/10 text-green-600 border border-green-500/20'
            }`}>
              {message.type === 'error' ? <AlertCircle className="w-3.5 h-3.5" /> : <CheckCircle2 className="w-3.5 h-3.5" />}
              {message.text}
            </div>
          )}
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <div className="w-64 border-r bg-muted/20 flex flex-col shrink-0 p-4 space-y-1">
          {sidebarItems.map(item => (
            <button
              key={item.id}
              onClick={() => setActiveTab(item.id)}
              className={`w-full flex items-center gap-3 px-4 py-2.5 rounded-xl text-sm transition-all ${
                activeTab === item.id 
                  ? 'bg-primary text-primary-foreground font-medium shadow-md' 
                  : 'hover:bg-muted text-muted-foreground hover:text-foreground'
              }`}
            >
              <item.icon className={`w-4 h-4 ${activeTab === item.id ? 'animate-pulse' : ''}`} />
              <span className="truncate">{item.name}</span>
              {tabDirtyMap[item.id] && (
                <span
                  className="ml-auto h-2 w-2 rounded-full bg-amber-500"
                  title={t('currentSectionUnsaved', 'Unsaved changes in this section')}
                />
              )}
            </button>
          ))}
        </div>

        {/* Content Area */}
        <div className="flex-1 bg-background/50 flex min-w-0 flex-col">
          <div className="shrink-0 border-b bg-card/70 backdrop-blur-sm px-8 py-3">
            <div className="max-w-3xl mx-auto flex items-center justify-between gap-3">
              <div className={`text-xs font-medium inline-flex items-center gap-2 ${
                currentTabDirty ? 'text-amber-700' : 'text-muted-foreground'
              }`}>
                {currentTabDirty ? <AlertCircle className="w-3.5 h-3.5" /> : <CheckCircle2 className="w-3.5 h-3.5" />}
                {currentTabDirty
                  ? t('currentSectionUnsaved', 'Unsaved changes in this section')
                  : t('currentSectionSaved', 'No unsaved changes in this section')}
              </div>
              {activeTab !== 'security' && (
                <div className="flex items-center gap-2">
                  <button
                    onClick={resetCurrentTab}
                    disabled={loading || !currentTabDirty}
                    className="flex items-center gap-2 px-4 py-2 border bg-background hover:bg-muted rounded-lg disabled:opacity-50 transition-all font-semibold active:scale-95"
                  >
                    {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Undo2 className="w-4 h-4" />}
                    {t('resetCurrentTab', 'Reset')}
                  </button>
                  <button
                    onClick={saveConfig}
                    disabled={loading || !currentTabDirty}
                    className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg disabled:opacity-50 transition-all font-semibold shadow-sm active:scale-95"
                  >
                    {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
                    {t('saveCurrentTab', 'Save Settings')}
                  </button>
                </div>
              )}
            </div>
            {hasOtherTabDrafts && (
              <div className="max-w-3xl mx-auto mt-2 text-[11px] text-amber-700">
                {t('otherSectionsUnsaved', 'Other sections still have unsaved changes.')}
              </div>
            )}
          </div>

          <div className="flex-1 overflow-y-auto p-8">
          <div className="max-w-3xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-500">
            
            {activeTab === 'storage' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('dataStorageMenu', 'Data Storage')}</h2>
                    <p className="text-sm text-muted-foreground">{t('dataStorageDesc', 'Configure where OneSpace data is saved and synced.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('storageType', 'Storage Type')}</label>
                      <div className="grid grid-cols-3 gap-2 p-1 bg-muted rounded-xl border">
                        <button 
                          onClick={() => setConfig({...config, storage_type: 'local'})}
                          className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${config.storage_type === 'local' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                        >
                          {t('local', 'Local')}
                        </button>
                        <button 
                          onClick={() => setConfig({...config, storage_type: 'icloud'})}
                          className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${config.storage_type === 'icloud' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                        >
                          {t('icloud', 'iCloud Drive')}
                        </button>
                        <button 
                          onClick={() => setConfig({...config, storage_type: 'git'})}
                          className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${config.storage_type === 'git' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'}`}
                        >
                          {t('gitRepo', 'Git Repository')}
                        </button>
                      </div>
                    </div>

                    <hr className="border-border/50" />

                    <div className="space-y-3">
                      <div className="space-y-1">
                        <h3 className="text-sm font-medium">{t('syncScopeTitle', 'Sync Data Scope')}</h3>
                        <p className="text-xs text-muted-foreground">
                          {t('syncScopeDesc', 'Choose which data domains should be synchronized across storage backends.')}
                        </p>
                      </div>
                      {syncScopeConfigurable ? (
                        <div className="space-y-3">
                          {syncScopeItems.map((item) => (
                            <div key={item.key} className="flex items-center justify-between gap-4 rounded-xl border bg-muted/20 px-4 py-3">
                              <div className="space-y-0.5">
                                <h4 className="text-sm font-medium">{t(item.titleKey, item.titleFallback)}</h4>
                                <p className="text-xs text-muted-foreground">{t(item.descKey, item.descFallback)}</p>
                              </div>
                              <Switch
                                checked={!!normalizeSyncPolicyForUi(config.sync_policy)[item.key]}
                                onCheckedChange={(checked) => updateSyncPolicy(item.key, checked)}
                              />
                            </div>
                          ))}
                        </div>
                      ) : (
                        <div className="rounded-xl border border-dashed bg-muted/20 px-4 py-3 text-xs text-muted-foreground">
                          {t(
                            'syncScopeLocalHint',
                            'Local mode stores data directly on this device. Sync scope is available when using iCloud or Git storage.',
                          )}
                        </div>
                      )}
                    </div>

                    {config.storage_type === 'icloud' && (
                      <div className="space-y-4 pt-4 animate-in fade-in zoom-in-95">
                        <div className="p-4 bg-primary/5 rounded-xl border border-primary/10">
                          <p className="text-sm text-primary/80">
                            {t('icloudDesc', 'Your data will be stored securely in iCloud Drive and synced automatically across your devices.')}
                          </p>
                        </div>
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('icloudStoragePath', 'iCloud Storage Path')}</label>
                          <div className="flex gap-2">
                            <input 
                              type="text" 
                              placeholder="~/Library/Mobile Documents/com~apple~CloudDocs/onespace"
                              className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all font-mono"
                              value={config.icloud_storage_path || ''}
                              onChange={e => setConfig({...config, icloud_storage_path: e.target.value})}
                            />
                            <button 
                              onClick={handleSelectICloudPath}
                              className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                            >
                              <FolderOpen className="w-4 h-4" />
                            </button>
                          </div>
                          <p className="text-[10px] text-muted-foreground leading-relaxed text-yellow-600 dark:text-yellow-500">
                            {t('icloudStoragePathNote', 'Path must be inside iCloud Drive (com~apple~CloudDocs). Changing this will migrate existing data.')}
                          </p>
                        </div>
                      </div>
                    )}

                    {config.storage_type === 'local' && (
                      <div className="space-y-4 pt-4 animate-in fade-in zoom-in-95">
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('localStoragePath', 'Local Storage Path')}</label>
                          <div className="flex gap-2">
                            <input 
                              type="text" 
                              placeholder="~/.config/onespace/data"
                              className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all font-mono"
                              value={config.local_storage_path || ''}
                              onChange={e => setConfig({...config, local_storage_path: e.target.value})}
                            />
                            <button 
                              onClick={handleSelectLocalStoragePath}
                              className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                            >
                              <FolderOpen className="w-4 h-4" />
                            </button>
                          </div>
                          <p className="text-[10px] text-muted-foreground leading-relaxed">
                            {t('localStoragePathNote', 'Default: ~/.config/onespace/data. Changing this will migrate existing local data.')}
                          </p>
                        </div>
                      </div>
                    )}

                    {config.storage_type === 'git' && (
                      <div className="space-y-4 pt-4 animate-in fade-in zoom-in-95">
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('remoteUrl', 'Remote URL')}</label>
                          <input 
                            type="text" 
                            placeholder="https://github.com/user/repo.git"
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all"
                            value={config.git_url || ''}
                            onChange={e => setConfig({...config, git_url: e.target.value})}
                          />
                        </div>

                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('authMethod', 'Authentication Method')}</label>
                          <select 
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                            value={config.auth_method || 'http'}
                            onChange={e => setConfig({...config, auth_method: e.target.value as 'http' | 'ssh'})}
                          >
                            <option value="http">{t('httpToken', 'HTTP Token')}</option>
                            <option value="ssh">{t('sshKey', 'SSH Key')}</option>
                          </select>
                        </div>

                        {config.auth_method === 'http' && (
                          <div className="grid grid-cols-2 gap-4 animate-in fade-in slide-in-from-top-2">
                            <div className="space-y-2">
                              <label className="text-sm font-medium text-muted-foreground">{t('username', 'Username')}</label>
                              <div className="relative">
                                <User className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                                <input 
                                  type="text"
                                  className="w-full bg-background border rounded-xl pl-10 pr-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={config.http_username || ''}
                                  onChange={e => setConfig({...config, http_username: e.target.value})}
                                />
                              </div>
                            </div>
                            <div className="space-y-2">
                              <label className="text-sm font-medium text-muted-foreground">{t('token', 'Token / Password')}</label>
                              <div className="relative">
                                <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                                <input 
                                  type="password"
                                  className="w-full bg-background border rounded-xl pl-10 pr-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={config.http_token || ''}
                                  onChange={e => setConfig({...config, http_token: e.target.value})}
                                />
                              </div>
                            </div>
                          </div>
                        )}

                        {config.auth_method === 'ssh' && (
                          <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                            <label className="text-sm font-medium text-muted-foreground">{t('sshKeyPath', 'SSH Private Key Path')}</label>
                            <div className="flex gap-2">
                              <div className="relative flex-1">
                                <Key className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                                <input 
                                  type="text"
                                  placeholder={t('chooseSshKey', 'Choose SSH key file...')}
                                  className="w-full bg-background border rounded-xl pl-10 pr-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 font-mono"
                                  value={config.ssh_key_path || ''}
                                  onChange={e => setConfig({...config, ssh_key_path: e.target.value})}
                                />
                              </div>
                              <button 
                                onClick={handleSelectSshKey}
                                className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                              >
                                <FolderOpen className="w-4 h-4" />
                              </button>
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'news' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('newsSettingsMenu', 'News')}</h2>
                    <p className="text-sm text-muted-foreground">
                      {t('newsSettingsDesc', 'Configure AI news fetching, retention, and sync behavior.')}
                    </p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('newsEnabled', 'Enable AI News')}</h3>
                        <p className="text-xs text-muted-foreground">
                          {t('newsEnabledDesc', 'When enabled, OneSpace fetches latest AI news in the background.')}
                        </p>
                      </div>
                      <Switch
                        checked={!!config.ai_news_enabled}
                        onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, ai_news_enabled: checked }))}
                      />
                    </div>

                    {config.ai_news_enabled && (
                      <>
                        <hr className="border-border/50" />

                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">
                            {t('newsSyncInterval', 'Fetch Interval (minutes)')}
                          </label>
                          <input
                            type="number"
                            min={5}
                            max={1440}
                            step={5}
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                            value={config.ai_news_sync_interval_minutes ?? 60}
                            onChange={(e) => {
                              const raw = parseInt(e.target.value, 10);
                              const value = Number.isFinite(raw) ? Math.max(5, Math.min(1440, raw)) : 60;
                              setConfig((prev) => ({ ...prev, ai_news_sync_interval_minutes: value }));
                            }}
                          />
                          <p className="text-xs text-muted-foreground">
                            {t('newsLastFetchedAt', 'Last fetched at: {{time}}', { time: lastAiNewsSyncText })}
                          </p>
                        </div>

                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">
                            {t('newsKeywords', 'News Keywords')}
                          </label>
                          <textarea
                            rows={3}
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                            placeholder={t(
                              'newsKeywordsPlaceholder',
                              'Use comma/newline separated keywords, e.g. OpenAI, Anthropic, Gemini',
                            )}
                            value={config.ai_news_keywords ?? ''}
                            onChange={(e) => {
                              setConfig((prev) => ({ ...prev, ai_news_keywords: e.target.value }));
                            }}
                          />
                          <p className="text-xs text-muted-foreground">
                            {t(
                              'newsKeywordsDesc',
                              'GNews query supports comma/newline-separated keywords and OR/AND expressions.',
                            )}
                          </p>
                        </div>

                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">
                            {t('newsRetentionPolicy', 'Retention Policy')}
                          </label>
                          <select
                            className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                            value={newsRetentionPreset}
                            onChange={(e) => {
                              const value = e.target.value as NewsRetentionPreset;
                              setNewsRetentionPreset(value);
                              if (value === '7d_200') {
                                setConfig((prev) => ({ ...prev, ai_news_retention_days: 7, ai_news_retention_max_items: 200 }));
                              } else if (value === '30d_500') {
                                setConfig((prev) => ({ ...prev, ai_news_retention_days: 30, ai_news_retention_max_items: 500 }));
                              } else if (value === '90d_1000') {
                                setConfig((prev) => ({ ...prev, ai_news_retention_days: 90, ai_news_retention_max_items: 1000 }));
                              }
                            }}
                          >
                            <option value="7d_200">{t('newsRetentionPreset7d200', '7 days + 200 items')}</option>
                            <option value="30d_500">{t('newsRetentionPreset30d500', '30 days + 500 items')}</option>
                            <option value="90d_1000">{t('newsRetentionPreset90d1000', '90 days + 1000 items')}</option>
                            <option value="custom">{t('custom', 'Custom')}</option>
                          </select>
                        </div>

                        <div className="grid grid-cols-2 gap-4">
                          <div className="space-y-2">
                            <label className="text-sm font-medium text-muted-foreground">
                              {t('newsRetentionDays', 'Retention Days')}
                            </label>
                            <input
                              type="number"
                              min={1}
                              max={3650}
                              step={1}
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={config.ai_news_retention_days ?? 90}
                              onChange={(e) => {
                                const raw = parseInt(e.target.value, 10);
                                const value = Number.isFinite(raw) ? Math.max(1, Math.min(3650, raw)) : 90;
                                setNewsRetentionPreset('custom');
                                setConfig((prev) => ({ ...prev, ai_news_retention_days: value }));
                              }}
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-medium text-muted-foreground">
                              {t('newsRetentionMaxItems', 'Max Items')}
                            </label>
                            <input
                              type="number"
                              min={10}
                              max={100000}
                              step={10}
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={config.ai_news_retention_max_items ?? 1000}
                              onChange={(e) => {
                                const raw = parseInt(e.target.value, 10);
                                const value = Number.isFinite(raw) ? Math.max(10, Math.min(100000, raw)) : 1000;
                                setNewsRetentionPreset('custom');
                                setConfig((prev) => ({ ...prev, ai_news_retention_max_items: value }));
                              }}
                            />
                          </div>
                        </div>

                        <hr className="border-border/50" />

                        <div className="space-y-3">
                          <h3 className="text-sm font-medium">{t('newsApiKeys', 'API Keys')}</h3>
                          <div className="space-y-2">
                            <label className="text-sm font-medium text-muted-foreground">GNews API Key</label>
                            <input
                              type="password"
                              autoComplete="off"
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 font-mono"
                              placeholder="Enter GNews API key"
                              value={newsApiKeys.gnews}
                              onChange={(e) => setNewsApiKeys((prev) => ({ ...prev, gnews: e.target.value }))}
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-medium text-muted-foreground">NewsAPI Key</label>
                            <input
                              type="password"
                              autoComplete="off"
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 font-mono"
                              placeholder="Enter NewsAPI key"
                              value={newsApiKeys.newsapi}
                              onChange={(e) => setNewsApiKeys((prev) => ({ ...prev, newsapi: e.target.value }))}
                            />
                          </div>
                        </div>
                      </>
                    )}
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'updates' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('updates', 'Updates')}</h2>
                    <p className="text-sm text-muted-foreground">{t('updatesDesc', 'Configure automatic version checks and background update downloads.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('autoUpdate', 'Automatic Updates')}</h3>
                        <p className="text-xs text-muted-foreground">{t('autoUpdateDesc', 'When enabled, OneSpace will silently check and download updates in the background.')}</p>
                      </div>
                      <Switch
                        checked={!!config.auto_update_enabled}
                        onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, auto_update_enabled: checked }))}
                      />
                    </div>

                    <hr className="border-border/50" />

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('updateCheckFrequency', 'Check Frequency (minutes)')}</label>
                      <input
                        type="number"
                        min={30}
                        max={1440}
                        step={10}
                        className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                        value={config.update_check_interval_minutes ?? 360}
                        onChange={(e) => {
                          const raw = parseInt(e.target.value, 10);
                          const value = Number.isFinite(raw) ? Math.max(30, Math.min(1440, raw)) : 360;
                          setConfig((prev) => ({ ...prev, update_check_interval_minutes: value }));
                        }}
                      />
                      <p className="text-xs text-muted-foreground">{t('updateCheckFrequencyDesc', 'Recommended range: 30 to 1440 minutes.')}</p>
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'general' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('general', 'General')}</h2>
                    <p className="text-sm text-muted-foreground">{t('generalDesc', 'Configure app-wide behavior settings.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('launchAtLogin', 'Launch at Login')}</h3>
                        <p className="text-xs text-muted-foreground">{t('launchAtLoginDesc', 'Start OneSpace automatically after system login, and keep it in tray by default.')}</p>
                      </div>
                      <Switch
                        checked={!!config.launch_at_login}
                        onCheckedChange={(checked) => setConfig((prev) => ({ ...prev, launch_at_login: checked }))}
                      />
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'skills' && (
              <div className="space-y-6">
                <section className="space-y-4">
                    <div className="flex items-center justify-between gap-3">
                      <div className="flex flex-col gap-1">
                        <h2 className="text-lg font-semibold">{t('skillsSourcesMenu', 'Skills 源')}</h2>
                        <p className="text-sm text-muted-foreground">{t('skillsSyncEnabledDesc', 'Global switch for scheduled Git repository skills sync.')}</p>
                      <div className="flex items-center gap-2 pt-1 text-xs text-muted-foreground">
                        <span className="px-2 py-0.5 rounded-full border bg-muted/40">
                          {t('lastSyncAt', 'Last Sync')}: {lastSkillsSyncText}
                        </span>
                        <span className="px-2 py-0.5 rounded-full border bg-muted/40">
                          {t('sources', 'Sources')}: {enabledSkillsSources}/{skillsSources.length}
                        </span>
                      </div>
                    </div>
                    <button
                      onClick={handleSkillsSyncNow}
                      disabled={loading || skillsSyncNowLoading}
                      className="px-3 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 disabled:opacity-50 inline-flex items-center gap-2"
                    >
                      {skillsSyncNowLoading && <RefreshCw className="w-4 h-4 animate-spin" />}
                      {t('syncNow', 'Sync Now')}
                    </button>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                      <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('skillsSyncEnabled', 'Enable Skills Auto Sync')}</h3>
                        <p className="text-xs text-muted-foreground">{t('skillsSyncEnabledDesc', 'Global switch for scheduled Git repository skills sync.')}</p>
                      </div>
                      <label className="relative inline-flex items-center cursor-pointer">
                        <input
                          type="checkbox"
                          className="sr-only peer"
                          checked={!!config.skills_sync_enabled}
                          onChange={(e) => setConfig((prev) => ({ ...prev, skills_sync_enabled: e.target.checked }))}
                        />
                        <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                      </label>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('skillsSyncInterval', 'Skills Sync Interval (minutes)')}</label>
                      <input
                        type="number"
                        min={5}
                        max={1440}
                        step={5}
                        disabled={!config.skills_sync_enabled}
                        className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 disabled:opacity-60"
                        value={config.skills_sync_interval_minutes ?? 60}
                        onChange={(e) => {
                          const raw = parseInt(e.target.value, 10);
                          const value = Number.isFinite(raw) ? Math.max(5, Math.min(1440, raw)) : 60;
                          setConfig((prev) => ({ ...prev, skills_sync_interval_minutes: value }));
                        }}
                      />
                      <p className="text-xs text-muted-foreground">
                        {config.skills_sync_enabled
                          ? t('skillsSyncIntervalDesc', 'Scheduled sync uses this interval.')
                          : t('skillsSyncDisabledHint', 'Auto sync is off. You can still click Sync Now manually.')}
                      </p>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">
                        {t('skillsNewBadgeHours', 'New Skill Badge Duration (hours)')}
                      </label>
                      <input
                        type="number"
                        min={1}
                        max={720}
                        step={1}
                        className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                        value={config.skills_new_badge_hours ?? 72}
                        onChange={(e) => {
                          const raw = parseInt(e.target.value, 10);
                          const value = Number.isFinite(raw) ? Math.max(1, Math.min(720, raw)) : 72;
                          setConfig((prev) => ({ ...prev, skills_new_badge_hours: value }));
                        }}
                      />
                      <p className="text-xs text-muted-foreground">
                        {t('skillsNewBadgeHoursDesc', 'Recommended skills marked as New will auto-hide after this duration. Default is 72 hours.')}
                      </p>
                    </div>

                    <hr className="border-border/50" />

                    <div className="space-y-2">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <h4 className="text-sm font-medium text-muted-foreground">{t('skillsSources', 'Git Repository Skills Sources')}</h4>
                        <div className="flex items-center gap-2">
                          <input
                            ref={skillsImportInputRef}
                            type="file"
                            accept="application/json,.json"
                            onChange={handleImportSkillSources}
                            className="hidden"
                          />
                          <button
                            type="button"
                            onClick={() => skillsImportInputRef.current?.click()}
                            className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border bg-background hover:bg-muted text-xs"
                          >
                            <Upload className="w-3.5 h-3.5" />
                            {t('import', 'Import')}
                          </button>
                          <button
                            type="button"
                            onClick={handleExportSkillSources}
                            className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border bg-background hover:bg-muted text-xs"
                          >
                            <Download className="w-3.5 h-3.5" />
                            {t('export', 'Export')}
                          </button>
                          <button
                            type="button"
                            onClick={() => {
                              resetNewSkillSourceForm();
                              setShowAddSkillSourceModal(true);
                            }}
                            className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm flex items-center gap-2 hover:bg-primary/90"
                          >
                            <Plus className="w-4 h-4" />
                            {t('addSource', 'Add Source')}
                          </button>
                        </div>
                      </div>

                      <div className="space-y-2">
                        {skillsSources.length === 0 && (
                          <div className="rounded-md border border-dashed p-4 text-xs text-muted-foreground bg-muted/10">
                            {t('noSkillSources', 'No Git repository source configured yet. Add one above to enable catalog sync.')}
                          </div>
                        )}
                        {skillsSources.map((source, idx) => {
                          const syncInfo = skillsSyncSourceMap.get(source.id);
                          const syncFailed = !!syncInfo?.last_error || !!syncInfo?.last_status?.includes('error');
                          const syncSucceeded = !syncFailed && !!syncInfo?.last_synced_at;
                          const syncToneClass = syncFailed
                            ? 'text-destructive'
                            : syncSucceeded
                              ? 'text-emerald-600 dark:text-emerald-400'
                              : 'text-muted-foreground';
                          const syncMessage = syncFailed
                            ? t('skillsSourceSyncFailed', 'Sync failed: {{message}}', {
                                message: syncInfo?.last_error || syncInfo?.last_status || t('unknownError', 'Unknown error'),
                              })
                            : syncSucceeded
                              ? t('skillsSourceSyncSuccessAt', 'Sync successful: {{time}}', {
                                time: formatSyncTs(syncInfo.last_synced_at),
                              })
                              : t('skillsSourceSyncNever', 'Not synced yet');
                          return (
                          <div key={source.id || `${idx}`} className="group relative flex flex-col justify-between p-4 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 overflow-hidden">
                            <div className={`absolute top-0 left-0 w-1 h-full transition-colors ${source.enabled ? 'bg-primary/0 group-hover:bg-primary' : 'bg-muted group-hover:bg-muted-foreground/40'}`}></div>
                            <div className="flex items-start justify-between gap-3">
                              <div className="space-y-1 min-w-0">
                                <div className="text-sm font-semibold truncate">{source.name || source.id || t('untitledSource', 'Untitled Source')}</div>
                                <div className="text-xs text-muted-foreground">
                                  {t('sourceId', 'Source ID')}: <span className="font-mono">{source.id || '-'}</span>
                                </div>
                              </div>
                              <label className="inline-flex items-center gap-1.5 text-xs shrink-0 cursor-pointer">
                                <input
                                  type="checkbox"
                                  className="sr-only peer"
                                  checked={!!source.enabled}
                                  onChange={(e) => updateSkillSource(source.id, { enabled: e.target.checked })}
                                />
                                <div className="w-10 h-5 bg-gray-200 rounded-full relative transition-colors peer-checked:bg-primary dark:bg-gray-700 peer-focus:ring-2 peer-focus:ring-primary/20 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:w-4 after:h-4 after:bg-white after:border after:rounded-full after:transition-all peer-checked:after:translate-x-5"></div>
                                <span>{t('enabled', 'Enabled')}</span>
                              </label>
                            </div>

                            <div className="mt-2 rounded-lg border bg-muted/20 p-2.5 space-y-1.5">
                              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                <span className="inline-block w-16 uppercase tracking-wider opacity-70">{t('branch', 'Branch')}</span>
                                <span className="font-mono text-foreground/80">{source.branch || 'main'}</span>
                              </div>
                              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                <span className="inline-block w-16 uppercase tracking-wider opacity-70">{t('baseDir', 'Base Directory')}</span>
                                <span className="font-mono text-foreground/80">{source.base_dir || '/'}</span>
                              </div>
                              <div className="flex items-start gap-2 text-xs text-muted-foreground group/repo">
                                <span className="inline-block w-16 uppercase tracking-wider opacity-70 pt-0.5">{t('repoUrl', 'Repo URL')}</span>
                                <div className="min-w-0 flex-1 flex items-start gap-1.5">
                                  <a
                                    href={source.repo_url}
                                    target="_blank"
                                    rel="noreferrer"
                                    className="font-mono break-all leading-relaxed text-primary hover:underline"
                                    title={source.repo_url}
                                  >
                                    {source.repo_url}
                                  </a>
                                  <button
                                    type="button"
                                    onClick={() => handleCopySkillSourceRepo(source.repo_url)}
                                    className="mt-0.5 p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-background/80 shrink-0 opacity-0 group-hover/repo:opacity-100 transition-opacity"
                                    title={t('copy', 'Copy')}
                                  >
                                    <Copy className="w-3.5 h-3.5" />
                                  </button>
                                </div>
                              </div>
                            </div>

                            {Array.isArray(source.default_models) && source.default_models.length > 0 && (
                              <div className="mt-2 flex flex-wrap gap-1.5">
                                {source.default_models.map((m) => (
                                  <span key={`${source.id}-${m}`} className="px-2 py-0.5 rounded border text-[11px] bg-background text-muted-foreground">
                                    {m}
                                  </span>
                                ))}
                              </div>
                            )}

                            <div className={`mt-2 text-xs ${syncToneClass}`}>
                              {syncMessage}
                            </div>

                            <div className="mt-3 flex items-center justify-end gap-2 shrink-0 border-t pt-2.5">
                              <button
                                type="button"
                                onClick={() => removeSkillSource(source.id)}
                                className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                              >
                                <Trash2 className="w-3.5 h-3.5" />
                                {t('delete', 'Delete')}
                              </button>
                            </div>
                          </div>
                        )})}
                      </div>
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'subagents' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex flex-col gap-1">
                      <h2 className="text-lg font-semibold">{t('subagentsSourcesMenu', 'Subagents 源')}</h2>
                      <p className="text-sm text-muted-foreground">{t('subagentsSyncEnabledDesc', 'Global switch for scheduled Git repository subagents sync.')}</p>
                      <div className="flex items-center gap-2 pt-1 text-xs text-muted-foreground">
                        <span className="px-2 py-0.5 rounded-full border bg-muted/40">
                          {t('lastSyncAt', 'Last Sync')}: {lastSubagentsSyncText}
                        </span>
                        <span className="px-2 py-0.5 rounded-full border bg-muted/40">
                          {t('sources', 'Sources')}: {enabledSubagentsSources}/{subagentsSources.length}
                        </span>
                      </div>
                    </div>
                    <button
                      onClick={handleSubagentsSyncNow}
                      disabled={loading || subagentsSyncNowLoading}
                      className="px-3 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 disabled:opacity-50 inline-flex items-center gap-2"
                    >
                      {subagentsSyncNowLoading && <RefreshCw className="w-4 h-4 animate-spin" />}
                      {t('syncNow', 'Sync Now')}
                    </button>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('subagentsSyncEnabled', 'Enable Subagents Auto Sync')}</h3>
                        <p className="text-xs text-muted-foreground">{t('subagentsSyncEnabledDesc', 'Global switch for scheduled Git repository subagents sync.')}</p>
                      </div>
                      <label className="relative inline-flex items-center cursor-pointer">
                        <input
                          type="checkbox"
                          className="sr-only peer"
                          checked={!!config.subagents_sync_enabled}
                          onChange={(e) => setConfig((prev) => ({ ...prev, subagents_sync_enabled: e.target.checked }))}
                        />
                        <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                      </label>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('subagentsSyncInterval', 'Subagents Sync Interval (minutes)')}</label>
                      <input
                        type="number"
                        min={5}
                        max={1440}
                        step={5}
                        disabled={!config.subagents_sync_enabled}
                        className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 disabled:opacity-60"
                        value={config.subagents_sync_interval_minutes ?? 60}
                        onChange={(e) => {
                          const raw = parseInt(e.target.value, 10);
                          const value = Number.isFinite(raw) ? Math.max(5, Math.min(1440, raw)) : 60;
                          setConfig((prev) => ({ ...prev, subagents_sync_interval_minutes: value }));
                        }}
                      />
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">
                        {t('subagentsNewBadgeHours', 'New Subagent Badge Duration (hours)')}
                      </label>
                      <input
                        type="number"
                        min={1}
                        max={720}
                        step={1}
                        className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                        value={config.subagents_new_badge_hours ?? 72}
                        onChange={(e) => {
                          const raw = parseInt(e.target.value, 10);
                          const value = Number.isFinite(raw) ? Math.max(1, Math.min(720, raw)) : 72;
                          setConfig((prev) => ({ ...prev, subagents_new_badge_hours: value }));
                        }}
                      />
                    </div>

                    <hr className="border-border/50" />

                    <div className="space-y-2">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <h4 className="text-sm font-medium text-muted-foreground">{t('subagentsSources', 'Git Repository Subagents Sources')}</h4>
                        <div className="flex items-center gap-2">
                          <input
                            ref={subagentsImportInputRef}
                            type="file"
                            accept="application/json,.json"
                            onChange={handleImportSubagentSources}
                            className="hidden"
                          />
                          <button
                            type="button"
                            onClick={() => subagentsImportInputRef.current?.click()}
                            className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border bg-background hover:bg-muted text-xs"
                          >
                            <Upload className="w-3.5 h-3.5" />
                            {t('import', 'Import')}
                          </button>
                          <button
                            type="button"
                            onClick={handleExportSubagentSources}
                            className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md border bg-background hover:bg-muted text-xs"
                          >
                            <Download className="w-3.5 h-3.5" />
                            {t('export', 'Export')}
                          </button>
                          <button
                            type="button"
                            onClick={() => {
                              resetNewSubagentSourceForm();
                              setShowAddSubagentSourceModal(true);
                            }}
                            className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm flex items-center gap-2 hover:bg-primary/90"
                          >
                            <Plus className="w-4 h-4" />
                            {t('addSource', 'Add Source')}
                          </button>
                        </div>
                      </div>

                      <div className="space-y-2">
                        {subagentsSources.length === 0 && (
                          <div className="rounded-md border border-dashed p-4 text-xs text-muted-foreground bg-muted/10">
                            {t('noSubagentsSources', 'No Git repository source configured yet. Add one above to enable catalog sync.')}
                          </div>
                        )}
                        {subagentsSources.map((source, idx) => {
                          const syncInfo = subagentsSyncSourceMap.get(source.id);
                          const diagnoseInfo = source.id ? subagentSourceDiagnostics[source.id] : null;
                          const diagnosing = !!(source.id && subagentSourceDiagnosing[source.id]);
                          const syncFailed = !!syncInfo?.last_error || !!syncInfo?.last_status?.includes('error');
                          const syncSucceeded = !syncFailed && !!syncInfo?.last_synced_at;
                          const syncToneClass = syncFailed
                            ? 'text-destructive'
                            : syncSucceeded
                              ? 'text-emerald-600 dark:text-emerald-400'
                              : 'text-muted-foreground';
                          const syncMessage = syncFailed
                            ? t('subagentsSourceSyncFailed', 'Sync failed: {{message}}', {
                                message: syncInfo?.last_error || syncInfo?.last_status || t('unknownError', 'Unknown error'),
                              })
                            : syncSucceeded
                              ? t('subagentsSourceSyncSuccessAt', 'Sync successful: {{time}}', {
                                  time: formatSyncTs(syncInfo.last_synced_at),
                                })
                              : t('subagentsSourceSyncNever', 'Not synced yet');
                          return (
                            <div key={source.id || `${idx}`} className="group relative flex flex-col justify-between p-4 rounded-xl border bg-card text-card-foreground shadow-sm hover:shadow-md transition-all hover:border-primary/50 overflow-hidden">
                              <div className={`absolute top-0 left-0 w-1 h-full transition-colors ${source.enabled ? 'bg-primary/0 group-hover:bg-primary' : 'bg-muted group-hover:bg-muted-foreground/40'}`}></div>
                              <div className="flex items-start justify-between gap-3">
                                <div className="space-y-1 min-w-0">
                                  <div className="text-sm font-semibold truncate">{source.name || source.id || t('untitledSource', 'Untitled Source')}</div>
                                  <div className="text-xs text-muted-foreground">
                                    {t('sourceId', 'Source ID')}: <span className="font-mono">{source.id || '-'}</span>
                                  </div>
                                </div>
                                <label className="inline-flex items-center gap-1.5 text-xs shrink-0 cursor-pointer">
                                  <input
                                    type="checkbox"
                                    className="sr-only peer"
                                    checked={!!source.enabled}
                                    onChange={(e) => updateSubagentSource(source.id, { enabled: e.target.checked })}
                                  />
                                  <div className="w-10 h-5 bg-gray-200 rounded-full relative transition-colors peer-checked:bg-primary dark:bg-gray-700 peer-focus:ring-2 peer-focus:ring-primary/20 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:w-4 after:h-4 after:bg-white after:border after:rounded-full after:transition-all peer-checked:after:translate-x-5"></div>
                                  <span>{t('enabled', 'Enabled')}</span>
                                </label>
                              </div>

                              <div className="mt-2 rounded-lg border bg-muted/20 p-2.5 space-y-1.5">
                                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                  <span className="inline-block w-16 uppercase tracking-wider opacity-70">{t('branch', 'Branch')}</span>
                                  <span className="font-mono text-foreground/80">{source.branch || 'main'}</span>
                                </div>
                                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                  <span className="inline-block w-16 uppercase tracking-wider opacity-70">{t('baseDir', 'Base Directory')}</span>
                                  <span className="font-mono text-foreground/80">{source.base_dir || '/'}</span>
                                </div>
                                <div className="flex items-start gap-2 text-xs text-muted-foreground group/repo">
                                  <span className="inline-block w-16 uppercase tracking-wider opacity-70 pt-0.5">{t('repoUrl', 'Repo URL')}</span>
                                  <div className="min-w-0 flex-1 flex items-start gap-1.5">
                                    <a
                                      href={source.repo_url}
                                      target="_blank"
                                      rel="noreferrer"
                                      className="font-mono break-all leading-relaxed text-primary hover:underline"
                                      title={source.repo_url}
                                    >
                                      {source.repo_url}
                                    </a>
                                    <button
                                      type="button"
                                      onClick={() => handleCopySkillSourceRepo(source.repo_url)}
                                      className="mt-0.5 p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-background/80 shrink-0 opacity-0 group-hover/repo:opacity-100 transition-opacity"
                                      title={t('copy', 'Copy')}
                                    >
                                      <Copy className="w-3.5 h-3.5" />
                                    </button>
                                  </div>
                                </div>
                              </div>

                              <div className={`mt-2 text-xs ${syncToneClass}`}>
                                {syncMessage}
                              </div>

                              {diagnoseInfo && (
                                <div className="mt-2 rounded-lg border bg-muted/20 p-2.5 text-xs space-y-1.5">
                                  <div className="font-medium text-foreground">
                                    {t(
                                      'subagentsDiagnoseSummary',
                                      'Diagnosis done: scanned {{total}}, accepted {{accepted}}, skipped {{skipped}}',
                                      {
                                        total: diagnoseInfo.total_entries || 0,
                                        accepted: diagnoseInfo.accepted_entries || 0,
                                        skipped: diagnoseInfo.skipped_entries || 0,
                                      },
                                    )}
                                  </div>
                                  <div className="text-muted-foreground">
                                    {t('subagentsDiagnoseScanRoot', 'Scan root')}: <span className="font-mono">{diagnoseInfo.scan_root || '-'}</span>
                                  </div>
                                  {!!diagnoseInfo.last_commit_sha && (
                                    <div className="text-muted-foreground">
                                      {t('subagentsDiagnoseCommit', 'Commit')}: <span className="font-mono">{diagnoseInfo.last_commit_sha}</span>
                                    </div>
                                  )}
                                  <div className="text-muted-foreground">
                                    {t('subagentsDiagnoseSkipBreakdown', 'Skipped breakdown')}:&nbsp;
                                    {t('subagentsDiagnoseReason_missing_frontmatter', 'Missing frontmatter block')} {diagnoseInfo.skipped_missing_frontmatter || 0},
                                    {' '}
                                    {t('subagentsDiagnoseReason_missing_name', 'Missing frontmatter name')} {diagnoseInfo.skipped_missing_name || 0},
                                    {' '}
                                    {t('subagentsDiagnoseReason_invalid_name', 'Invalid frontmatter name')} {diagnoseInfo.skipped_invalid_name || 0},
                                    {' '}
                                    {t('subagentsDiagnoseReason_read_error', 'Failed to read markdown')} {diagnoseInfo.skipped_read_error || 0}
                                  </div>
                                  {(diagnoseInfo.skipped_samples || []).length > 0 && (
                                    <div className="space-y-1">
                                      {(diagnoseInfo.skipped_samples || []).slice(0, 6).map((item, itemIdx) => (
                                        <div key={`${source.id}-diag-${itemIdx}`} className="text-muted-foreground">
                                          <span className="font-mono">{item.rel_path}</span>
                                          {' - '}
                                          {getSubagentDiagnoseReasonLabel(item.reason)}
                                        </div>
                                      ))}
                                    </div>
                                  )}
                                </div>
                              )}

                              <div className="mt-3 flex items-center justify-end gap-2 shrink-0 border-t pt-2.5">
                                <button
                                  type="button"
                                  onClick={() => handleDiagnoseSubagentSource(source.id)}
                                  disabled={diagnosing || !source.id}
                                  className="text-muted-foreground hover:text-foreground hover:bg-muted px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors disabled:opacity-50"
                                >
                                  {diagnosing ? <RefreshCw className="w-3.5 h-3.5 animate-spin" /> : <AlertCircle className="w-3.5 h-3.5" />}
                                  {t('subagentsDiagnoseAction', 'Diagnose')}
                                </button>
                                <button
                                  type="button"
                                  onClick={() => removeSubagentSource(source.id)}
                                  className="text-muted-foreground hover:text-destructive hover:bg-destructive/10 px-3 py-1.5 rounded-md flex items-center gap-2 text-sm font-medium transition-colors"
                                >
                                  <Trash2 className="w-3.5 h-3.5" />
                                  {t('delete', 'Delete')}
                                </button>
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'shortcuts' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('shortcuts', 'Global Shortcuts')}</h2>
                    <p className="text-sm text-muted-foreground">{t('shortcutsDesc', 'Hotkeys to trigger OneSpace from anywhere.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="space-y-3">
                      <label className="text-sm font-medium text-muted-foreground">{t('toggleMainWindow', 'Toggle Main Window')}</label>
                      <div className="flex items-center gap-4">
                        <div className={`flex-1 flex items-center bg-muted/30 border rounded-xl px-4 py-4 text-sm transition-all h-14 ${recordingField === 'main' ? 'ring-2 ring-primary border-primary bg-primary/5' : ''}`}>
                          {recordingField === 'main' ? (
                            <span className="flex items-center gap-3 text-primary font-bold animate-pulse">
                              <CircleDot className="w-4 h-4" />
                              {t('recordingPlaceholder', 'Press keys...')}
                            </span>
                          ) : (
                            <div className="flex gap-1.5">
                              {config.main_shortcut?.split('+').map((key, i) => (
                                <kbd key={i} className="px-2.5 py-1 bg-background border-b-2 border-x border-t rounded-md font-mono text-sm shadow-sm">
                                  {key === 'Control' ? <Command className="w-3 h-3 inline mr-1" /> : null}
                                  {key}
                                </kbd>
                              ))}
                            </div>
                          )}
                        </div>
                        <button 
                          onClick={() => setRecordingField(recordingField === 'main' ? null : 'main')}
                          className={`px-6 h-14 rounded-xl text-sm font-semibold transition-all active:scale-95 ${
                            recordingField === 'main' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90 shadow-lg shadow-destructive/20' : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                          }`}
                        >
                          {recordingField === 'main' ? t('stopRecording', 'Stop') : t('record', 'Record')}
                        </button>
                      </div>
                    </div>

                    <div className="space-y-3">
                      <label className="text-sm font-medium text-muted-foreground">{t('toggleQuickAi', 'Quick AI Session Bar')}</label>
                      <div className="flex items-center gap-4">
                        <div className={`flex-1 flex items-center bg-muted/30 border rounded-xl px-4 py-4 text-sm transition-all h-14 ${recordingField === 'quick' ? 'ring-2 ring-primary border-primary bg-primary/5' : ''}`}>
                          {recordingField === 'quick' ? (
                            <span className="flex items-center gap-3 text-primary font-bold animate-pulse">
                              <CircleDot className="w-4 h-4" />
                              {t('recordingPlaceholder', 'Press keys...')}
                            </span>
                          ) : (
                            <div className="flex gap-1.5">
                              {config.quick_ai_shortcut?.split('+').map((key, i) => (
                                <kbd key={i} className="px-2.5 py-1 bg-background border-b-2 border-x border-t rounded-md font-mono text-sm shadow-sm">
                                  {key === 'Control' ? <Command className="w-3 h-3 inline mr-1" /> : null}
                                  {key}
                                </kbd>
                              ))}
                            </div>
                          )}
                        </div>
                        <button 
                          onClick={() => setRecordingField(recordingField === 'quick' ? null : 'quick')}
                          className={`px-6 h-14 rounded-xl text-sm font-semibold transition-all active:scale-95 ${
                            recordingField === 'quick' ? 'bg-destructive text-destructive-foreground hover:bg-destructive/90 shadow-lg shadow-destructive/20' : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                          }`}
                        >
                          {recordingField === 'quick' ? t('stopRecording', 'Stop') : t('record', 'Record')}
                        </button>
                      </div>
                    </div>

                    <div className="p-4 bg-primary/5 rounded-xl border border-primary/10 flex gap-3">
                      <Zap className="w-5 h-5 text-primary shrink-0 mt-0.5" />
                      <p className="text-xs text-primary/80 leading-relaxed italic">
                        {t('shortcutsNote', 'Tip: You can use combinations like Command+Shift+K or Alt+Space.')}
                      </p>
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'ai' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('aiSessions', 'AI Terminal Sessions')}</h2>
                    <p className="text-sm text-muted-foreground">{t('aiSessionsDesc', 'Default configuration for quick AI terminal sessions.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-4">
                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('defaultAiModel', 'Default Model')}</label>
                      <div className="grid grid-cols-2 gap-2">
                        {skillModelOptions.map(({ id, label, Icon }) => {
                          const active = (config.default_ai_model || 'claude') === id;
                          return (
                            <button
                              key={id}
                              type="button"
                              onClick={() => setConfig({...config, default_ai_model: id as StorageConfig['default_ai_model']})}
                              className={`flex items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-all ${
                                active
                                  ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                                  : 'bg-background hover:bg-muted/50 text-foreground border-border'
                              }`}
                            >
                              <Icon className="w-4 h-4 shrink-0" />
                              <span className="truncate">{label}</span>
                            </button>
                          );
                        })}
                      </div>
                      <p className="text-xs text-muted-foreground">{t('defaultAiModelDesc', 'Preselected model for the Quick AI Session Bar.')}</p>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">
                        {t('aiModelLaunchCommands', 'Model Launch Commands')}
                      </label>
                      <div className="space-y-2">
                        {skillModelOptions.map(({ id, label, Icon }) => {
                          if (!['claude', 'gemini', 'codex', 'opencode'].includes(id)) return null;
                          const commands = normalizeAiModelLaunchCommandsForUi(config.ai_model_launch_commands);
                          const modelId = id as AiModelId;
                          return (
                            <div key={`ai-launch-command-${id}`} className="grid grid-cols-[180px_1fr] gap-2 items-center">
                              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                                <Icon className="w-4 h-4 shrink-0" />
                                <span className="truncate">{label}</span>
                              </div>
                              <input
                                type="text"
                                value={commands[modelId]}
                                onChange={(e) =>
                                  setConfig((prev) => ({
                                    ...prev,
                                    ai_model_launch_commands: {
                                      ...normalizeAiModelLaunchCommandsForUi(prev.ai_model_launch_commands),
                                      [modelId]: e.target.value,
                                    },
                                  }))
                                }
                                className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm font-mono"
                                placeholder={DEFAULT_AI_MODEL_LAUNCH_COMMANDS[modelId]}
                              />
                            </div>
                          );
                        })}
                      </div>
                      <p className="text-xs text-muted-foreground">
                        {t(
                          'aiModelLaunchCommandsDesc',
                          'Used when creating a new AI terminal session. Supports {session_id} placeholder.',
                        )}
                      </p>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('aiTerminalApp', 'Terminal Application')}</label>
                      <div className="flex gap-2">
                        <input
                          type="text"
                          readOnly
                          value={config.ai_terminal_app || t('aiTerminalAppPlaceholder', '终端')}
                          className="flex-1 bg-muted/50 border rounded-xl px-4 py-2.5 text-sm text-muted-foreground cursor-default"
                        />
                        <button
                          onClick={handleSelectTerminalApp}
                          className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                        >
                          <FolderOpen className="w-4 h-4" />
                        </button>
                      </div>
                      <p className="text-xs text-muted-foreground">{t('aiTerminalAppDesc', 'Choose an app from Applications. OneSpace will use it to launch AI terminal sessions.')}</p>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('defaultAiPath', 'Default Project Directory')}</label>
                      <div className="flex gap-2">
                        <input 
                          type="text" 
                          readOnly
                          className="flex-1 bg-muted/50 border rounded-xl px-4 py-2.5 text-sm text-muted-foreground font-mono truncate cursor-default"
                          value={config.default_ai_dir || t('notSet', 'Not Set')}
                        />
                        <button 
                          onClick={handleSelectDefaultDir}
                          className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                        >
                          <FolderOpen className="w-4 h-4" />
                        </button>
                      </div>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('aiSessionsHistoryDays', 'History Sync Duration (Days)')}</label>
                      <div className="flex items-center gap-3">
                        <input
                          type="number"
                          min="1"
                          max="365"
                          value={config.ai_sessions_history_days ?? 30}
                          onChange={(e) => {
                            const value = Math.min(365, Math.max(1, parseInt(e.target.value) || 30));
                            setConfig({ ...config, ai_sessions_history_days: value });
                          }}
                          className="w-32 bg-background border rounded-xl px-4 py-2.5 text-sm font-mono"
                        />
                        <span className="text-sm text-muted-foreground">{t('aiSessionsHistoryDaysDesc', 'Only sync sessions from the past N days')}</span>
                      </div>
                      <p className="text-xs text-muted-foreground">{t('aiSessionsHistoryDaysNote', 'Sessions older than this will be hidden from the list')}</p>
                    </div>
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'appearance' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('appearance', 'Appearance & Locale')}</h2>
                    <p className="text-sm text-muted-foreground">{t('appearanceDesc', 'Customize how OneSpace looks and feels.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-8">
                    {/* Theme */}
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('theme', 'App Theme')}</h3>
                        <p className="text-xs text-muted-foreground">{t('themeDesc', 'Select your preferred visual theme.')}</p>
                      </div>
                      <button 
                        onClick={cycleTheme}
                        className="flex items-center gap-2 px-4 py-2 bg-muted hover:bg-muted/80 rounded-xl transition-all"
                      >
                        <ThemeIcon className="w-4 h-4" />
                        <span className="text-sm capitalize">{theme}</span>
                      </button>
                    </div>

                    <hr className="border-border/50" />

                    {/* Language */}
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('language', 'Language')}</h3>
                        <p className="text-xs text-muted-foreground">{t('languageDesc', 'Choose the language for the user interface.')}</p>
                      </div>
                      <button 
                        onClick={toggleLanguage}
                        className="px-4 py-2 bg-muted hover:bg-muted/80 rounded-xl transition-all text-sm font-medium"
                      >
                        {i18n.language === 'zh' ? '简体中文' : 'English'}
                      </button>
                    </div>
                  </div>
                </section>
              </div>
            )}

                        {activeTab === 'proxy' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('proxySettings', 'Network Proxy Settings')}</h2>
                    <p className="text-sm text-muted-foreground">{t('proxySettingsDesc', 'Configure proxy for backend network requests')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    {/* Enable Proxy Switch */}
                    <div className="flex items-center justify-between">
                      <div className="space-y-0.5">
                        <h3 className="text-sm font-medium">{t('proxyEnabled', 'Enable Proxy')}</h3>
                        <p className="text-xs text-muted-foreground">{t('proxyEnabledDesc', 'All backend requests will use the proxy')}</p>
                      </div>
                      <label className="relative inline-flex items-center cursor-pointer">
                        <input
                          type="checkbox"
                          className="sr-only peer"
                          checked={proxyConfig.proxy_enabled}
                          onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_enabled: e.target.checked })}
                        />
                        <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                      </label>
                    </div>

                    {/* Conditional Content - Only show when proxy is enabled */}
                    {proxyConfig.proxy_enabled && (
                      <>
                        <hr className="border-border/50 animate-in fade-in" />

                        {/* Proxy Type - Tab Style */}
                        <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                          <label className="text-sm font-medium">{t('proxyType', 'Proxy Type')}</label>
                          <div className="grid grid-cols-3 gap-2 p-1 bg-muted rounded-xl border">
                            {(['http', 'https', 'socks5'] as const).map((type) => (
                              <button
                                key={type}
                                onClick={() => setProxyConfig({ ...proxyConfig, proxy_type: type })}
                                className={`py-2.5 px-4 rounded-lg text-sm font-medium transition-all ${
                                  proxyConfig.proxy_type === type
                                    ? 'bg-background shadow-sm text-foreground'
                                    : 'text-muted-foreground hover:text-foreground'
                                }`}
                              >
                                {type.toUpperCase()}
                              </button>
                            ))}
                          </div>
                        </div>

                        {/* Host and Port */}
                        <div className="grid grid-cols-3 gap-4 animate-in fade-in slide-in-from-top-2">
                          <div className="col-span-2 space-y-2">
                            <label className="text-sm font-medium">{t('proxyHost', 'Proxy Host')}</label>
                            <input
                              type="text"
                              placeholder="127.0.0.1"
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={proxyConfig.proxy_host}
                              onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_host: e.target.value })}
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-medium">{t('proxyPort', 'Port')}</label>
                            <input
                              type="number"
                              placeholder="1080"
                              className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={proxyConfig.proxy_port}
                              onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_port: parseInt(e.target.value) || 0 })}
                            />
                          </div>
                        </div>

                        {/* Authentication Switch */}
                        <div className="space-y-3 animate-in fade-in slide-in-from-top-2">
                          <div className="flex items-center justify-between">
                            <div className="space-y-0.5">
                              <h3 className="text-sm font-medium">{t('proxyAuth', 'Authentication')}</h3>
                              <p className="text-xs text-muted-foreground">{t('proxyAuthDesc', 'Enable if your proxy requires credentials')}</p>
                            </div>
                            <label className="relative inline-flex items-center cursor-pointer">
                              <input
                                type="checkbox"
                                className="sr-only peer"
                                checked={authEnabled}
                                onChange={(e) => {
                                  setAuthEnabled(e.target.checked);
                                  if (!e.target.checked) {
                                    setProxyConfig({ ...proxyConfig, proxy_username: '', proxy_password: '' });
                                  }
                                }}
                              />
                              <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                            </label>
                          </div>

                          {authEnabled && (
                            <div className="grid grid-cols-2 gap-4 animate-in fade-in slide-in-from-top-2">
                              <div className="space-y-2">
                                <label className="text-sm font-medium">{t('proxyUsername', 'Username')}</label>
                                <input
                                  type="text"
                                  className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={proxyConfig.proxy_username}
                                  onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_username: e.target.value })}
                                />
                              </div>
                              <div className="space-y-2">
                                <label className="text-sm font-medium">{t('proxyPassword', 'Password')}</label>
                                <input
                                  type="password"
                                  className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                                  value={proxyConfig.proxy_password}
                                  onChange={(e) => setProxyConfig({ ...proxyConfig, proxy_password: e.target.value })}
                                />
                              </div>
                            </div>
                          )}
                        </div>

                        {/* Check Interval - Quick Select */}
                        <div className="space-y-2 animate-in fade-in slide-in-from-top-2">
                          <label className="text-sm font-medium">{t('checkInterval', 'Check Interval')}</label>
                          <div className="grid grid-cols-4 gap-2">
                            {[
                              { value: 5, label: t('interval5min', '5 min') },
                              { value: 15, label: t('interval15min', '15 min') },
                              { value: 30, label: t('interval30min', '30 min') },
                              { value: 60, label: t('interval1h', '1 hour') },
                            ].map((item) => (
                              <button
                                key={item.value}
                                onClick={() => setProxyConfig({ ...proxyConfig, check_interval: item.value })}
                                className={`py-2.5 px-3 rounded-lg text-sm font-medium transition-all ${
                                  proxyConfig.check_interval === item.value
                                    ? 'bg-primary text-primary-foreground shadow-sm'
                                    : 'bg-muted text-muted-foreground hover:bg-muted/80'
                                }`}
                              >
                                {item.label}
                              </button>
                            ))}
                          </div>
                        </div>

                        {/* Test Button */}
                        <div className="flex items-center gap-4 pt-4 border-t animate-in fade-in slide-in-from-top-2">
                          <button
                            onClick={async () => {
                              setTestingProxy(true);
                              try {
                                // Test with current form config (even if not saved yet)
                                const status = await invoke<ProxyStatus>('test_proxy_connection', {
                                  config: proxyConfig
                                });
                                setProxyStatus(status);
                              } catch (e: any) {
                                setProxyStatus({
                                  is_available: false,
                                  latency_ms: 0,
                                  message: e.toString(),
                                  proxy_type: proxyConfig.proxy_type,
                                  proxy_host: proxyConfig.proxy_host,
                                });
                              } finally {
                                setTestingProxy(false);
                              }
                            }}
                            disabled={testingProxy || !proxyConfig.proxy_host}
                            className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-xl text-sm font-medium hover:bg-primary/90 disabled:opacity-50"
                          >
                            {testingProxy ? (
                              <RefreshCw className="w-4 h-4 animate-spin" />
                            ) : (
                              <PlugZap className="w-4 h-4" />
                            )}
                            {t('testProxy', 'Test Connection')}
                          </button>

                          {proxyStatus && (
                            <div className={`flex items-center gap-2 text-sm ${
                              proxyStatus.is_available ? 'text-green-600' : 'text-red-600'
                            }`}>
                              {proxyStatus.is_available ? (
                                <CheckCircle2 className="w-4 h-4" />
                              ) : (
                                <AlertCircle className="w-4 h-4" />
                              )}
                              {proxyStatus.message} {proxyStatus.latency_ms > 0 && `(${proxyStatus.latency_ms}ms)`}
                            </div>
                          )}
                        </div>
                      </>
                    )}
                  </div>
                </section>
              </div>
            )}

            {activeTab === 'security' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('security', 'Data Security')}</h2>
                    <p className="text-sm text-muted-foreground">{t('securityDesc', 'Manage your master password used for encrypting sensitive data.')}</p>
                  </div>

                  <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
                    <div className="bg-muted/30 p-5 rounded-2xl border border-dashed space-y-4">
                      <div className="flex items-center justify-between">
                        <label className="text-sm font-medium text-muted-foreground">{t('currentMasterPassword', 'Current Master Password')}</label>
                        <ShieldCheck className="w-5 h-5 text-primary opacity-50" />
                      </div>
                      
                      <div className="relative">
                        <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                        <input 
                          type={showPass ? 'text' : 'password'}
                          readOnly
                          className="w-full bg-background border rounded-xl pl-10 pr-20 py-3 text-sm font-mono tracking-widest"
                          value={masterPassword}
                        />
                        <button
                          onClick={async () => {
                            try {
                              await navigator.clipboard.writeText(masterPassword);
                              setMessage({ type: 'success', text: t('copiedToClipboard', 'Copied to clipboard') });
                              setTimeout(() => setMessage({ type: '', text: '' }), 2000);
                            } catch (e: any) {
                              setMessage({ type: 'error', text: e.toString() });
                            }
                          }}
                          title={t('copyToClipboard', 'Copy to clipboard')}
                          className="absolute right-10 top-3 text-muted-foreground hover:text-foreground transition-colors"
                        >
                          <Copy className="w-4 h-4" />
                        </button>
                        <button 
                          onClick={() => setShowPass(!showPass)}
                          className="absolute right-3.5 top-3 text-muted-foreground hover:text-foreground transition-colors"
                        >
                          {showPass ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                        </button>
                      </div>
                      <p className="text-[10px] text-muted-foreground leading-relaxed">
                        {t('defaultPassNotice', 'Note: This key is used for encrypting Git credentials and other sensitive data locally.')}
                      </p>
                    </div>

                    {!changingPass ? (
                      <button 
                        onClick={() => {
                          setChangingPass(true);
                          setShowNewPass(true);
                          setShowConfirmNewPass(true);
                        }}
                        className="w-full py-3 border border-primary/20 bg-primary/5 text-primary rounded-xl text-sm font-semibold hover:bg-primary/10 transition-all"
                      >
                        {t('changeMasterPassword', 'Change Master Password')}
                      </button>
                    ) : (
                      <div className="space-y-4 pt-4 border-t animate-in fade-in slide-in-from-top-2">
                        <div className="flex justify-end">
                          <button
                            onClick={handleGenerateMd5Password}
                            disabled={loading}
                            className="inline-flex items-center gap-2 px-3 py-1.5 border rounded-lg text-xs font-medium hover:bg-muted transition-colors disabled:opacity-50"
                          >
                            <Sparkles className="w-3.5 h-3.5" />
                            {t('generateMd5Password', 'Generate MD5 Password')}
                          </button>
                        </div>
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('newPassword', 'New Password')}</label>
                          <div className="relative">
                            <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                            <input 
                              type={showNewPass ? 'text' : 'password'}
                              className="w-full bg-background border rounded-xl pl-10 pr-12 py-3 text-sm font-mono tracking-widest focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={newPass}
                              onChange={e => setNewPass(e.target.value)}
                            />
                            <button 
                              onClick={() => setShowNewPass(!showNewPass)}
                              className="absolute right-3.5 top-3 text-muted-foreground hover:text-foreground transition-colors"
                            >
                              {showNewPass ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                            </button>
                          </div>
                        </div>
                        <div className="space-y-2">
                          <label className="text-sm font-medium text-muted-foreground">{t('confirmPassword', 'Confirm Password')}</label>
                          <div className="relative">
                            <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                            <input 
                              type={showConfirmNewPass ? 'text' : 'password'}
                              className="w-full bg-background border rounded-xl pl-10 pr-12 py-3 text-sm font-mono tracking-widest focus:outline-none focus:ring-2 focus:ring-primary/20"
                              value={confirmNewPass}
                              onChange={e => setConfirmNewPass(e.target.value)}
                            />
                            <button 
                              onClick={() => setShowConfirmNewPass(!showConfirmNewPass)}
                              className="absolute right-3.5 top-3 text-muted-foreground hover:text-foreground transition-colors"
                            >
                              {showConfirmNewPass ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                            </button>
                          </div>
                        </div>
                        <div className="flex gap-2 pt-2">
                          <button 
                            onClick={handleChangeMasterPassword}
                            disabled={!newPass || !confirmNewPass || newPass !== confirmNewPass || loading || !masterPassword}
                            className="flex-1 bg-primary text-primary-foreground py-2.5 rounded-xl text-sm font-semibold hover:bg-primary/90 disabled:opacity-50"
                          >
                            {loading ? <RefreshCw className="w-4 h-4 animate-spin mx-auto" /> : t('confirmChange', 'Update Password')}
                          </button>
                          <button 
                            onClick={() => {
                              setChangingPass(false);
                              setNewPass('');
                              setConfirmNewPass('');
                              setShowNewPass(true);
                              setShowConfirmNewPass(true);
                            }}
                            className="px-6 py-2.5 border rounded-xl text-sm font-medium hover:bg-muted transition-all"
                          >
                            {t('cancel', 'Cancel')}
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                </section>
              </div>
            )}

            {showAddSkillSourceModal && (
              <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
                <div className="bg-background rounded-lg w-full max-w-2xl max-h-[90vh] overflow-y-auto">
                  <div className="p-6 border-b flex justify-between items-center sticky top-0 bg-background z-10">
                    <h3 className="text-xl font-bold">{t('addSource', 'Add Source')}</h3>
                    <button
                      type="button"
                      onClick={() => {
                        setShowAddSkillSourceModal(false);
                        setNewSourceValidation({});
                      }}
                      className="p-2 hover:bg-secondary rounded"
                    >
                      <X className="w-5 h-5" />
                    </button>
                  </div>

                  <form
                    className="p-6 space-y-4"
                    onSubmit={(e) => {
                      e.preventDefault();
                      const ok = addSkillSource();
                      if (ok) setShowAddSkillSourceModal(false);
                    }}
                  >
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('sourceId', 'Source ID')} *</label>
                        <input
                          type="text"
                          className={`w-full bg-background border rounded-md px-3 py-2 text-sm ${newSourceValidation.id ? 'border-destructive ring-1 ring-destructive/40' : ''}`}
                          value={newSkillSource.id}
                          onChange={(e) => setNewSkillSource((prev) => ({ ...prev, id: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('sourceName', 'Source Name')}</label>
                        <input
                          type="text"
                          className="w-full bg-background border rounded-md px-3 py-2 text-sm"
                          value={newSkillSource.name}
                          onChange={(e) => setNewSkillSource((prev) => ({ ...prev, name: e.target.value }))}
                        />
                      </div>
                    </div>

                    <div>
                      <label className="block text-sm font-medium mb-1">{t('repoUrl', 'Repo URL')} *</label>
                      <input
                        type="text"
                        placeholder="https://git.example.com/group/repo.git"
                        className={`w-full bg-background border rounded-md px-3 py-2 text-sm font-mono ${newSourceValidation.repo_url ? 'border-destructive ring-1 ring-destructive/40' : ''}`}
                        value={newSkillSource.repo_url}
                        onChange={(e) => setNewSkillSource((prev) => ({ ...prev, repo_url: e.target.value }))}
                      />
                    </div>

                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('branch', 'Branch')}</label>
                        <input
                          type="text"
                          className="w-full bg-background border rounded-md px-3 py-2 text-sm"
                          value={newSkillSource.branch || ''}
                          onChange={(e) => setNewSkillSource((prev) => ({ ...prev, branch: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('baseDir', 'Base Directory')}</label>
                        <input
                          type="text"
                          className={`w-full bg-background border rounded-md px-3 py-2 text-sm font-mono ${newSourceValidation.base_dir ? 'border-destructive ring-1 ring-destructive/40' : ''}`}
                          value={newSkillSource.base_dir || '/'}
                          onChange={(e) => setNewSkillSource((prev) => ({ ...prev, base_dir: e.target.value }))}
                        />
                      </div>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('sourceModels', 'Apply Models')}</label>
                      <div className="grid grid-cols-2 gap-2">
                        {skillModelOptions.map(({ id, label, Icon }) => {
                          const active = !!newSkillSource.default_models?.includes(id);
                          return (
                            <button
                              key={`new-source-model-${id}`}
                              type="button"
                              onClick={() => toggleNewSkillSourceModel(id)}
                              className={`flex items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-all ${
                                active
                                  ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                                  : 'bg-background hover:bg-muted/50 text-foreground border-border'
                              }`}
                            >
                              <Icon className="w-4 h-4 shrink-0" />
                              <span className="truncate">{label}</span>
                            </button>
                          );
                        })}
                      </div>
                    </div>

                    <label className="inline-flex items-center justify-between gap-3 text-sm rounded-md border p-3">
                      <span className="font-medium">{t('enabled', 'Enabled')}</span>
                      <input
                        type="checkbox"
                        className="sr-only peer"
                        checked={!!newSkillSource.enabled}
                        onChange={(e) => setNewSkillSource((prev) => ({ ...prev, enabled: e.target.checked }))}
                      />
                      <div className="w-10 h-5 bg-gray-200 rounded-full relative transition-colors peer-checked:bg-primary dark:bg-gray-700 peer-focus:ring-2 peer-focus:ring-primary/20 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:w-4 after:h-4 after:bg-white after:border after:rounded-full after:transition-all peer-checked:after:translate-x-5"></div>
                    </label>

                    {(newSourceValidation.id || newSourceValidation.repo_url || newSourceValidation.base_dir || newSourceValidation.default_models) && (
                      <div className="text-xs text-destructive space-y-0.5">
                        {newSourceValidation.id && <div>{newSourceValidation.id}</div>}
                        {newSourceValidation.repo_url && <div>{newSourceValidation.repo_url}</div>}
                        {newSourceValidation.base_dir && <div>{newSourceValidation.base_dir}</div>}
                        {newSourceValidation.default_models && <div>{newSourceValidation.default_models}</div>}
                      </div>
                    )}

                    <div className="flex justify-end gap-3 pt-4 border-t">
                      <button
                        type="button"
                        onClick={() => {
                          setShowAddSkillSourceModal(false);
                          setNewSourceValidation({});
                        }}
                        className="px-4 py-2 hover:bg-secondary rounded"
                      >
                        {t('cancel', 'Cancel')}
                      </button>
                      <button
                        type="submit"
                        className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:bg-primary/90"
                      >
                        {t('addSource', 'Add Source')}
                      </button>
                    </div>
                  </form>
                </div>
              </div>
            )}

            {showAddSubagentSourceModal && (
              <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
                <div className="bg-background rounded-lg w-full max-w-2xl max-h-[90vh] overflow-y-auto">
                  <div className="p-6 border-b flex justify-between items-center sticky top-0 bg-background z-10">
                    <h3 className="text-xl font-bold">{t('addSource', 'Add Source')}</h3>
                    <button
                      type="button"
                      onClick={() => {
                        setShowAddSubagentSourceModal(false);
                        setNewSubagentSourceValidation({});
                      }}
                      className="p-2 hover:bg-secondary rounded"
                    >
                      <X className="w-5 h-5" />
                    </button>
                  </div>

                  <form
                    className="p-6 space-y-4"
                    onSubmit={(e) => {
                      e.preventDefault();
                      const ok = addSubagentSource();
                      if (ok) setShowAddSubagentSourceModal(false);
                    }}
                  >
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('sourceId', 'Source ID')} *</label>
                        <input
                          type="text"
                          className={`w-full bg-background border rounded-md px-3 py-2 text-sm ${newSubagentSourceValidation.id ? 'border-destructive ring-1 ring-destructive/40' : ''}`}
                          value={newSubagentSource.id}
                          onChange={(e) => setNewSubagentSource((prev) => ({ ...prev, id: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('sourceName', 'Source Name')}</label>
                        <input
                          type="text"
                          className="w-full bg-background border rounded-md px-3 py-2 text-sm"
                          value={newSubagentSource.name}
                          onChange={(e) => setNewSubagentSource((prev) => ({ ...prev, name: e.target.value }))}
                        />
                      </div>
                    </div>

                    <div>
                      <label className="block text-sm font-medium mb-1">{t('repoUrl', 'Repo URL')} *</label>
                      <input
                        type="text"
                        placeholder="https://git.example.com/group/repo.git"
                        className={`w-full bg-background border rounded-md px-3 py-2 text-sm font-mono ${newSubagentSourceValidation.repo_url ? 'border-destructive ring-1 ring-destructive/40' : ''}`}
                        value={newSubagentSource.repo_url}
                        onChange={(e) => setNewSubagentSource((prev) => ({ ...prev, repo_url: e.target.value }))}
                      />
                    </div>

                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('branch', 'Branch')}</label>
                        <input
                          type="text"
                          className="w-full bg-background border rounded-md px-3 py-2 text-sm"
                          value={newSubagentSource.branch || ''}
                          onChange={(e) => setNewSubagentSource((prev) => ({ ...prev, branch: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-sm font-medium mb-1">{t('baseDir', 'Base Directory')}</label>
                        <input
                          type="text"
                          className={`w-full bg-background border rounded-md px-3 py-2 text-sm font-mono ${newSubagentSourceValidation.base_dir ? 'border-destructive ring-1 ring-destructive/40' : ''}`}
                          value={newSubagentSource.base_dir || '/'}
                          onChange={(e) => setNewSubagentSource((prev) => ({ ...prev, base_dir: e.target.value }))}
                        />
                      </div>
                    </div>

                    <div className="space-y-2">
                      <label className="text-sm font-medium text-muted-foreground">{t('sourceModels', 'Apply Models')}</label>
                      <div className="grid grid-cols-2 gap-2">
                        {skillModelOptions.map(({ id, label, Icon }) => {
                          const active = !!newSubagentSource.default_models?.includes(id);
                          return (
                            <button
                              key={`new-subagent-source-model-${id}`}
                              type="button"
                              onClick={() => toggleNewSubagentSourceModel(id)}
                              className={`flex items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-all ${
                                active
                                  ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                                  : 'bg-background hover:bg-muted/50 text-foreground border-border'
                              }`}
                            >
                              <Icon className="w-4 h-4 shrink-0" />
                              <span className="truncate">{label}</span>
                            </button>
                          );
                        })}
                      </div>
                    </div>

                    <label className="inline-flex items-center justify-between gap-3 text-sm rounded-md border p-3">
                      <span className="font-medium">{t('enabled', 'Enabled')}</span>
                      <input
                        type="checkbox"
                        className="sr-only peer"
                        checked={!!newSubagentSource.enabled}
                        onChange={(e) => setNewSubagentSource((prev) => ({ ...prev, enabled: e.target.checked }))}
                      />
                      <div className="w-10 h-5 bg-gray-200 rounded-full relative transition-colors peer-checked:bg-primary dark:bg-gray-700 peer-focus:ring-2 peer-focus:ring-primary/20 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:w-4 after:h-4 after:bg-white after:border after:rounded-full after:transition-all peer-checked:after:translate-x-5"></div>
                    </label>

                    {(newSubagentSourceValidation.id || newSubagentSourceValidation.repo_url || newSubagentSourceValidation.base_dir || newSubagentSourceValidation.default_models) && (
                      <div className="text-xs text-destructive space-y-0.5">
                        {newSubagentSourceValidation.id && <div>{newSubagentSourceValidation.id}</div>}
                        {newSubagentSourceValidation.repo_url && <div>{newSubagentSourceValidation.repo_url}</div>}
                        {newSubagentSourceValidation.base_dir && <div>{newSubagentSourceValidation.base_dir}</div>}
                        {newSubagentSourceValidation.default_models && <div>{newSubagentSourceValidation.default_models}</div>}
                      </div>
                    )}

                    <div className="flex justify-end gap-3 pt-4 border-t">
                      <button
                        type="button"
                        onClick={() => {
                          setShowAddSubagentSourceModal(false);
                          setNewSubagentSourceValidation({});
                        }}
                        className="px-4 py-2 hover:bg-secondary rounded"
                      >
                        {t('cancel', 'Cancel')}
                      </button>
                      <button
                        type="submit"
                        className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:bg-primary/90"
                      >
                        {t('addSource', 'Add Source')}
                      </button>
                    </div>
                  </form>
                </div>
              </div>
            )}
            
            {/* Bottom Spacing */}
            <div className="h-20" />
          </div>
        </div>
      </div>
      </div>
    </div>
  );
}
