import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { applyAppearance, type Appearance } from "./appearance";
import { api } from "./store";

/**
 * The stored appearance, kept current in any window.
 *
 * Every window paints from the same settings, so each one reads them on open
 * and then follows the `appearance` event for later changes — otherwise a theme
 * switched in Settings reaches only the window it was switched in, which is
 * exactly what happened to the panel and the drawing canvas. It reads the
 * narrow appearance record rather than the whole app state: the dictation
 * overlay paints from it too, and has no business holding the clipboard
 * history.
 *
 * It used to re-read on every `refresh` — the event a clipboard capture raises
 * — so each capture cost one invoke in each of the four windows for eight
 * strings that had not changed. The change is announced with the record
 * attached instead, and a refresh is nothing to do with the theme.
 */
export function useAppearance(): Appearance | null {
  const [appearance, setAppearance] = useState<Appearance | null>(null);
  useEffect(() => {
    let alive = true;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const stamp = (next: Appearance): Appearance => ({ ...next, systemDark: media.matches });
    const sync = () => {
      void api
        .getAppearance()
        .then((next) => {
          if (alive) setAppearance(stamp(next));
        })
        .catch(() => undefined);
    };
    sync();
    const off = listen<Appearance>("appearance", (event) => {
      if (alive) setAppearance(stamp(event.payload));
    });
    // "System" has to keep following the system, not just sample it once — and
    // the system changing scheme is not an appearance write, so the record is
    // re-stamped from what the window already holds rather than re-read.
    const restamp = () =>
      setAppearance((current) => (current ? { ...current, systemDark: media.matches } : current));
    media.addEventListener("change", restamp);
    return () => {
      alive = false;
      void off.then((fn) => fn());
      media.removeEventListener("change", restamp);
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
