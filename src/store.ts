import { invoke } from "@tauri-apps/api/core";
import type { AppState, Item, ModelInfo, ModelProgress, ModelStatus } from "./types";

export const api = {
  getState: () => invoke<AppState>("get_state"),
  addItem: (content: string) => invoke<void>("add_item", { content }),
  addImage: (dataBase64: string, ext: string) =>
    invoke<void>("add_image", { dataBase64, ext }),
  deleteItem: (id: number) => invoke<void>("delete_item", { id }),
  setFavorite: (id: number, favorite: boolean) =>
    invoke<void>("set_favorite", { id, favorite }),
  clearHistory: () => invoke<void>("clear_history"),
  copyText: (text: string) => invoke<unknown>("copy_text", { text }),
  copyItem: (id: number) => invoke<unknown>("copy_item", { id }),
  imageData: (path: string) => invoke<string | null>("image_data", { path }),
  setTheme: (theme: string) => invoke<void>("set_theme", { theme }),
  setKeybinds: (json: string) => invoke<void>("set_keybinds", { json }),
  setOnboardingCompleted: (done: boolean) =>
    invoke<void>("set_onboarding_completed", { done }),
  setAutostart: (enabled: boolean) => invoke<unknown>("set_autostart", { enabled }),
  setCaptureDoubleShift: (enabled: boolean) =>
    invoke<void>("set_capture_double_shift", { enabled }),
  captureNow: () => invoke<void>("capture_now"),
  saveWindowGeometry: (x: number, y: number, width: number, height: number) =>
    invoke<void>("save_window_geometry", { x, y, width, height }),
  minimizePanel: () => invoke<void>("minimize_panel"),
  showPanel: () => invoke<void>("show_panel"),
  hidePanel: () => invoke<void>("hide_panel"),
  togglePanel: () => invoke<void>("toggle_panel"),
  getModels: () => invoke<ModelInfo[]>("get_models"),
  getModel: () => invoke<ModelStatus>("get_model"),
  setModel: (id: string) => invoke<void>("set_model", { id }),
  downloadModel: (id: string) => invoke<unknown>("download_model", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  dictateStart: () => invoke<void>("dictate_start"),
  dictateStop: () => invoke<void>("dictate_stop"),
  dictateCancel: () => invoke<void>("dictate_cancel"),
  setProvider: (baseUrl: string, apiKey: string, model: string) =>
    invoke<void>("set_provider", { baseUrl, apiKey, model }),
  clearProvider: () => invoke<void>("clear_provider"),
  ask: (prompt: string, contextItemIds: number[]) =>
    invoke<string>("ask", { prompt, contextItemIds }),
};

export interface PersistedProvider {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export async function getProvider(): Promise<PersistedProvider> {
  const [baseUrl, apiKey, model] = await Promise.all([
    invoke<string>("get_setting_str", { key: "provider_base_url" }),
    invoke<string>("get_setting_str", { key: "provider_api_key" }),
    invoke<string>("get_setting_str", { key: "provider_model" }),
  ]);
  return { baseUrl, apiKey, model };
}

export type { Item, ModelProgress };