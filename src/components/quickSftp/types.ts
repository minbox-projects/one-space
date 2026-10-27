export type SftpPreviewFile = {
  name: string;
  content: string;
};

export type SftpTailSession = {
  id: string;
  fileName: string;
  filePath: string;
  lines: string[];
  autoScroll: boolean;
  filter: string;
};
