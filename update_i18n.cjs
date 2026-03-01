const fs = require('fs');
const path = require('path');

const i18nPath = path.join('src', 'i18n.ts');
let content = fs.readFileSync(i18nPath, 'utf8');

const additions = {
  en: {
    cliInstalled: "CLI tool installed to ~/.local/bin/onespace",
    installCliTitle: "Install CLI tool to ~/.local/bin",
    installCli: "Install CLI",
    settingsSaved: "Settings saved successfully",
    syncSuccess: "Sync successful",
    dataStorage: "Data Storage Location",
    storageType: "Storage Type",
    local: "Local (~/.config/onespace/data)",
    gitRepo: "Git Repository",
    remoteUrl: "Remote URL",
    authMethod: "Authentication Method",
    token: "Password / Personal Access Token",
    sshKeyPath: "Private Key Path",
    syncing: "Syncing...",
    syncNow: "Sync Now",
    syncHint: "Data is auto-synced on save. Use this to force pull/push."
  },
  zh: {
    cliInstalled: "CLI 工具已成功安装到 ~/.local/bin/onespace",
    installCliTitle: "安装 CLI 工具到 ~/.local/bin",
    installCli: "安装 CLI",
    settingsSaved: "设置已成功保存",
    syncSuccess: "同步成功",
    dataStorage: "数据存储位置",
    storageType: "存储类型",
    local: "本地 (~/.config/onespace/data)",
    gitRepo: "Git 仓库",
    remoteUrl: "远程 URL",
    authMethod: "认证方式",
    token: "密码 / 个人访问令牌 (Token)",
    sshKeyPath: "私钥路径",
    syncing: "同步中...",
    syncNow: "立即同步",
    syncHint: "数据在保存时会自动同步。点击此按钮强制拉取/推送。"
  }
};

// Insert into English block
for (const [key, val] of Object.entries(additions.en)) {
  if (!content.includes(`"${key}":`)) {
    content = content.replace(
      /"settings": "Settings",/,
      `"settings": "Settings",\n      "${key}": "${val}",`
    );
  }
}

// Insert into Chinese block
for (const [key, val] of Object.entries(additions.zh)) {
  if (!content.includes(`"${key}":`)) {
    content = content.replace(
      /"settings": "设置",/,
      `"settings": "设置",\n      "${key}": "${val}",`
    );
  }
}

fs.writeFileSync(i18nPath, content);
