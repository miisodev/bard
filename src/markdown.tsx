import { Fragment, type ReactNode } from "react";

/*
  Inline markdown for cards: **bold**, *italic*, ~~strikethrough~~, `code`.

  Underscore emphasis is deliberately unsupported. Captured text is mostly code
  and prose about code, where `snake_case` and `__dunder__` are far commoner than
  _emphasis_ — supporting it would mangle more than it formatted.

  Output is React elements, never HTML, so captured text cannot inject markup.
*/
const TOKEN = /(\*\*[^*\n]+\*\*|~~[^~\n]+~~|`[^`\n]+`|\*[^*\n]+\*)/;

/**
 * Render `text` with inline markdown, highlighting occurrences of `search`.
 *
 * Highlighting runs *inside* each token's content rather than over the raw
 * string, so a hit spanning a marker (`**bo` in `**bold**`) can't happen and a
 * hit inside emphasis still marks.
 */
export function renderInline(text: string, search = ""): ReactNode[] {
  return text.split(TOKEN).flatMap((part, i) => {
    // String.split with a capturing group interleaves: even indices are the
    // plain runs between tokens, odd indices are the tokens themselves.
    if (i % 2 === 0) return highlight(part, search, `t${i}`);

    const inner = (n: number) => highlight(part.slice(n, -n), search, `m${i}`);
    if (part.startsWith("**")) return <b key={i}>{inner(2)}</b>;
    if (part.startsWith("~~")) return <s key={i}>{inner(2)}</s>;
    if (part.startsWith("`")) return <code key={i} className="md-code">{inner(1)}</code>;
    return <i key={i}>{inner(1)}</i>;
  });
}

function highlight(text: string, search: string, keyBase: string): ReactNode[] {
  const q = search.trim().toLowerCase();
  if (!q || !text) return [text];

  const lower = text.toLowerCase();
  const out: ReactNode[] = [];
  let from = 0;
  let n = 0;

  for (;;) {
    const at = lower.indexOf(q, from);
    if (at < 0) break;
    if (at > from) out.push(<Fragment key={`${keyBase}-${n++}`}>{text.slice(from, at)}</Fragment>);
    out.push(<mark key={`${keyBase}-${n++}`}>{text.slice(at, at + q.length)}</mark>);
    from = at + q.length;
  }

  if (from === 0) return [text];
  if (from < text.length) out.push(<Fragment key={`${keyBase}-${n}`}>{text.slice(from)}</Fragment>);
  return out;
}
