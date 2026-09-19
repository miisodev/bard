import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { applyTheme } from "./App";
import { api } from "./store";

/**
 * Follow the app's theme, in any window.
 *
 * Every window paints from the same stored setting, so each one has to read it
 * on open *and* listen for later changes — otherwise a theme switched in
 * settings reaches only the window it was switched in, which is exactly what
 * happened to the panel and the drawing canvas.
 */
export function useTheme() {
  useEffect(() => {
    let alive = true;
    const sync = () => {
      void api.getState().then((s) => {
        if (alive) applyTheme(s.theme);
      });
    };
    sync();
    const off = listen("refresh", sync);

    // "System" has to keep following the system, not just sample it once.
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", sync);

    return () => {
      alive = false;
      void off.then((fn) => fn());
      media.removeEventListener("change", sync);
    };
  }, []);
}
