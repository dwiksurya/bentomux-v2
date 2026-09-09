/* ---------------- TextMate scope -> TokenKind mapping ----------------
   Shiki returns ThemedToken[] where each token carries a stack of TextMate
   scopes (e.g. `source.ts`, `keyword.control.import.ts`, `punctuation.definition
   .begin.bracket.ts`). The renderer only needs 10 semantic kinds so the per-
   palette CSS rules in styles.css can keep doing their job.

   Algorithm: scan the scope stack from OUTERMOST (source.lang) to INNERMOST
   (most specific) and apply the first matching rule. A few TextMate patterns
   need cross-stack reasoning though:
     - a `"` inside a `string.quoted` is `punctuation.definition.string.*` —
       the surrounding string context should win.
     - HTML/XML tag brackets and JSX `<>` are `punctuation.definition.tag.*` —
       not regular punctuation, but our 10-kind palette can't tell them apart,
       so we treat them as `punc` to keep behavior consistent.
   The string/comment override below handles the common "definition" cases
   without needing a full TextMate parser. */

import type { TokenKind } from './tokenize';

interface ScopeRule {
  readonly prefix: string;
  readonly kind: TokenKind;
}

/* These prefixes indicate the token is part of a string or comment, and
   should win over deeper `punctuation.definition.*` / `constant.character.*`
   scopes that would otherwise classify the wrapper/delimiter characters. */
const STRING_OVERRIDE = ['string', 'markup.raw'];
const COMMENT_OVERRIDE = ['comment'];

const SCOPE_RULES: readonly ScopeRule[] = [
  /* ---------- identifiers with semantic role ---------- */

  { prefix: 'entity.name.function', kind: 'fn' },
  { prefix: 'meta.macro', kind: 'fn' },
  { prefix: 'variable.function', kind: 'fn' },
  { prefix: 'support.function', kind: 'fn' },
  { prefix: 'meta.function-call', kind: 'fn' },

  { prefix: 'entity.name.type', kind: 'type' },
  { prefix: 'entity.name.class', kind: 'type' },
  { prefix: 'entity.name.enum', kind: 'type' },
  { prefix: 'entity.name.interface', kind: 'type' },
  { prefix: 'entity.name.struct', kind: 'type' },
  { prefix: 'entity.name.trait', kind: 'type' },
  { prefix: 'entity.name.namespace', kind: 'type' },
  { prefix: 'entity.name.module', kind: 'type' },
  { prefix: 'support.type', kind: 'type' },
  { prefix: 'support.class', kind: 'type' },

  { prefix: 'variable.other.property', kind: 'prop' },
  { prefix: 'meta.property-name', kind: 'prop' },
  { prefix: 'support.variable.property', kind: 'prop' },
  { prefix: 'entity.other.attribute-name', kind: 'prop' },

  /* ---------- literal values ---------- */

  { prefix: 'constant.numeric', kind: 'num' },
  { prefix: 'constant.character', kind: 'num' },
  { prefix: 'constant.language', kind: 'var' },

  /* ---------- keywords, storage, modifiers ---------- */

  /* storage modifiers / type declarations that read as keywords:
     `def` in Python, `let`/`const`/`class` in TS, `fn` in Rust, etc. */
  { prefix: 'storage.type', kind: 'kw' },
  { prefix: 'storage.modifier', kind: 'kw' },
  { prefix: 'keyword.control', kind: 'kw' },
  { prefix: 'keyword.other', kind: 'kw' },
  { prefix: 'keyword.declaration', kind: 'kw' },
  { prefix: 'keyword', kind: 'kw' },

  /* ---------- punctuation ---------- */

  /* `keyword.operator` AFTER `keyword` so `keyword.control` wins, but plain
     operators (`=`, `+`, `&&`) hit this rule. */
  { prefix: 'keyword.operator', kind: 'op' },
  { prefix: 'punctuation', kind: 'punc' },
  { prefix: 'meta.brace', kind: 'punc' },
  { prefix: 'meta.delimiter', kind: 'punc' },

  /* ---------- generic identifiers + constants ---------- */
  { prefix: 'variable', kind: 'var' },
  { prefix: 'constant', kind: 'var' },
];

function hasAnyScope(scopes: readonly { scopeName: string }[], prefixes: readonly string[]): boolean {
  for (const s of scopes) {
    for (const p of prefixes) {
      if (s.scopeName === p || s.scopeName.startsWith(p + '.') || s.scopeName.startsWith(p + ' ')) {
        return true;
      }
    }
  }
  return false;
}

/* map a TextMate scope stack to one of the 10 CSS-bound kinds. Scan
   outermost-to-innermost so deeper, more specific scopes win on ties. */
export function mapScopeToKind(scopes: readonly { scopeName: string }[]): TokenKind {
  /* cross-stack overrides: if any parent scope indicates string or comment,
     that wins regardless of punctuation/character scopes below. */
  if (hasAnyScope(scopes, COMMENT_OVERRIDE)) return 'cmt';
  if (hasAnyScope(scopes, STRING_OVERRIDE)) return 'str';

  for (const s of scopes) {
    const name = s.scopeName;
    for (const rule of SCOPE_RULES) {
      if (name === rule.prefix || name.startsWith(rule.prefix + '.') || name.startsWith(rule.prefix + ' ')) {
        return rule.kind;
      }
    }
  }
  return 'var';
}
