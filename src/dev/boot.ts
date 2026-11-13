/**
 * Installs the browser preview's mocked core when a window runs outside Tauri
 * in development. A production build never contains the preview: the branch is
 * dead code there and Vite drops the import with it.
 */
export async function boot(label: "main" | "settings" | "draw" | "pill" | "guide"): Promise<void> {
  // The styles need to know whose window frame they are drawn in: Windows
  // rounds and shadows the window itself, Linux leaves both to the page.
  const asked = new URLSearchParams(window.location.search).get("frame");
  document.documentElement.dataset.platform =
    asked ?? (/Windows/.test(navigator.userAgent) ? "windows" : "linux");
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    (await import("./preview")).install(label);
  }
}
