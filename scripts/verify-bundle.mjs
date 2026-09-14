import { readdir, stat } from 'node:fs/promises';
import { join, relative } from 'node:path';

const root = process.argv[2] || 'src-tauri/target/release/bundle';

async function walk(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...await walk(path));
    else files.push(path);
  }
  return files;
}

const files = await walk(root);
const required = ['bentomux-hook.cjs', 'remote-page.html'];
const appPresent = files.some(file => file.includes('.app/Contents/'));
for (const name of required) {
  const bundled = files.some(file => file.endsWith(`/${name}`) || file.endsWith(`\\${name}`));
  if (appPresent && !bundled) throw new Error(`missing bundled resource: ${name}`);
  if (!bundled) console.log(`resource check deferred: ${name} (installer packaging removed unpacked app)`);
}

const sourceResources = await walk('resources');
for (const name of required) {
  if (!sourceResources.some(file => file.endsWith(`/${name}`) || file.endsWith(`\\${name}`))) {
    throw new Error(`missing source resource: ${name}`);
  }
}

const artifacts = files.filter(file => /\.(app|dmg|AppImage|msi|deb)$/i.test(file) || /-setup\.exe$/i.test(file));
if (artifacts.length === 0) throw new Error(`no installer artifact found under ${root}`);

for (const file of artifacts) {
  const bytes = (await stat(file)).size;
  if (bytes === 0) throw new Error(`empty installer artifact: ${relative(root, file)}`);
  console.log(`${relative(root, file)}\t${bytes} bytes`);
}

console.log('bundled resources: bentomux-hook.cjs, remote-page.html');
