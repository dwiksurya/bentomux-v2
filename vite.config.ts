import { defineConfig } from 'vite';
import { resolve } from 'path';

// The renderer is plain vanilla TS (copied from the Electron version);
// it lives at src/ with index.html at the root and modules in src/src/.
// `approval.html` is the always-on-top overlay window (agent PermissionRequests),
// built as a second page just like the main one.
export default defineConfig({
  root: 'src',
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: '../out/renderer',
    emptyOutDir: true,
    target: 'es2022',
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'src/index.html'),
        approval: resolve(__dirname, 'src/approval.html'),
      },
    },
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src/src'),
    },
  },
});
