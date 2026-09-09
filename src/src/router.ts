/* ---------------- navigation (routes on top of tabs) ---------------- */

import { ui, type Route } from './state';
import { setRoute } from './views/tabs';

export function go(route: Route): void {
  ui.sel = null;
  ui.sidebarOpen = false;
  setRoute(route);
}
