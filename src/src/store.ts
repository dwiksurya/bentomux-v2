/* ---------------- client-side cache of main-process state ---------------- */

import type { AppState, RuntimeStatus } from '../shared/types';

export let db: AppState;

export function setDb(next: AppState): void {
  db = next;
}

/* git branch per workspace id */
export const branches = new Map<string, string | null>();

/* agent runtime status per terminal tab id */
export const runtime: Record<string, RuntimeStatus> = {};

/* last output activity per terminal tab id (drives the relative time) */
export const activity: Record<string, number> = {};
