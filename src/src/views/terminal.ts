/* ---------------- terminal view (xterm.js over node-pty) ----------------
   Each pane owns a persistent xterm instance living in a parking-lot div;
   switching tabs MOVES the DOM nodes, preserving scrollback and state.
   A tab may hold up to two panes side by side (split view). */

import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import '@xterm/xterm/css/xterm.css';
import { h, $ } from '../dom';
import { openContextMenu, type MenuEntry } from '../components/menu';
import { leafIds, type PaneNode } from '../../shared/split-tree';
import { db } from '../store';
import { splitTerminalPane, closeTerminalPane, setNodeDir } from './tabs';

interface Live {
  id: string;
  term: Terminal;
  fit: FitAddon;
  host: HTMLElement;
  observer: ResizeObserver | null;
}

const lives = new Map<string, Live>();
const pending = new Map<string, string>();
const lastFocus = new Map<string, number>();
/* per-axis divider position within a session ('%' of the axis) */
const ratioByNode = new Map<string, number>();
let parking: HTMLElement;

const MIN_RATIO_PCT = 15;
const MAX_RATIO_PCT = 85;
const FOCUS_TOGGLE_SELECTOR = '.workspace-child[data-pane]';

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function xtermTheme(): Record<string, string> {
  return {
    background: cssVar('--content-bg') || '#FAFAFA',
    foreground: cssVar('--ink') || '#292827',
    cursor: cssVar('--ink-2') || '#686766',
    selectionBackground: cssVar('--tint').replace(/rgba?\(([^)]+)\)/, 'rgba($1)') || 'rgba(191,198,199,.48)',
    selectionForeground: cssVar('--ink'),
  };
}

function monoFont(): string {
  const pref = db.prefs.font;
  if (pref) return pref;
  /* SF Mono/Menlo guaranteed on macOS with box-drawing support.
     ui-monospace can resolve to proportional fonts (Verdana) → skip it */
  const fallback = '"SF Mono", Menlo, Monaco, Consolas, "Courier New", monospace';
  return fallback;
}

const DEFAULT_TERM_FONT_SIZE = 12.5;

function termFontSize(): number {
  return db.prefs.fontSize || DEFAULT_TERM_FONT_SIZE;
}

/* push prefs font onto every live terminal, including parked panes */
export function applyTerminalFont(): void {
  for (const live of lives.values()) {
    live.term.options.fontFamily = monoFont();
    live.term.options.fontSize = termFontSize();
    /* cell metrics changed; refit so lines fill the pane again.
       Parked hosts have no layout, so fit() would throw. */
    if (live.host.isConnected && !parking.contains(live.host)) live.fit.fit();
  }
}

export function initTerminalEvents(): void {
  parking = $('#termParking');
  window.bentomux.onPtyData((id, chunk) => {
    console.log('[PTY DATA]', JSON.stringify(chunk));
    const live = lives.get(id);

    if (
      live &&
      live.host.isConnected &&
      live.host.parentElement &&
      !parking.contains(live.host)
    ) {
      live.term.write(chunk);
    } else {
      pending.set(id, (pending.get(id) || '') + chunk);
    }
  });
  window.bentomux.onPtyExit((id, _code) => {
    /* keep the dead shell visible until the user closes the pane/tab */
    const live = lives.get(id);
    if (live) live.term.write('\r\n\x1b[2m[process exited]\x1b[0m\r\n');
  });
}
function createXterm(tabId: string): { term: Terminal; fit: FitAddon; host: HTMLElement } {
  const fontFamily = monoFont();
  const fontSize = termFontSize();
  console.log('[DEBUG createXterm] Creating xterm for tabId:', tabId);
  console.log('[DEBUG createXterm] fontFamily:', fontFamily);
  console.log('[DEBUG createXterm] fontSize:', fontSize);
  const term = new Terminal({
    theme: xtermTheme(),
    fontFamily,
    fontSize,
    fontWeight: 'normal',
    fontWeightBold: 'bold',
    letterSpacing: 0,
    cursorBlink: false,
    allowProposedApi: true,
    scrollback: 1000,
    mouseWheelScrollSensitivity: 5,
    fastScrollSensitivity: 10,
  });
  console.log('[DEBUG createXterm] Terminal instance created:', term);
  const fit = new FitAddon();
  console.log('[DEBUG createXterm] FitAddon created');
  term.loadAddon(fit);
  term.loadAddon(new WebLinksAddon());
  console.log('[DEBUG createXterm] Addons loaded');
  const host = h('div', { class: 'terminal-host', 'data-tab-id': tabId });
  console.log('[DEBUG createXterm] Host div created:', host);

  // MUST call term.open() before term.element is available
  term.open(host);
  console.log('[DEBUG createXterm] term.open() called, term.element:', term.element);

  wireXtermEvents(term, tabId);
  wireFocusIn(host, tabId);
  wireFileDrop(host, tabId);
  const result = { term, fit, host };
  console.log('[DEBUG createXterm] Returning:', result);
  return result;
}

function wireXtermEvents(term: Terminal, tabId: string): void {
  term.attachCustomKeyEventHandler(e => {
    /* Ctrl+C / Cmd+C dengan selection aktif → copy; tanpa selection → biarkan SIGINT lewat */
    const isCopy = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c' && term.hasSelection();
    if (isCopy) {
      const sel = term.getSelection();
      if (sel) {
        e.preventDefault();
        void navigator.clipboard.writeText(sel);
        term.clearSelection();
      }
      return false;
    }
    /* Cmd/Ctrl+Home → scroll to top instantly */
    if ((e.metaKey || e.ctrlKey) && e.key === 'Home' && e.type === 'keydown') {
      e.preventDefault();
      term.scrollToTop();
      return false;
    }
    /* Cmd/Ctrl+End → scroll to bottom instantly */
    if ((e.metaKey || e.ctrlKey) && e.key === 'End' && e.type === 'keydown') {
      e.preventDefault();
      term.scrollToBottom();
      return false;
    }
    /* Shift+Enter → kirim newline literal (\n) bukan carriage return, supaya
       AI CLI tools (Claude, aider, dll.) bisa insert baris baru tanpa submit */
    if (e.shiftKey && e.key === 'Enter' && e.type === 'keydown') {
      e.preventDefault();
      window.bentomux.writeTab(tabId, '\n');
      return false;
    }
    return true;
  });
  term.onData(d => window.bentomux.writeTab(tabId, d));
  term.onResize(({ cols, rows }) => window.bentomux.resizeTab(tabId, cols, rows));
  wireClipboardPaste(term, tabId);
}

/* Handle Cmd+V paste of files and images from clipboard.
   Uses the textarea `paste` event (clipboardData) — no permission prompt. */
function wireClipboardPaste(term: Terminal, tabId: string): void {
  /* term.textarea is available after term.open() */
  const textarea = term.textarea;
  if (!textarea) return;
  textarea.addEventListener('paste', e => {
    const cd = e.clipboardData;
    if (!cd) return;

    /* 1. Files (e.g. dragged from Finder then Cmd+C → Cmd+V, or copied files) */
    if (cd.files.length > 0) {
      e.preventDefault();
      e.stopPropagation();
      const paths = Array.from(cd.files).map(f => {
        const p = (f as File & { path?: string }).path || f.name;
        return p.includes(' ') ? '"' + p + '"' : p;
      });
      window.bentomux.writeTab(tabId, paths.join(' '));
      return;
    }

    /* 2. Image in clipboard (screenshot, copied image) → save to temp PNG */
    const imageItem = Array.from(cd.items).find(it => it.type.startsWith('image/'));
    if (imageItem) {
      e.preventDefault();
      e.stopPropagation();
      const blob = imageItem.getAsFile();
      if (!blob) return;
      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = reader.result as string;
        /* strip "data:image/png;base64," prefix */
        const b64 = dataUrl.split(',')[1];
        if (!b64) return;
        void window.bentomux.saveClipboardImage(b64).then((path: string) => {
          const quoted = path.includes(' ') ? '"' + path + '"' : path;
          window.bentomux.writeTab(tabId, quoted);
        });
      };
      reader.readAsDataURL(blob);
      return;
    }
    /* plain text: let xterm handle it natively */
  });
}

function markFocusedPane(host: HTMLElement, tabId: string): void {
  const pane = host.closest('.pane');
  if (pane) {
    for (const el of document.querySelectorAll('.pane.focused')) el.classList.remove('focused');
    pane.classList.add('focused');
  }
  /* keep the sidebar's per-pane item in step without a re-render */
  for (const el of document.querySelectorAll(FOCUS_TOGGLE_SELECTOR)) {
    el.classList.toggle('active', el.getAttribute('data-pane') === tabId);
  }
}

/* xterm has no public focus event; its hidden textarea bubbles focusin */
function wireFocusIn(host: HTMLElement, tabId: string): void {
  host.addEventListener('focusin', () => {
    lastFocus.set(tabId, Date.now());
    markFocusedPane(host, tabId);
  });
}

/* Drag-and-drop files/images onto terminal → write absolute path(s) to PTY.
   Multiple files separated by spaces; paths with spaces are quoted. */
function wireFileDrop(host: HTMLElement, tabId: string): void {
  host.addEventListener('dragover', e => {
    if (!e.dataTransfer?.types.includes('Files')) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    host.classList.add('drop-active');
  });
  host.addEventListener('dragleave', e => {
    /* only clear when leaving the host itself, not a child */
    if (!host.contains(e.relatedTarget as Node)) host.classList.remove('drop-active');
  });
  host.addEventListener('drop', e => {
    e.preventDefault();
    host.classList.remove('drop-active');
    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;
    const paths = Array.from(files).map(f => {
      /* In Tauri/WebKit the File object exposes the real FS path via .path
         (non-standard but available in Tauri's WebView). Fall back to name. */
      const p = (f as File & { path?: string }).path || f.name;
      return p.includes(' ') ? '"' + p + '"' : p;
    });
    window.bentomux.writeTab(tabId, paths.join(' '));
  });
}

function ensureLive(tabId: string): Live {
  console.log('[DEBUG ensureLive] Called with tabId:', tabId);
  let live = lives.get(tabId);
  if (!live) {
    console.log('[DEBUG ensureLive] Creating new terminal for:', tabId);
    const { term, fit, host } = createXterm(tabId);
    console.log('[DEBUG ensureLive] createXterm returned - term:', term, 'fit:', fit, 'host:', host);
    live = { id: tabId, term, fit, host, observer: null };
    lives.set(tabId, live);
    console.log('[DEBUG ensureLive] Live object created and stored:', live);
  } else {
    console.log('[DEBUG ensureLive] Found existing live for:', tabId, live);
  }
  return live;
}

export function disposeTerminal(tabId: string): void {
  const live = lives.get(tabId);
  if (!live) return;
  if (live.observer) live.observer.disconnect();
  try { live.term.dispose(); } catch { /* already gone */ }
  pending.delete(tabId);
  lastFocus.delete(tabId);
  lives.delete(tabId);
}

function observe(container: HTMLElement, live: Live): void {
  requestAnimationFrame(() => {
    if (live.observer) live.observer.disconnect();
    live.observer = new ResizeObserver(() => {
      try { live.fit.fit(); } catch { /* not laid out yet */ }
    });
    live.observer.observe(container);
    try { live.fit.fit(); } catch { /* tiny container on first paint */ }
  });
}

/* the pane the user worked in most recently (default: the first) */
export function mostRecentPane(ids: string[]): string {
  return [...ids].sort((x, y) => (lastFocus.get(y) || 0) - (lastFocus.get(x) || 0))[0] || ids[0];
}

/* a sidebar item picked this pane: make it the focus target on next mount */
export function primePaneFocus(paneId: string): void {
  lastFocus.set(paneId, Date.now());
}

/* drop a remembered divider position so that axis returns to 50/50 */
export function resetPaneRatio(nodeKey: string): void {
  ratioByNode.delete(nodeKey);
}

/* draggable boundary between two branches; ResizeObservers refit both sides.
   Position is remembered per axis for the session. */
function wireDivider(divider: HTMLElement, first: HTMLElement, axis: HTMLElement, stacked: boolean, key: string): void {
  let dragging = false;
  const dragClass = stacked ? 'resizing-stacked' : 'resizing-panes';
  divider.addEventListener('pointerdown', e => {
    dragging = true;
    divider.setPointerCapture(e.pointerId);
    document.body.classList.add(dragClass);
  });
  divider.addEventListener('pointermove', e => {
    if (!dragging) return;
    applyDividerPosition(e, axis, first, stacked, key);
  });
  const stop = (): void => {
    if (!dragging) return;
    dragging = false;
    document.body.classList.remove(dragClass);
  };
  divider.addEventListener('pointerup', stop);
  divider.addEventListener('pointercancel', stop);
}

function applyDividerPosition(e: PointerEvent, axis: HTMLElement, first: HTMLElement, stacked: boolean, key: string): void {
  const r = axis.getBoundingClientRect();
  const pos = stacked ? e.clientY - r.top : e.clientX - r.left;
  const span = stacked ? r.height : r.width;
  if (span <= 0) return;
  const pct = clampRatio((pos / span) * 100);
  first.style.flex = '0 0 ' + pct + '%';
  ratioByNode.set(key, pct);
}

function clampRatio(pct: number): number {
  return Math.max(MIN_RATIO_PCT, Math.min(MAX_RATIO_PCT, pct));
}

function paneMenu(e: MouseEvent, ids: string[]): void {
  const paneEl = (e.target as HTMLElement).closest('.pane');
  const targetId = paneEl?.getAttribute('data-pane') || ids[0];
  const entries: MenuEntry[] = [
    { label: 'Split right', action: () => void splitTerminalPane(targetId, 'v') },
    { label: 'Split down', action: () => void splitTerminalPane(targetId, 'h') },
  ];
  if (ids.length > 1) entries.push({ label: 'Close pane', action: () => void closeTerminalPane(targetId) });
  openContextMenu(e.clientX, e.clientY, entries);
}

function dividerMenu(e: MouseEvent, key: string, dir: 'v' | 'h'): void {
  openContextMenu(e.clientX, e.clientY, [
    { label: dir === 'v' ? 'Switch to stacked' : 'Switch to side by side', action: () => setNodeDir(key, dir === 'v' ? 'h' : 'v') },
  ]);
}

/* build the DOM for one tree node; leaves host their xterm instance */
function mountAxis(parent: HTMLElement, node: PaneNode): HTMLElement {
  if (node.kind === 'leaf') {
    const live = ensureLive(node.id);
    const pane = h('div', { class: 'pane', 'data-pane': node.id });
    parent.append(pane);
    pane.append(live.host); /* moves out of the parking lot */
    observe(pane, live);
    return pane;
  }
  const axis = h('div', { class: 'split-axis' + (node.dir === 'h' ? ' stacked' : '') });
  parent.append(axis);
  const first = mountAxis(axis, node.first);
  const divider = h('div', { class: 'pane-divider', title: 'Drag to resize · right-click to switch direction' });
  divider.addEventListener('contextmenu', e => {
    e.preventDefault();
    e.stopPropagation();
    dividerMenu(e, node.key, node.dir);
  });
  axis.append(divider);
  mountAxis(axis, node.second);
  const saved = ratioByNode.get(node.key);
  if (saved != null) first.style.flex = '0 0 ' + saved + '%';
  wireDivider(divider, first, axis, node.dir === 'h', node.key);
  return axis;
}

export function terminalPage(start: PaneNode | string): HTMLElement {
  console.log('[DEBUG terminalPage] Called with start:', start);
  console.log('[DEBUG terminalPage] typeof start:', typeof start);
  try {
    const root = h('div', { class: 'terminal-page' });
    console.log('[DEBUG terminalPage] root created:', root);
    const body = h('div', { class: 'mux-body' + (typeof start === 'string' ? '' : ' split') });
    console.log('[DEBUG terminalPage] body created:', body);
    root.append(body);

    const ids = typeof start === 'string' ? [start] : leafIds(start);
    console.log('[DEBUG terminalPage] ids:', ids);
    const pageLives = ids.map(id => ensureLive(id));
    console.log('[DEBUG terminalPage] pageLives created:', pageLives.length);

    /* refresh palette in case the theme toggled since creation */
    for (const live of pageLives) {
      live.term.options.theme = xtermTheme() as never;
      live.term.options.fontFamily = monoFont();
    }

    if (typeof start === 'string') {
      console.log('[DEBUG terminalPage] Single pane mode, appending host');
      body.append(pageLives[0].host); /* moves out of the parking lot */
      observe(body, pageLives[0]);
    } else {
      console.log('[DEBUG terminalPage] Split mode, mounting axis');
      mountAxis(body, start);
    }

    for (const id of ids) {
      const buffered = pending.get(id);
      if (buffered) {
        pending.delete(id);
        lives.get(id)?.term.write(buffered);
      }
    }

    const focusLive = lives.get(mostRecentPane(ids));
    if (focusLive) {
      requestAnimationFrame(() => {
        try { focusLive.term.focus(); } catch { /* pane gone */ }
      });
    }

    root.addEventListener('contextmenu', e => {
      e.preventDefault();
      e.stopPropagation();
      paneMenu(e, ids);
    });

    console.log('[DEBUG terminalPage] Returning root:', root);
    return root;
  } catch (error) {
    console.error('[terminalPage] Error creating terminal:', error);
    const errorDiv = h('div', { class: 'page' }, h('p', {}, 'Terminal error: ' + (error instanceof Error ? error.message : String(error))));
    console.log('[DEBUG terminalPage] Returning error div:', errorDiv);
    return errorDiv;
  }
}
