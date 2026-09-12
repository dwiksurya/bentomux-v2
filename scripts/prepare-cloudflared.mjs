import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, createWriteStream, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const VERSION = '2026.9.0';
const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const RESOURCE_ROOT = join(ROOT, 'resources', 'cloudflared');

const RELEASES = {
  'darwin-x86_64': {
    asset: 'cloudflared-darwin-amd64.tgz',
    sha256: '8f2ecf41776d942bcc8070a56e7bafa4c5de70a1d1781110e2eb3774cca512a8',
    archive: true,
  },
  'darwin-aarch64': {
    asset: 'cloudflared-darwin-arm64.tgz',
    sha256: 'c0eccb3758420d1f4e46cbf2b8ecde01d9802a154232a817f25133340009fcc7',
    archive: true,
  },
  'linux-x86_64': {
    asset: 'cloudflared-linux-amd64',
    sha256: '53b7a7a5420d188758d24341294acb0d1bca54296548ac05e38811a694ac6134',
    archive: false,
  },
  'linux-aarch64': {
    asset: 'cloudflared-linux-arm64',
    sha256: '98aca3173f73248fad6180fc75dade2d186a6e54fa807e088108cb4345de8efe',
    archive: false,
  },
  'windows-x86_64': {
    asset: 'cloudflared-windows-amd64.exe',
    sha256: '547057326266f0e1c7d50d102dbd22ff283d740c055bd61e94f10e2c606f89af',
    archive: false,
  },
};

if (process.env.TAURI_ENV_TARGET_TRIPLE === 'universal-apple-darwin' && process.argv.includes('--check')) {
  for (const target of ['darwin-x86_64', 'darwin-aarch64']) console.log(`${target}: ${join(RESOURCE_ROOT, target, 'cloudflared')}`);
  process.exit(0);
}

if (process.env.TAURI_ENV_TARGET_TRIPLE === 'universal-apple-darwin') {
  for (const target of ['darwin-x86_64', 'darwin-aarch64']) {
    execFileSync(process.execPath, [fileURLToPath(import.meta.url)], {
      stdio: 'inherit',
      env: { ...process.env, TAURI_ENV_TARGET_TRIPLE: '', CLOUDFLARED_TARGET: target },
    });
  }
  process.exit(0);
}

function hostTarget() {
  const triple = process.env.TAURI_ENV_TARGET_TRIPLE;
  if (triple) {
    if (triple === 'x86_64-apple-darwin') return 'darwin-x86_64';
    if (triple === 'aarch64-apple-darwin') return 'darwin-aarch64';
    if (triple === 'x86_64-unknown-linux-gnu') return 'linux-x86_64';
    if (triple === 'aarch64-unknown-linux-gnu') return 'linux-aarch64';
    if (triple === 'x86_64-pc-windows-msvc' || triple === 'x86_64-pc-windows-gnu') return 'windows-x86_64';
    throw new Error(`Unsupported cloudflared Tauri target: ${triple}`);
  }
  const platform = process.platform;
  const arch = process.arch;
  if (platform === 'darwin' && arch === 'x64') return 'darwin-x86_64';
  if (platform === 'darwin' && arch === 'arm64') return 'darwin-aarch64';
  if (platform === 'linux' && arch === 'x64') return 'linux-x86_64';
  if (platform === 'linux' && arch === 'arm64') return 'linux-aarch64';
  if (platform === 'win32' && arch === 'x64') return 'windows-x86_64';
  throw new Error(`Unsupported cloudflared build target: ${platform}/${arch}`);
}
function targetPath(target) {
  const file = target.startsWith('windows-') ? 'cloudflared.exe' : 'cloudflared';
  return join(RESOURCE_ROOT, target, file);
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

async function download(url, destination) {
  const response = await fetch(url);
  if (!response.ok || !response.body) throw new Error(`Download failed (${response.status}): ${url}`);
  await new Promise((resolve, reject) => {
    const stream = createWriteStream(destination);
    stream.on('error', reject);
    stream.on('finish', resolve);
    response.body.pipeTo(new WritableStream({
      write(chunk) { stream.write(chunk); },
      close() { stream.end(); },
      abort(error) { stream.destroy(error); },
    })).catch(reject);
  });
}

const target = process.env.CLOUDFLARED_TARGET || hostTarget();
const release = RELEASES[target];
if (!release) throw new Error(`Unsupported cloudflared target: ${target}`);
const destination = targetPath(target);
const checkOnly = process.argv.includes('--check');

if (checkOnly) {
  console.log(`${target}: ${destination}`);
  process.exit(0);
}

if (existsSync(destination) && sha256(destination) === release.sha256) {
  console.log(`cloudflared ${VERSION} already prepared for ${target}`);
  process.exit(0);
}

const work = mkdtempSync(join(tmpdir(), 'bentomux-cloudflared-'));
const archive = join(work, release.asset);
const url = `https://github.com/cloudflare/cloudflared/releases/download/${VERSION}/${release.asset}`;
try {
  console.log(`Downloading cloudflared ${VERSION} for ${target}...`);
  await download(url, archive);
  if (sha256(archive) !== release.sha256) throw new Error(`Checksum mismatch for ${release.asset}`);

  const outputDir = join(work, 'output');
  mkdirSync(outputDir);
  let binary = archive;
  if (release.archive) {
    execFileSync('tar', ['-xzf', archive, '-C', outputDir]);
    binary = join(outputDir, readdirSync(outputDir).find(name => name === 'cloudflared') || 'cloudflared');
  }
  if (!existsSync(binary)) throw new Error(`Archive did not contain cloudflared: ${release.asset}`);

  mkdirSync(dirname(destination), { recursive: true });
  const temporary = `${destination}.tmp`;
  copyFileSync(binary, temporary);
  if (!existsSync(temporary)) throw new Error(`Failed to stage cloudflared for ${target}`);
  if (process.platform !== 'win32') chmodSync(temporary, 0o755);
  renameSync(temporary, destination);
  console.log(`Prepared ${destination}`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
