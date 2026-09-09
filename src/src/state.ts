/* ---------------- ui state (session-only, not persisted) ---------------- */

import type { PaneNode } from '../shared/split-tree';

export type Route =
  | { view: 'terminal'; tabId: string }
  | { view: 'agents' }
  | { view: 'agentDetail'; agentId: string; tab: 'model' | 'memory' | 'skills' | 'mcp' }
  | { view: 'diff'; workspaceId: string; path: string };

export interface TabEntry {
  id: string;
  route: Route;
  workspaceId?: string;
  /* split view layout; undefined when the tab holds a single pane.
     route.tabId stays the anchor until that pane closes. */
  tree?: PaneNode;
  /* custom tab title (terminal tabs only); falls back to branch/workspace name */
  title?: string;
}

export const ui = {
  route: { view: 'agents' } as Route,
  tabs: [] as TabEntry[],
  activeTab: null as string | null,
  sel: null as string | null,
  sidebarOpen: false,
  gitPanelOpen: false,
  maximized: false,
  history: [] as string[],
  future: [] as string[],
};
