/* ---------------- Shiki singleton (lazy, fine-grained) ----------------
   One HighlighterCore for the whole renderer. Languages are dynamic-imported
   the first time they're needed; the highlighter itself is built with the
   JavaScript regex engine (no WASM) so first paint isn't blocked on the
   Oniguruma wasm fetch.

   We use a SINGLE theme (`github-dark`) only so Shiki has something to
   tokenize against — we never read the colors. The output that reaches the
   DOM is TextMate scopes -> our own `TokenKind` (see scopeMap.ts), and the
   per-palette colors come from `styles.css` via `.syn-*` rules. */

import type { HighlighterCore, LanguageInput } from 'shiki/core';
import type { LangId } from './lang';

/* Shiki's `lang` field accepts any bundled-language id (a string union),
   but the type isn't exported. We model it as `string` internally and cast
   at the API boundary. The set of valid ids is exactly what's in
   LANG_LOADERS below. */
type ShikiLang = string;

/* Map our short LangId -> Shiki's bundled-language id. The renderer-side
   EXT table gives us `ts`/`py`/`go`/etc; Shiki uses the full name. */
const SHIKI_LANG: Record<LangId, ShikiLang> = {
  ts: 'typescript',
  tsx: 'tsx',
  js: 'javascript',
  jsx: 'jsx',
  mjs: 'javascript',
  cjs: 'javascript',
  py: 'python',
  rb: 'ruby',
  go: 'go',
  rs: 'rust',
  java: 'java',
  kt: 'kotlin',
  cs: 'csharp',
  cpp: 'cpp',
  c: 'c',
  h: 'c',
  swift: 'swift',
  php: 'php',
  sh: 'bash',
  sql: 'sql',
  html: 'html',
  css: 'css',
  scss: 'scss',
  json: 'json',
  yaml: 'yaml',
  toml: 'toml',
  md: 'markdown',
  xml: 'xml',
  lua: 'lua',
  pl: 'perl',
  r: 'r',
  dart: 'dart',
};

/* languages to preload at boot — the ones most users will open first.
   5 grammars is <100KB and avoids a visible stall on the first diff. */
const PRELOAD_LANGS: readonly LangId[] = ['js', 'ts', 'py', 'go', 'rs'];

let highlighterPromise: Promise<HighlighterCore> | null = null;

/* dynamic import map for all 29 languages we support. Vite code-splits each
   entry, so unused grammars are never downloaded. Keyed by Shiki's full
   language name (so Vite emits stable filenames). */
const LANG_LOADERS: Partial<Record<ShikiLang, () => Promise<{ default: LanguageInput }>>> = {
  javascript: () => import('@shikijs/langs/javascript'),
  typescript: () => import('@shikijs/langs/typescript'),
  tsx: () => import('@shikijs/langs/tsx'),
  jsx: () => import('@shikijs/langs/jsx'),
  python: () => import('@shikijs/langs/python'),
  ruby: () => import('@shikijs/langs/ruby'),
  go: () => import('@shikijs/langs/go'),
  rust: () => import('@shikijs/langs/rust'),
  java: () => import('@shikijs/langs/java'),
  kotlin: () => import('@shikijs/langs/kotlin'),
  csharp: () => import('@shikijs/langs/csharp'),
  cpp: () => import('@shikijs/langs/cpp'),
  c: () => import('@shikijs/langs/c'),
  swift: () => import('@shikijs/langs/swift'),
  php: () => import('@shikijs/langs/php'),
  bash: () => import('@shikijs/langs/bash'),
  sql: () => import('@shikijs/langs/sql'),
  html: () => import('@shikijs/langs/html'),
  css: () => import('@shikijs/langs/css'),
  scss: () => import('@shikijs/langs/scss'),
  json: () => import('@shikijs/langs/json'),
  yaml: () => import('@shikijs/langs/yaml'),
  toml: () => import('@shikijs/langs/toml'),
  markdown: () => import('@shikijs/langs/markdown'),
  xml: () => import('@shikijs/langs/xml'),
  lua: () => import('@shikijs/langs/lua'),
  perl: () => import('@shikijs/langs/perl'),
  r: () => import('@shikijs/langs/r'),
  dart: () => import('@shikijs/langs/dart'),
};

async function preloadLanguages(): Promise<LanguageInput[]> {
  return Promise.all(
    PRELOAD_LANGS.map(async (id) => {
      const shikiId = SHIKI_LANG[id];
      const loader = LANG_LOADERS[shikiId];
      if (!loader) throw new Error(`preload lang "${id}" has no loader`);
      const mod = await loader();
      return mod.default;
    }),
  );
}

async function getHighlighter(): Promise<HighlighterCore> {
  if (!highlighterPromise) {
    highlighterPromise = (async () => {
      const { createHighlighterCore } = await import('shiki/core');
      const { createJavaScriptRegexEngine } = await import('@shikijs/engine-javascript');
      const { default: githubDark } = await import('@shikijs/themes/github-dark');
      const langs = await preloadLanguages();
      return createHighlighterCore({
        /* github-dark is required by codeToTokens even though we strip
           the colors — we only consume the TextMate scope explanations. */
        themes: [githubDark],
        langs,
        engine: createJavaScriptRegexEngine({ forgiving: true }),
      });
    })();
  }
  return highlighterPromise;
}

/* resolve a renderer-side LangId to the Shiki language id (or null if the
   id is somehow unknown to our map). */
export function shikiIdFor(lang: LangId): ShikiLang | null {
  return SHIKI_LANG[lang] ?? null;
}

/* ensure a language is loaded into the highlighter, dynamic-importing its
   grammar on first use. Idempotent: if already loaded, no-op. */
export async function ensureLang(lang: LangId): Promise<ShikiLang | null> {
  const hl = await getHighlighter();
  const shikiId = SHIKI_LANG[lang];
  if (!shikiId) return null;
  const loaded = hl.getLoadedLanguages();
  if (loaded.includes(shikiId)) return shikiId;
  const loader = LANG_LOADERS[shikiId];
  if (!loader) return null;
  const mod = await loader();
  await hl.loadLanguage(mod.default);
  return shikiId;
}

/* ensure the highlighter is ready AND a specific language is available.
   Returns the highlighter + the resolved shiki language id, so callers can
   run codeToTokens right after. */
export async function ensureHighlighter(lang: LangId): Promise<{ hl: HighlighterCore; shikiId: ShikiLang | null }> {
  const hl = await getHighlighter();
  const shikiId = await ensureLang(lang);
  return { hl, shikiId };
}
