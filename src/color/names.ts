/*
  What a colour is called. Given a hex value, colorName answers with the name
  of the nearest colour in the bundled "Best of" set from meodai/color-names
  (MIT, v14.62.0 — see THIRD_PARTY_NOTICES.md), so a swatch can introduce
  itself as "Chocolate Plum" instead of "#6f4a4a".

  Distances are measured in OKLab, where equal steps read as equally different
  everywhere in the space. The dataset is converted once at module load and
  lookups are memoised, so asking again on every render is free.
*/

import colornames from "./colornames.bestof.json";

interface NamedColor {
  name: string;
  /** OKLab, as [L, a, b]. */
  lab: [number, number, number];
}

/** A linear-light sRGB channel, from Ottosson's sRGB → OKLab note. */
function toLinear(channel: number): number {
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

/** Convert one "#rrggbb" value to OKLab. The dataset is all six digits. */
function hexToOklab(hex: string): [number, number, number] {
  const value = parseInt(hex.slice(1), 16);
  const r = toLinear(((value >> 16) & 0xff) / 255);
  const g = toLinear(((value >> 8) & 0xff) / 255);
  const b = toLinear((value & 0xff) / 255);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

const namedColors: NamedColor[] = colornames.map(({ name, hex }) => ({
  name,
  lab: hexToOklab(hex),
}));

/**
 * The nearest name by squared OKLab distance. The comparison is strictly
 * less-than, so of equally distant colours the first in dataset order wins —
 * a fixed rule that outlives any dataset revision.
 */
function nearestName(lab: [number, number, number]): string {
  let best = namedColors[0];
  let bestDistance = Infinity;
  for (const color of namedColors) {
    const dl = color.lab[0] - lab[0];
    const da = color.lab[1] - lab[1];
    const db = color.lab[2] - lab[2];
    const distance = dl * dl + da * da + db * db;
    if (distance < bestDistance) {
      bestDistance = distance;
      best = color;
    }
  }
  return best.name;
}

const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i;
const cache = new Map<string, string>();

/**
 * The name of the colour `hex` points at, given as "#rgb" or "#rrggbb" in
 * either case. Anything else — including a non-string from untyped callers —
 * answers "Unknown" rather than throwing. Equivalent spellings share one
 * memoised result, and ties in the nearest match go to the first dataset
 * entry, so the answer is deterministic.
 */
export function colorName(hex: string): string {
  if (typeof hex !== "string") return "Unknown";
  const key = hex.trim().toLowerCase();
  const cached = cache.get(key);
  if (cached !== undefined) return cached;
  let name = "Unknown";
  if (HEX.test(key)) {
    const full =
      key.length === 4 ? `#${key[1]}${key[1]}${key[2]}${key[2]}${key[3]}${key[3]}` : key;
    name = nearestName(hexToOklab(full));
  }
  cache.set(key, name);
  return name;
}
