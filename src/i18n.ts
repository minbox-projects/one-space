import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

// the translations
const resources = {
  en: {
    translation: {
      "launcher": "Launcher",
      "aiSessions": "AI Sessions",
      "sshServers": "SSH Servers",
      "snippets": "Snippets",
      "bookmarks": "Bookmarks",
      "notes": "Notes",
      "cloudDrive": "Cloud Drive",
      "mail": "Mail",
      "settings": "Settings",
      "search": "Search...",
      "dashboard": "Dashboard",
      "contentArea": "Content Area",
      "manageAiAssistants": "Manage your terminal-based AI assistants",
      "newSession": "New Session",
      "createNewAiSession": "Create New AI Session",
      "sessionName": "Session Name",
      "aiCommand": "AI Command",
      "workingDirectory": "Working Directory",
      "browse": "Browse",
      "cancel": "Cancel",
      "launch": "Launch",
      "noActiveSessions": "No active AI sessions found.",
      "createOneToGetStarted": "Create one to get started.",
      "attached": "Attached",
      "kill": "Kill",
      "attach": "Attach",
      "provideNameAndDir": "Please provide both a session name and directory.",
      "confirmKill": "Are you sure you want to kill session {{name}}?",
      "selectProjectDir": "Select a project directory...",
      "emptyTerminal": "Bash (Empty Terminal)",
      "runningInBackground": "Running in background",
      "attachedElsewhere": "Attached elsewhere",
      "toggleLanguage": "English",
      "notInTauri": "Error: Not running in Tauri desktop environment. Please use 'npm run tauri dev'.",
      "manageCommands": "Manage Commands",
      "commandName": "Name (e.g. Claude)",
      "commandValue": "Command (e.g. claude code)",
      "add": "Add",
      "restoreDefaults": "Restore Defaults",
      "themeSystem": "System Theme",
      "themeLight": "Light Theme",
      "themeDark": "Dark Theme"
    }
  },
  zh: {
    translation: {
      "launcher": "启动台",
      "aiSessions": "AI 会话",
      "sshServers": "SSH 服务器",
      "snippets": "代码片段",
      "bookmarks": "收藏夹",
      "notes": "备忘录",
      "cloudDrive": "云盘",
      "mail": "邮件",
      "settings": "设置",
      "search": "搜索...",
      "dashboard": "仪表盘",
      "contentArea": "内容区域",
      "manageAiAssistants": "管理基于终端的 AI 助手",
      "newSession": "新建会话",
      "createNewAiSession": "创建新的 AI 会话",
      "sessionName": "会话名称",
      "aiCommand": "AI 命令",
      "workingDirectory": "工作目录",
      "browse": "浏览",
      "cancel": "取消",
      "launch": "启动",
      "noActiveSessions": "未找到活跃的 AI 会话。",
      "createOneToGetStarted": "创建一个开始使用。",
      "attached": "已挂载",
      "kill": "终止",
      "attach": "恢复",
      "provideNameAndDir": "请提供会话名称和工作目录。",
      "confirmKill": "确定要终止会话 {{name}} 吗？",
      "selectProjectDir": "选择项目目录...",
      "emptyTerminal": "Bash (空终端)",
      "runningInBackground": "后台运行中",
      "attachedElsewhere": "已在其他窗口挂载",
      "toggleLanguage": "中文",
      "notInTauri": "错误：未在 Tauri 桌面环境中运行。请使用 'npm run tauri dev' 启动以获得完整的系统功能。",
      "manageCommands": "管理命令",
      "commandName": "名称 (如 Claude)",
      "commandValue": "命令 (如 claude code)",
      "add": "添加",
      "restoreDefaults": "恢复默认",
      "themeSystem": "跟随系统",
      "themeLight": "浅色模式",
      "themeDark": "深色模式"
    }
  }
};

i18n
  .use(initReactI18next)
  .init({
    resources,
    lng: "zh",
    fallbackLng: "en",
    interpolation: {
      escapeValue: false
    }
  });

export default i18n;