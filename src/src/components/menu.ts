/* ---------------- context menu (one popup, every right-click) ---------------- */

import { h } from '../dom';

export interface MenuItem { label: string; action?: () => void; disabled?: boolean }
export interface MenuSep { sep: true }
export type MenuEntry = MenuItem | MenuSep;

let current: HTMLElement | null = null;
/* element that opened the menu as a left-click toggle; pointerdowns on it
   don't dismiss the menu so its own click handler can close it instead */
let toggleAnchor: Element | null = null;
let dismissFn: ((e: Event) => void) | null = null;
let keyFn: ((e: KeyboardEvent) => void) | null = null;

export function closeContextMenu(): void {
  toggleAnchor = null;
  if (dismissFn) {
    document.removeEventListener('pointerdown', dismissFn, true);
    document.removeEventListener('contextmenu', dismissFn, true);
    dismissFn = null;
  }
  if (keyFn) {
    document.removeEventListener('keydown', keyFn, true);
    keyFn = null;
  }
  window.removeEventListener('blur', closeContextMenu);
  current?.remove();
  current = null;
}

export function contextMenuAnchoredTo(el: Element): boolean {
  return current != null && toggleAnchor === el;
}

export function openContextMenu(x: number, y: number, entries: MenuEntry[], toggleAnchorEl?: Element): void {
  closeContextMenu();
  const menu = h('div', { class: 'ctx-menu', role: 'menu' });
  for (const entry of entries) {
    if ('sep' in entry) { menu.append(h('div', { class: 'ctx-sep' })); continue; }
    menu.append(h('button', {
      class: 'ctx-item' + (entry.disabled ? ' disabled' : ''),
      role: 'menuitem',
      onclick: () => { if (entry.disabled) return; closeContextMenu(); entry.action?.(); },
    }, entry.label));
  }
  document.body.append(menu);

  /* clamp inside the viewport once measured */
  const r = menu.getBoundingClientRect();
  menu.style.left = Math.max(4, Math.min(x, window.innerWidth - r.width - 4)) + 'px';
  menu.style.top = Math.max(4, Math.min(y, window.innerHeight - r.height - 4)) + 'px';
  current = menu;
  toggleAnchor = toggleAnchorEl ?? null;

  dismissFn = (e: Event): void => {
    if (toggleAnchor && e.target instanceof Node && toggleAnchor.contains(e.target)) return;
    if (!menu.contains(e.target as Node)) closeContextMenu();
  };
  keyFn = (e: KeyboardEvent): void => { if (e.key === 'Escape') closeContextMenu(); };
  /* defer so the very event that opened the menu can't dismiss it */
  setTimeout(() => {
    if (!current || !dismissFn || !keyFn) return;
    document.addEventListener('pointerdown', dismissFn, true);
    document.addEventListener('contextmenu', dismissFn, true);
    document.addEventListener('keydown', keyFn, true);
    window.addEventListener('blur', closeContextMenu);
  }, 0);
}
