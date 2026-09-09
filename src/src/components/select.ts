/* ---------------- select ---------------- */

import { h } from '../dom';

export function selectEl(opts: Array<[string, string]>, val?: string, attrs?: Record<string, unknown>): HTMLSelectElement {
  const sel = h('select', { class: 'select-input', ...(attrs || {}) }) as unknown as HTMLSelectElement;
  for (const [v, label] of opts) sel.append(h('option', { value: v }, label));
  if (val != null) sel.value = val;
  return sel;
}
