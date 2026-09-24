import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { applyAppearance, type Appearance } from "./appearance";
import { api } from "./store";

/**
 * The stored appearance, kept current in any window.
 *
 * Every window paints from the same settings, so each one reads them on open
 * *and* follows later changes — otherwise a theme switched in Settings reaches
 * only the window it was switched in, which is exactly what happened to the
 * panel and the drawing canvas. It reads the narrow appearance record rather
 * than the whole app state: the dictation overlay paints from it too, and has
 * no business holding the clipboard history.
 */
export function useAppearance(): Appearance | null {
  const [appearance, setAppearance] = useState<Appearance | null>(null);
  useEffect(() => {
    let alive = true;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const sync = () => {
      void api
        .getAppearance()
        .then((next) => {
          if (alive) setAppearance({ ...next, systemDark: media.matches });
        })
        .catch(() => undefined);
    };
    sync();
    const off = listen("refresh", sync);
    // "System" has to keep following the system, not just sample it once.
    media.addEventListener("change", sync);
    return () => {
      alive = false;
      void off.then((fn) => fn());
      media.removeEventListener("change", sync);
    };
  }, []);
  return appearance;
}

/** Follow the theme — and, unless told otherwise, the panel background. */
export function useTheme(options: { background?: boolean } = {}): Appearance | null {
  const appearance = useAppearance();
  const background = options.background !== false;
  useEffect(() => {
    if (appearance) applyAppearance(appearance, { background });
  }, [appearance, background]);
  return appearance;
}
