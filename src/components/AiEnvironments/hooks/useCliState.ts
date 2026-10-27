import { useState } from 'react';
import type { CliTool } from '@/lib/serviceProviders';
import type {
  CliUpdateInfo,
  CliVersionState,
} from '../helpers/providerUtils';

export function useCliState() {
  const [cliVersions, setCliVersions] = useState<Partial<Record<CliTool, CliVersionState>>>({});
  const [checkingVersions, setCheckingVersions] = useState<Partial<Record<CliTool, boolean>>>({});
  const [checkingAllVersions, setCheckingAllVersions] = useState(false);
  const [cliUpdates, setCliUpdates] = useState<Partial<Record<CliTool, CliUpdateInfo>>>({});
  const [checkingUpdates, setCheckingUpdates] = useState<Partial<Record<CliTool, boolean>>>({});
  const [updatingTool, setUpdatingTool] = useState<Partial<Record<CliTool, boolean>>>({});
  const [probingTool, setProbingTool] = useState<Partial<Record<CliTool, boolean>>>({});
  const [, setAutoImportInactiveNotice] = useState<Partial<Record<CliTool, string>>>({});

  return {
    cliVersions,
    setCliVersions,
    checkingVersions,
    setCheckingVersions,
    checkingAllVersions,
    setCheckingAllVersions,
    cliUpdates,
    setCliUpdates,
    checkingUpdates,
    setCheckingUpdates,
    updatingTool,
    setUpdatingTool,
    probingTool,
    setProbingTool,
    setAutoImportInactiveNotice,
  };
}

export type CliStateBindings = ReturnType<typeof useCliState>;
