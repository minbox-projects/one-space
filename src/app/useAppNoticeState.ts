import { useRef, useState } from "react";
import type {
  CapabilityTargetTab,
  WorkspaceCapabilityContext,
} from "@/components/workspaceCapabilityContext";

export function useAppNoticeState() {
  const [skillsAutoUpdateNotice, setSkillsAutoUpdateNotice] = useState<
    string | null
  >(null);
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
  const [autoUpdateNoticeOpen, setAutoUpdateNoticeOpen] = useState(false);
  const [ignoredUpdateVersion, setIgnoredUpdateVersion] = useState<
    string | null
  >(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [runtimeErrorCopied, setRuntimeErrorCopied] = useState(false);
  const [workspaceCapabilityNavigation, setWorkspaceCapabilityNavigation] =
    useState<{
      targetTab: CapabilityTargetTab;
      context: WorkspaceCapabilityContext;
    } | null>(null);
  const ignoredUpdateVersionRef = useRef<string | null>(null);
  const activeTabRef = useRef("launcher");

  return {
    skillsAutoUpdateNotice,
    setSkillsAutoUpdateNotice,
    updateDialogOpen,
    setUpdateDialogOpen,
    autoUpdateNoticeOpen,
    setAutoUpdateNoticeOpen,
    ignoredUpdateVersion,
    setIgnoredUpdateVersion,
    runtimeError,
    setRuntimeError,
    runtimeErrorCopied,
    setRuntimeErrorCopied,
    workspaceCapabilityNavigation,
    setWorkspaceCapabilityNavigation,
    ignoredUpdateVersionRef,
    activeTabRef,
  };
}
