import { useState, useEffect, useRef, type ChangeEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { 
  Save, 
  RefreshCw, 
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
  Copy,
  Download,
  Upload,
  Plus,
  Trash2,
  X
} from 'lucide-react';
import { open, save } from '@tauri-apps/plugin-dialog';
import { useTheme } from './ThemeProvider';
import { skillModelOptions } from './skillsModelOptions';

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
  language?: string;
  local_storage_path?: string;
  icloud_storage_path?: string;
  proxy?: ProxyConfig;
  auto_update_enabled?: boolean;
  update_check_interval_minutes?: number;
  update_last_checked_at?: number;
  skills_sync_enabled?: boolean;
  skills_sync_interval_minutes?: number;
  skills_last_synced_at?: number;
  skills_sources?: SkillSourceConfig[];
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

export function SettingsView({ initialTab = 'storage', onBack }: { initialTab?: string, onBack: () => void }) {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const [activeTab, setActiveTab] = useState(initialTab);
  const [config, setConfig] = useState<StorageConfig>({ storage_type: 'local' });
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState({ type: '', text: '' });
  
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
  const [proxyConfig, setProxyConfig] = useState<ProxyConfig>({
    proxy_enabled: false,
    proxy_type: 'socks5',
    proxy_host: '',
    proxy_port: 1080,
    proxy_username: '',
    proxy_password: '',
    check_interval: 15,
  });
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

  useEffect(() => {
    loadConfig();
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
      setConfig({
        ...cfg,
        storage_type: cfg.storage_type || 'local',
        auth_method: cfg.auth_method || 'http',
        main_shortcut: cfg.main_shortcut || 'Alt+Space',
        quick_ai_shortcut: cfg.quick_ai_shortcut || 'Alt+Shift+A',
        default_ai_model: cfg.default_ai_model || 'claude',
        ai_terminal_app: cfg.ai_terminal_app || t('aiTerminalAppPlaceholder', '终端'),
        auto_update_enabled: cfg.auto_update_enabled ?? false,
        update_check_interval_minutes: cfg.update_check_interval_minutes ?? 360,
        skills_sync_enabled: cfg.skills_sync_enabled ?? true,
        skills_sync_interval_minutes: cfg.skills_sync_interval_minutes ?? 60,
        skills_sources: cfg.skills_sources || [],
      });
      
      if (cfg.proxy) {
        setProxyConfig({
          proxy_enabled: cfg.proxy.proxy_enabled,
          proxy_type: cfg.proxy.proxy_type || 'socks5',
          proxy_host: cfg.proxy.proxy_host || '',
          proxy_port: cfg.proxy.proxy_port || 1080,
          proxy_username: cfg.proxy.proxy_username || '',
          proxy_password: cfg.proxy.proxy_password || '',
          check_interval: cfg.proxy.check_interval || 15,
        });
        // Enable auth switch if username or password is set
        setAuthEnabled(!!(cfg.proxy.proxy_username || cfg.proxy.proxy_password));
      }
      await loadSkillsSyncState();
    } catch (e) {
      console.error(e);
    }
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

  const removeSkillSource = (id: string) => {
    setConfig(prev => ({ ...prev, skills_sources: (prev.skills_sources || []).filter(s => s.id !== id) }));
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

  const hasSkillSourcesChanged = (before: SkillSourceConfig[] = [], after: SkillSourceConfig[] = []) =>
    JSON.stringify(normalizeSkillSourcesForSyncCompare(before)) !==
    JSON.stringify(normalizeSkillSourcesForSyncCompare(after));

  const saveConfig = async () => {
    setLoading(true);
    setMessage({ type: '', text: '' });
    try {
      const fullConfig = { ...config, proxy: proxyConfig };
      const currentConfig = await invoke<StorageConfig>('get_storage_config');
      const needSyncSkillsCatalog = hasSkillSourcesChanged(
        currentConfig.skills_sources || [],
        fullConfig.skills_sources || []
      );
      await invoke('save_storage_config', { config: fullConfig });
      
      await invoke('update_shortcuts', { 
        main: config.main_shortcut, 
        quick: config.quick_ai_shortcut 
      });

      if (config.language) {
        await invoke('update_tray_menu', { lang: config.language });
      }

      if (needSyncSkillsCatalog) {
        setMessage({ type: 'success', text: t('skillsSourcesSavedSyncing', 'Skills sources saved. Syncing recommendations...') });
        try {
          await invoke('skills_sync_now');
          await Promise.all([loadConfig(), loadSkillsSyncState()]);
          setMessage({ type: 'success', text: t('skillsSourcesSavedSynced', 'Skills sources saved and recommendations synced.') });
        } catch (syncErr: any) {
          await loadSkillsSyncState();
          setMessage({
            type: 'error',
            text: t('skillsSourcesSavedSyncFailed', 'Skills sources saved, but sync failed: {{message}}', {
              message: String(syncErr),
            }),
          });
        }
      } else {
        setMessage({ type: 'success', text: t('settingsSavedHotReload', 'Settings saved! Shortcuts updated immediately.') });
      }
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

  const sidebarItems = [
    { id: 'storage', name: t('dataStorage', 'Data Storage'), icon: HardDrive },
    { id: 'updates', name: t('updates', 'Updates'), icon: RefreshCw },
    { id: 'skills', name: t('skillsSourcesMenu', 'Skills 源'), icon: Sparkles },
    { id: 'proxy', name: t('proxy', 'Network Proxy'), icon: Globe },
    { id: 'shortcuts', name: t('shortcuts', 'Shortcuts'), icon: KeyboardIcon },
    { id: 'ai', name: t('aiSessions', 'AI Terminal'), icon: Terminal },
    { id: 'appearance', name: t('appearance', 'Appearance'), icon: Palette },
    { id: 'security', name: t('security', 'Security'), icon: ShieldCheck },
  ];

  const handleSkillsSyncNow = async () => {
    setLoading(true);
    try {
      await invoke('skills_sync_now');
      await loadSkillsSyncState();
      setMessage({ type: 'success', text: t('syncSuccess', 'Sync successful') });
      setTimeout(() => setMessage({ type: '', text: '' }), 3000);
    } catch (e: any) {
      await loadSkillsSyncState();
      setMessage({ type: 'error', text: e.toString() });
    } finally {
      setLoading(false);
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

  const ThemeIcon = theme === 'system' ? Monitor : theme === 'dark' ? Moon : Sun;
  const skillsSources = config.skills_sources || [];
  const skillsSyncSourceMap = new Map((skillsSyncState?.sources || []).map((s) => [s.source_id, s]));
  const enabledSkillsSources = skillsSources.filter((s) => s.enabled).length;
  const lastSkillsSyncText = config.skills_last_synced_at
    ? new Date(config.skills_last_synced_at * 1000).toLocaleString()
    : t('never', 'Never');
  const formatSyncTs = (ts?: number) => (ts ? new Date(ts * 1000).toLocaleString() : t('never', 'Never'));

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
          <button 
            onClick={saveConfig}
            disabled={loading}
            className="flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg disabled:opacity-50 transition-all font-semibold shadow-sm active:scale-95"
          >
            {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            {t('saveAllSettings', 'Save')}
          </button>
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
              {item.name}
            </button>
          ))}
        </div>

        {/* Content Area */}
        <div className="flex-1 overflow-y-auto p-8 bg-background/50">
          <div className="max-w-3xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-2 duration-500">
            
            {activeTab === 'storage' && (
              <div className="space-y-6">
                <section className="space-y-4">
                  <div className="flex flex-col gap-1">
                    <h2 className="text-lg font-semibold">{t('dataStorage', 'Data Storage Location')}</h2>
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
                      <label className="relative inline-flex items-center cursor-pointer">
                        <input
                          type="checkbox"
                          className="sr-only peer"
                          checked={!!config.auto_update_enabled}
                          onChange={(e) => setConfig((prev) => ({ ...prev, auto_update_enabled: e.target.checked }))}
                        />
                        <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-primary/20 rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all dark:border-gray-600 peer-checked:bg-primary"></div>
                      </label>
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
                      disabled={loading}
                      className="px-3 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 disabled:opacity-50"
                    >
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

                            {!!source.default_models?.length && (
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
            
            {/* Bottom Spacing */}
            <div className="h-20" />
          </div>
        </div>
      </div>
    </div>
  );
}
