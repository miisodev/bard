import { Fragment, type ReactNode } from "react";

/*
  Inline markdown for cards: **bold**, *italic*, ~~strikethrough~~, `code`.

  Underscore emphasis is deliberately unsupported. Captured text is mostly code
  and prose about code, where `snake_case` and `__dunder__` are far commoner
  than _emphasis_ — supporting it would mangle more than it formatted.

  This is a small hand-written scanner rather than a split on one alternating
  pattern. It costs a few more lines and buys three things: marker precedence
  is stated rather than implied by alternation order, an unclosed marker is
  plain text instead of swallowing the rest of the line, and there is no regex
  to backtrack over a card full of asterisks.

  Output is React elements, never HTML, so captured text cannot inject markup.
*/

type Wrap = (content: ReactNode[], key: number) => ReactNode;

/** Longest marker first: `**` must be tested before `*`. */
const MARKERS: { open: string; wrap: Wrap }[] = [
  { open: "**", wrap: (c, k) => <b key={k}>{c}</b> },
  { open: "~~", wrap: (c, k) => <s key={k}>{c}</s> },
  { open: "`", wrap: (c, k) => <code key={k} className="md-code">{c}</code> },
  { open: "*", wrap: (c, k) => <i key={k}>{c}</i> },
];

/**
 * Render `text` with inline markdown, highlighting occurrences of `search`.
 *
 * Highlighting runs *inside* each span's content rather than over the raw
 * string, so a hit can never straddle a marker and a hit inside emphasis
 * still marks.
 */
export function renderInline(text: string, search = ""): ReactNode[] {
  const out: ReactNode[] = [];
  let plain = "";
  let key = 0;
  let i = 0;

  const flushPlain = () => {
    if (!plain) return;
    out.push(...highlight(plain, search, `p${key++}`));
    plain = "";
  };

  while (i < text.length) {
    const marker = MARKERS.find((m) => text.startsWith(m.open, i));
    if (!marker) {
      plain += text[i++];
      continue;
    }

    const from = i + marker.open.length;
    const close = text.indexOf(marker.open, from);
    // A marker with nothing after it, or spanning a line break, is just text —
    // an unterminated `*` should not italicise the rest of the card.
    const newline = text.indexOf("\n", from);
    if (close === -1 || close === from || (newline !== -1 && newline < close)) {
      plain += text[i++];
      continue;
    }

    flushPlain();
    const inner = text.slice(from, close);
    out.push(marker.wrap(highlight(inner, search, `m${key}`), key));
    key++;
    i = close + marker.open.length;
  }

  flushPlain();
  return out;
}

function highlight(text: string, search: string, keyBase: string): ReactNode[] {
  const needle = search.trim().toLowerCase();
  if (!needle || !text) return [text];

  const haystack = text.toLowerCase();
  const out: ReactNode[] = [];
  let from = 0;
  let n = 0;

  for (;;) {
    const at = haystack.indexOf(needle, from);
    if (at === -1) break;
    if (at > from) out.push(<Fragment key={`${keyBase}-${n++}`}>{text.slice(from, at)}</Fragment>);
    out.push(<mark key={`${keyBase}-${n++}`}>{text.slice(at, at + needle.length)}</mark>);
    from = at + needle.length;
  }

  if (from === 0) return [text];
  if (from < text.length) out.push(<Fragment key={`${keyBase}-${n}`}>{text.slice(from)}</Fragment>);
  return out;
}
