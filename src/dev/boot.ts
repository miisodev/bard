/**
 * Installs the browser preview's mocked core when a window runs outside Tauri
 * in development. A production build never contains the preview: the branch is
 * dead code there and Vite drops the import with it.
 */
export async function boot(label: "main" | "settings" | "draw" | "overlay"): Promise<void> {
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    (await import("./preview")).install(label);
  }
}
