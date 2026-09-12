/* ---------------- tokenize: async Shiki wrapper ----------------
   Public API: `tokenize(text, lang) -> Promise<Token[]>`. Same shape as
   the previous heuristic tokenizer, so call sites (diff.ts) need minimal
   changes. The TokenKind union is the same 10 kinds the CSS in styles.css
   expects under `.syn-*` rules. */

import type { LangId } from './lang';
import { ensureHighlighter } from './shiki';
import { mapScopeToKind } from './scopeMap';

export type TokenKind = 'kw' | 'str' | 'cmt' | 'num' | 'fn' | 'type' | 'var' | 'op' | 'punc' | 'prop';

export interface Token {
  kind: TokenKind;
  start: number;
  end: number;
}

/* tokenize a single line of source. Returns half-open [start, end) ranges
   into `text` so the renderer can slice the original string and emit
   `<span class="syn-{kind}">` spans. Whitespace-only or empty lines yield
   an empty array (the caller falls through to plain text). */
export async function tokenize(text: string, lang: LangId | null): Promise<Token[]> {
  if (!lang) return [];
  if (!text) return [];

  const { hl, shikiId } = await ensureHighlighter(lang);
  if (!shikiId) return [];

  /* We feed Shiki the line and ask for explanations so we can read the
     TextMate scope stack. CSS owns the rendering via `.syn-*` rules. */
  const result = hl.codeToTokens(text, {
    lang: shikiId,
    theme: 'bentomux',
    includeExplanation: 'scopeName',
  });

  /* Shiki returns a 2D array (per line); we tokenized one line, so we
     flatten the first row. */
  const lineTokens = result.tokens[0] ?? [];
  const out: Token[] = [];
  for (const tok of lineTokens) {
    const exp = tok.explanation;
    const kind: TokenKind = exp && exp.length > 0
      ? mapScopeToKind(exp[0].scopes)
      : 'var';
    out.push({ kind, start: tok.offset, end: tok.offset + tok.content.length });
  }
  return out;
}
