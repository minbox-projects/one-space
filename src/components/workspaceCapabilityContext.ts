export type WorkspaceCapabilityEntry = 'installed' | 'recommended' | 'repository';

export type CapabilityTargetTab = 'skills' | 'subagents' | 'mcp-servers';

export type WorkspaceCapabilityContext = {
  workspaceId: string;
  workspaceName: string;
  rootPath: string;
  persistence: 'one_shot';
  entry?: WorkspaceCapabilityEntry;
};
