/* ---------------- yes/no segmented toggle ----------------
   Two-option boolean control styled like the remote on/off switch
   (off side first, active side carries the .seg accent). Labels default
   to No/Yes; pass `labels` for other wordings (e.g. Active/Inactive on
   the agent resource tabs). tag 'span' variant exists for contexts
   that already live inside a <button> (resource cards/rows), since
   nested <button> elements are invalid HTML. */

import { h } from '../dom';

export function toggleSeg(
  on: boolean,
  onChange: (next: boolean) => void,
  opts?: { labels?: [string, string]; tag?: 'button' | 'span' },
): HTMLElement {
  const [offLabel, onLabel] = opts?.labels ?? ['No', 'Yes'];
  const tag = opts?.tag ?? 'button';
  let current = on;
  const mk = (label: string): HTMLElement => h(tag, {
    class: 'seg-opt',
    type: tag === 'button' ? 'button' : null,
    role: tag === 'span' ? 'button' : null,
    tabindex: tag === 'span' ? '0' : null,
  }, label);
  const off = mk(offLabel);
  const onBtn = mk(onLabel);
  const paint = (): void => {
    onBtn.classList.toggle('on', current);
    off.classList.toggle('on', !current);
  };
  const choose = (next: boolean): void => {
    if (current === next) return;
    current = next;
    paint();
    onChange(next);
  };
  off.addEventListener('click', () => choose(false));
  onBtn.addEventListener('click', () => choose(true));
  paint();
  return h('div', { class: 'seg' }, off, onBtn);
}
