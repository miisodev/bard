/*
  How every window paints itself: the theme, and the panel's background.

  The background follows the theme until someone chooses one. Light and Dark
  pin the measured canvas of that theme; Custom takes any colour. Cards, fields
  and menus keep the theme's surfaces either way — only the canvas changes — so
  the ink that sits directly on the canvas (section labels, pane titles, toolbar
  icons) is derived from the canvas itself: whichever of the two inks reads with
  more contrast against it.
*/

export type BackgroundMode = "theme" | "light" | "dark" | "custom";

export interface Appearance {
  theme: string;
  background: BackgroundMode;
  backgroundColor: string;
  /** Draw's canvas, chosen separately: it is a page, not window chrome. */
  drawBackground: BackgroundMode;
  drawBackgroundColor: string;
  /** Sampled with each read so a system theme change re-renders its users. */
  systemDark?: boolean;
}

/** The canvases of the two themes, measured off the Copper captures. */
export const CANVAS = { light: "#e6e7ea", dark: "#1c1c1f" } as const;

/**
 * Draw's page in each theme. The dark page is the dark card surface rather
 * than the dark canvas, so the page still reads as a page inside Draw's chrome.
 */
export const DRAW_CANVAS = { light: "#ffffff", dark: "#26262a" } as const;

const INK_DARK = [29, 29, 31] as const;
const INK_LIGHT = [245, 245, 247] as const;
const CANVAS_PROPERTIES = [
  "--canvas",
  "--on-canvas",
  "--on-canvas-2",
  "--on-canvas-3",
  "--canvas-rule",
  "--canvas-hover",
];

export function isHexColor(value: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(value);
}

function channel(value: number): number {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function luminance([r, g, b]: readonly number[]): number {
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function rgb(hex: string): number[] {
  return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
}

function contrast(a: number, b: number): number {
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

export function resolvedTheme(theme: string): "light" | "dark" {
  if (theme === "light" || theme === "dark") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** The canvas colour a background setting produces, or null when it follows the theme. */
export function canvasFor(appearance: Appearance): string | null {
  if (appearance.background === "light") return CANVAS.light;
  if (appearance.background === "dark") return CANVAS.dark;
  if (appearance.background === "custom" && isHexColor(appearance.backgroundColor)) {
    return appearance.backgroundColor;
  }
  return null;
}

/** The colour Draw's page is painted — and exported — with. */
export function drawCanvasFor(appearance: Appearance): string {
  if (appearance.drawBackground === "custom" && isHexColor(appearance.drawBackgroundColor)) {
    return appearance.drawBackgroundColor.toLowerCase();
  }
  const mode =
    appearance.drawBackground === "light" || appearance.drawBackground === "dark"
      ? appearance.drawBackground
      : resolvedTheme(appearance.theme);
  return DRAW_CANVAS[mode];
}

/** Whichever of the two inks reads with more contrast on `hex`. */
export function inkFor(hex: string): string {
  const background = luminance(rgb(hex));
  return contrast(background, luminance(INK_DARK)) >= contrast(background, luminance(INK_LIGHT))
    ? "#1d1d1f"
    : "#f5f5f7";
}

/**
 * Paint a window. `background: false` keeps the theme's own canvas, for a
 * surface that must not take the chosen background.
 */
export function applyAppearance(appearance: Appearance, options: { background?: boolean } = {}) {
  const root = document.documentElement;
  root.dataset.theme = resolvedTheme(appearance.theme);

  const canvas = options.background === false ? null : canvasFor(appearance);
  if (!canvas) {
    for (const property of CANVAS_PROPERTIES) root.style.removeProperty(property);
    return;
  }
  const background = luminance(rgb(canvas));
  const ink =
    contrast(background, luminance(INK_DARK)) >= contrast(background, luminance(INK_LIGHT))
      ? INK_DARK
      : INK_LIGHT;
  const tone = ink.join(", ");
  root.style.setProperty("--canvas", canvas);
  root.style.setProperty("--on-canvas", `rgb(${tone})`);
  root.style.setProperty("--on-canvas-2", `rgba(${tone}, 0.72)`);
  root.style.setProperty("--on-canvas-3", `rgba(${tone}, 0.6)`);
  root.style.setProperty("--canvas-rule", `rgba(${tone}, 0.12)`);
  root.style.setProperty("--canvas-hover", `rgba(${tone}, 0.08)`);
}
