/* ---------------- empty state (one sentence, at most one button) ---------------- */

import { h } from '../dom';

export function emptyState(text: string, btnLabel?: string, fn?: () => void): HTMLElement {
  const kids = [h('p', {}, text)];
  if (btnLabel && fn) kids.push(h('button', { class: 'btn', onclick: fn }, btnLabel));
  return h('div', { class: 'empty' }, kids);
}
