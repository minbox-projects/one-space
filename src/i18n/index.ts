import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enLauncher from "./locales/en/launcher";
import zhLauncher from "./locales/zh/launcher";
import enAiGateway from "./locales/en/aiGateway";
import zhAiGateway from "./locales/zh/aiGateway";
import enAiUsage from "./locales/en/aiUsage";
import zhAiUsage from "./locales/zh/aiUsage";
import enAiSessions from "./locales/en/aiSessions";
import zhAiSessions from "./locales/zh/aiSessions";
import enNews from "./locales/en/news";
import zhNews from "./locales/zh/news";
import enAiWorkflow from "./locales/en/aiWorkflow";
import zhAiWorkflow from "./locales/zh/aiWorkflow";
import enAiEnvironments from "./locales/en/aiEnvironments";
import zhAiEnvironments from "./locales/zh/aiEnvironments";
import enSshTunnels from "./locales/en/sshTunnels";
import zhSshTunnels from "./locales/zh/sshTunnels";
import enSettings from "./locales/en/settings";
import zhSettings from "./locales/zh/settings";
import enTray from "./locales/en/tray";
import zhTray from "./locales/zh/tray";
import enMessages from "./locales/en/messages";
import zhMessages from "./locales/zh/messages";
import enBleLock from "./locales/en/bleLock";
import zhBleLock from "./locales/zh/bleLock";
import enServerMonitor from "./locales/en/serverMonitor";
import zhServerMonitor from "./locales/zh/serverMonitor";
import enFileSharing from "./locales/en/fileSharing";
import zhFileSharing from "./locales/zh/fileSharing";
import enPortTunnel from "./locales/en/portTunnel";
import zhPortTunnel from "./locales/zh/portTunnel";
import enSslCertificate from "./locales/en/sslCertificate";
import zhSslCertificate from "./locales/zh/sslCertificate";
import enSftp from "./locales/en/sftp";
import zhSftp from "./locales/zh/sftp";
import enUptimeMonitor from "./locales/en/uptimeMonitor";
import zhUptimeMonitor from "./locales/zh/uptimeMonitor";
import enGames from "./locales/en/games";
import zhGames from "./locales/zh/games";
import enToolbox from "./locales/en/toolbox";
import zhToolbox from "./locales/zh/toolbox";
import enProtocolRouter from "./locales/en/protocolRouter";
import zhProtocolRouter from "./locales/zh/protocolRouter";
import enWorkspaces from "./locales/en/workspaces";
import zhWorkspaces from "./locales/zh/workspaces";
import enSkills from "./locales/en/skills";
import zhSkills from "./locales/zh/skills";
import enSubagents from "./locales/en/subagents";
import zhSubagents from "./locales/zh/subagents";
import enMcp from "./locales/en/mcp";
import zhMcp from "./locales/zh/mcp";
import enMail from "./locales/en/mail";
import zhMail from "./locales/zh/mail";
import enDocs from "./locales/en/docs";
import zhDocs from "./locales/zh/docs";
import enAbout from "./locales/en/about";
import zhAbout from "./locales/zh/about";
import enCommon from "./locales/en/common";
import zhCommon from "./locales/zh/common";

// Localization resources are partitioned into domain files by top-level feature
// prefix. The ordered feature prefixes cover product areas such as launcher,
// aiGateway, aiEnvironments, sshTunnels, settings and tools; any key that matches
// no feature prefix belongs to the single `common` fallback bucket. English and
// Chinese use the same domain set and key assignment, so the merged bundles stay
// key-for-key aligned. Values are copied verbatim from the original flat bundle.
const resources = {
  en: {
    translation: {
    ...enLauncher,
    ...enAiGateway,
    ...enAiUsage,
    ...enAiSessions,
    ...enNews,
    ...enAiWorkflow,
    ...enAiEnvironments,
    ...enSshTunnels,
    ...enSettings,
    ...enTray,
    ...enMessages,
    ...enBleLock,
    ...enServerMonitor,
    ...enFileSharing,
    ...enPortTunnel,
    ...enSslCertificate,
    ...enSftp,
    ...enUptimeMonitor,
    ...enGames,
    ...enToolbox,
    ...enProtocolRouter,
    ...enWorkspaces,
    ...enSkills,
    ...enSubagents,
    ...enMcp,
    ...enMail,
    ...enDocs,
    ...enAbout,
    ...enCommon,
    },
  },
  zh: {
    translation: {
    ...zhLauncher,
    ...zhAiGateway,
    ...zhAiUsage,
    ...zhAiSessions,
    ...zhNews,
    ...zhAiWorkflow,
    ...zhAiEnvironments,
    ...zhSshTunnels,
    ...zhSettings,
    ...zhTray,
    ...zhMessages,
    ...zhBleLock,
    ...zhServerMonitor,
    ...zhFileSharing,
    ...zhPortTunnel,
    ...zhSslCertificate,
    ...zhSftp,
    ...zhUptimeMonitor,
    ...zhGames,
    ...zhToolbox,
    ...zhProtocolRouter,
    ...zhWorkspaces,
    ...zhSkills,
    ...zhSubagents,
    ...zhMcp,
    ...zhMail,
    ...zhDocs,
    ...zhAbout,
    ...zhCommon,
    },
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: "zh",
  fallbackLng: "en",
  interpolation: {
    escapeValue: false,
  },
});

export default i18n;
