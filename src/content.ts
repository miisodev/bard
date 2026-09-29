/*
  What a card's text is, when it is one recognisable thing: a colour or a web
  address. The card draws a swatch or puts the site first; everything else is
  prose and goes through the Markdown renderer.
*/

/** A card whose whole text is a CSS colour, or null. */
export function colorValue(content: string): string | null {
  const value = content.trim();
  if (/^#(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(value)) return value;
  if (/^(?:rgb|hsl|oklch)a?\([^()]{3,60}\)$/i.test(value)) return value;
  return null;
}

/** A card that is one web address, split so the site reads first. */
export function linkParts(content: string): { host: string; rest: string } | null {
  const value = content.trim();
  if (!/^https?:\/\/\S+$/i.test(value)) return null;
  try {
    const url = new URL(value);
    const rest = `${url.pathname === "/" ? "" : url.pathname}${url.search}${url.hash}`;
    return { host: url.host.replace(/^www\./, ""), rest };
  } catch {
    return null;
  }
}
