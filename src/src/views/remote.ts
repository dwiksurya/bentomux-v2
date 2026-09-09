/* ---------------- remote monitor: dock button + popover ----------------
   Sits next to the settings gear at the bottom of the sidebar. Clicking
   it turns remote access ON (first click) and opens a popover with the
   pairing QR, the port field, and the on/off switch. The panel is
   appended to document.body — the sidebar re-renders on every runtime
   status tick, which would otherwise destroy an open panel. */

import { h } from '../dom';
import { field } from '../components/modal';
import { ic } from '../icons';
import type { RemotePairing } from '../../shared/types';

let info: RemotePairing | null = null;
let fetched = false;
let panel: HTMLElement | null = null;

function syncSeg(seg: HTMLElement, onIndex: 0 | 1): void {
  const [first, second] = seg.children as HTMLCollectionOf<HTMLElement>;
  first.classList.toggle('on', onIndex === 0);
  second.classList.toggle('on', onIndex === 1);
}

function updateDockDot(): void {
  const dot = document.querySelector('.sidebar-remote .dock-dot');
  if (dot) dot.classList.toggle('on', !!info?.running);
}

async function refresh(): Promise<void> {
  info = await window.bentomux.remoteInfo();
  updateDockDot();
  if (panel) paintPanel();
}

async function applyEnabled(on: boolean): Promise<void> {
  try {
    info = await window.bentomux.remoteSetEnabled(on);
  } catch (e) {
    if (info) info.error = e instanceof Error ? e.message : String(e);
  }
  updateDockDot();
  if (panel) paintPanel();
}

/* ---------------- dock button (rendered on every sidebar render) ---------------- */

export function remoteDockButton(): HTMLElement {
  if (!fetched) { fetched = true; void refresh(); }
  const btn = h('button', {
    class: 'iconbtn sidebar-remote' + (panel ? ' active' : ''),
    type: 'button',
    title: 'Remote access from phone',
    'aria-label': 'Remote access from phone',
    onclick: () => { if (panel) closePanel(); else openPanel(); },
  }, ic('phone'), h('span', { class: 'dock-dot' + (info?.running ? ' on' : '') }));
  return btn;
}

/* ---------------- popover ---------------- */

function openPanel(): void {
  if (panel) return;
  panel = h('div', { class: 'remote-pop' });
  document.body.append(panel);
  const anchor = document.querySelector('.sidebar-remote');
  if (anchor) {
    const r = anchor.getBoundingClientRect();
    panel.style.left = Math.round(r.left) + 'px';
    panel.style.bottom = Math.round(window.innerHeight - r.top + 8) + 'px';
  }
  paintPanel();
  /* "klik langsung on": first open while disabled turns it on */
  if (info && !info.enabled) void applyEnabled(true);
  window.addEventListener('pointerdown', onOutside, true);
  window.addEventListener('keydown', onKey, true);
}

function closePanel(): void {
  panel?.remove();
  panel = null;
  window.removeEventListener('pointerdown', onOutside, true);
  window.removeEventListener('keydown', onKey, true);
  document.querySelector('.sidebar-remote')?.classList.remove('active');
}

/* outside pointerdown (capture) dismisses; the dock button's own click
   then toggles it closed through its handler */
function onOutside(e: PointerEvent): void {
  const t = e.target as Node;
  if (panel?.contains(t)) return;
  if (document.querySelector('.sidebar-remote')?.contains(t)) return;
  closePanel();
}

function onKey(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    e.stopPropagation();
    closePanel();
  }
}

function copyUrl(url: string, btn: HTMLElement): void {
  const done = (): void => {
    btn.textContent = 'Copied';
    setTimeout(() => { btn.textContent = 'Copy URL'; }, 1200);
  };
  void navigator.clipboard.writeText(url).then(done).catch(() => {
    /* clipboard API can be unavailable on file:// — fall back to select+copy */
    const inp = panel?.querySelector('.remote-url') as HTMLInputElement | null;
    if (inp) { inp.select(); document.execCommand('copy'); done(); }
  });
}

function paintPanel(): void {
  if (!panel) return;
  panel.innerHTML = '';
  panel.append(
    h('div', { class: 'remote-pop-head' },
      h('strong', {}, 'Remote'),
      h('span', { class: 'remote-pop-status' },
        info?.running ? 'running · port ' + info.port : 'off')),
  );

  if (!info) {
    panel.append(h('div', { class: 'settings-hint' }, 'Loading…'));
    return;
  }

  const body = h('div', { class: 'settings-section' });

  const seg = h('div', { class: 'seg' },
    h('button', { onclick: () => void applyEnabled(false) }, 'Off'),
    h('button', { onclick: () => void applyEnabled(true) }, 'On'));
  syncSeg(seg, info.enabled ? 1 : 0);
  body.append(field('Remote access', seg));

  if (info.enabled) {
    const portInput = h('input', { type: 'number', min: '1024', max: '65535', value: info.port }) as HTMLInputElement;
    portInput.addEventListener('change', () => {
      const p = parseInt(portInput.value, 10);
      if (Number.isFinite(p)) void window.bentomux.remoteSetPort(p).then(next => { info = next; updateDockDot(); paintPanel(); });
    });
    body.append(field('Port', portInput));

    if (info.running && info.urls.length && info.qr) {
      const urlInput = h('input', { class: 'remote-url', readonly: true, value: info.urls[0] }) as HTMLInputElement;
      const copyBtn = h('button', { class: 'btn ghost', type: 'button', onclick: () => copyUrl(info!.urls[0], copyBtn) }, 'Copy URL');
      body.append(
        field('Pairing', h('div', { class: 'remote-pair' }, h('img', { src: info.qr, alt: 'Pairing QR code' }))),
        field('Pairing URL', h('div', { class: 'remote-urlrow' }, urlInput, copyBtn)),
      );
    } else if (info.error) {
      body.append(h('div', { class: 'settings-hint' }, info.error));
    } else {
      body.append(h('div', { class: 'settings-hint' }, 'Starting…'));
    }
    body.append(h('div', { class: 'settings-hint' },
      'Scan with your phone (same Wi-Fi) to watch terminals and answer agent approvals. Read-only except approve/deny.'));
  }

  panel.append(body);
}
