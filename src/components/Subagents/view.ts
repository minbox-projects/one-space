import type { SubagentsActions } from './hooks/useSubagentsActions';
import type { SubagentsData } from './hooks/useSubagentsData';

export type SubagentsView = SubagentsData & SubagentsActions;
