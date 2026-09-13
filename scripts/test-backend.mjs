/* Runs the Rust unit tests (`cargo test --lib`) with the env var that makes
   src-tauri/build.rs embed a Common-Controls-v6 manifest into the test
   harness. On Windows the harness imports comctl32!TaskDialogIndirect (via
   rfd), which only resolves with that manifest — plain `cargo test` binaries
   die at load with STATUS_ENTRYPOINT_NOT_FOUND. `--lib` skips the bin's test
   harness, which contains no tests and would otherwise hit a duplicate
   manifest resource (CVT1100) while the env var is set. */

import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const manifestDir = join(dirname(dirname(fileURLToPath(import.meta.url))), 'src-tauri');

const result = spawnSync('cargo', ['test', '--no-default-features', '--lib'], {
  stdio: 'inherit',
  cwd: manifestDir,
  env: { ...process.env, BENTOMUX_EMBED_TEST_MANIFEST: '1' },
  shell: process.platform === 'win32',
});

process.exit(result.status ?? 1);
