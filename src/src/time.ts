/* ---------------- time ---------------- */

export const HOUR = 36e5, DAY = 864e5;

export function rel(ts: number): string {
  const d = Date.now() - ts;
  if (d < 6e4) return 'now';
  if (d < HOUR) return Math.floor(d / 6e4) + 'm';
  if (d < DAY) return Math.floor(d / HOUR) + 'h';
  const days = Math.floor(d / DAY);
  if (days < 14) return days + 'd';
  if (days < 70) return Math.floor(days / 7) + 'w';
  return new Date(ts).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

export const abs = (ts: number): string => new Date(ts).toLocaleString();
export const dshort = (ts: number): string => new Date(ts).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
export const clock = (ts: number): string => new Date(ts).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
