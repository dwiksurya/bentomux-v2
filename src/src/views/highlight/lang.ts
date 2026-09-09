/* ---------------- language detection by file extension ----------------
   Maps a file path to a coarse language id consumed by tokenize.ts.
   Returns null for unknown extensions — callers should fall back to
   plain text rendering when the language is unknown. */

export type LangId =
  | 'ts' | 'tsx' | 'js' | 'jsx' | 'mjs' | 'cjs'
  | 'py' | 'rb' | 'go' | 'rs' | 'java' | 'kt' | 'cs' | 'cpp' | 'c' | 'h'
  | 'swift' | 'php' | 'sh' | 'sql' | 'html' | 'css' | 'scss' | 'json' | 'yaml' | 'toml' | 'md' | 'xml' | 'lua' | 'pl' | 'r' | 'dart';

const EXT: Record<string, LangId> = {
  /* JS family */
  ts: 'ts', mts: 'ts', cts: 'ts',
  tsx: 'tsx',
  js: 'js', mjs: 'js', cjs: 'js',
  jsx: 'jsx',
  /* common languages */
  py: 'py', pyi: 'py', pyw: 'py',
  rb: 'rb', rake: 'rb', gemspec: 'rb',
  go: 'go',
  rs: 'rs',
  java: 'java',
  kt: 'kt', kts: 'kt',
  cs: 'cs', csx: 'cs',
  cpp: 'cpp', cxx: 'cpp', cc: 'cpp', hpp: 'cpp', hh: 'cpp',
  c: 'c', h: 'c',
  swift: 'swift',
  php: 'php', phtml: 'php',
  sh: 'sh', bash: 'sh', zsh: 'sh',
  sql: 'sql',
  /* web */
  html: 'html', htm: 'html', vue: 'html', svelte: 'html',
  css: 'css',
  scss: 'scss', sass: 'scss', less: 'css',
  /* data */
  json: 'json', jsonc: 'json', json5: 'json',
  yaml: 'yaml', yml: 'yaml',
  toml: 'toml',
  xml: 'xml', xsd: 'xml', xsl: 'xml', svg: 'xml',
  /* docs */
  md: 'md', mdx: 'md', markdown: 'md',
  /* misc */
  lua: 'lua',
  pl: 'pl', pm: 'pl',
  r: 'r',
  dart: 'dart',
};

/* files that look like config or text and shouldn't claim to be code even
   though they have a recognized extension (e.g. .css used as a stylesheet
   vs .css used as a random file is not distinguishable here — that's fine). */

export function detectLang(path: string): LangId | null {
  if (!path) return null;
  /* basename, drop query/hash if any */
  const slash = path.lastIndexOf('/') >= 0 ? path.lastIndexOf('/') : path.lastIndexOf('\\');
  const base = slash >= 0 ? path.slice(slash + 1) : path;
  const dot = base.lastIndexOf('.');
  if (dot <= 0) return null;
  const ext = base.slice(dot + 1).toLowerCase();
  return EXT[ext] ?? null;
}
