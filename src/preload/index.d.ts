import type { BentomuxApi } from '../shared/types';

declare global {
  interface Window {
    bentomux: BentomuxApi;
  }
}

export {};
