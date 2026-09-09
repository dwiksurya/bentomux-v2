/* ---------------- render registry ----------------
   Views trigger re-renders through here; the actual implementations
   live in main.ts and are registered at boot. This keeps views and the
   root renderer from importing each other. */

import { ui, type Route } from './state';

let rootFn: (() => void) | null = null;
let contentFn: ((route: Route) => void) | null = null;
let sidebarFn: (() => void) | null = null;

export function registerRenderers(fns: {
  root: () => void;
  content: (route: Route) => void;
  sidebar: () => void;
}): void {
  rootFn = fns.root;
  contentFn = fns.content;
  sidebarFn = fns.sidebar;
}

export function render(): void {
  if (rootFn) rootFn();
}
export function renderContent(): void {
  if (contentFn) contentFn(ui.route);
}
export function renderSidebar(): void {
  if (sidebarFn) sidebarFn();
}
