import { invoke } from "@tauri-apps/api/core";
import type { AppState, CachedHfModel, HfFile, ModelInfo, ModelProgress, ModelStatus } from "./types";

export const api = {
  getState: () => invoke<AppState>("get_state"),
  getSettingStr: (key: string) => invoke<string>("get_setting_str", { key }),
  addItem: (content: string) => invoke<void>("add_item", { content }),
  addImage: (dataBase64: string, ext: string) =>
    invoke<void>("add_image", { dataBase64, ext }),
  addFiles: (paths: string[]) => invoke<number>("add_files", { paths }),
  deleteItem: (id: number) => invoke<void>("delete_item", { id }),
  updateItem: (id: number, content: string) =>
    invoke<void>("update_item", { id, content }),
  setFavorite: (id: number, favorite: boolean) =>
    invoke<void>("set_favorite", { id, favorite }),
  clearHistory: () => invoke<void>("clear_history"),
  copyText: (text: string) => invoke<unknown>("copy_text", { text }),
  copyItem: (id: number) => invoke<unknown>("copy_item", { id }),
  imageData: (path: string) => invoke<string | null>("image_data", { path }),
  createSection: (name: string) => invoke<number>("create_section", { name }),
  setActiveSection: (id: number | null) =>
    invoke<void>("set_active_section", { id }),
  renameSection: (id: number, name: string) =>
    invoke<void>("rename_section", { id, name }),
  deleteSection: (id: number) => invoke<void>("delete_section", { id }),
  attachImage: (itemId: number, dataBase64: string, ext: string) =>
    invoke<number>("attach_image", { itemId, dataBase64, ext }),
  detachAttachment: (id: number) => invoke<void>("detach_attachment", { id }),
  addNoteToImage: (id: number) => invoke<void>("add_note_to_image", { id }),
  exportMarkdown: () => invoke<string>("export_markdown"),
  setExportDir: (path: string) => invoke<void>("set_export_dir", { path }),
  getExportDir: () => invoke<string>("get_export_dir"),
  setTheme: (theme: string) => invoke<void>("set_theme", { theme }),
  setKeybinds: (json: string) => invoke<void>("set_keybinds", { json }),
  setOnboardingCompleted: (done: boolean) =>
    invoke<void>("set_onboarding_completed", { done }),
  setAutostart: (enabled: boolean) => invoke<unknown>("set_autostart", { enabled }),
  setCaptureDoubleShift: (enabled: boolean) =>
    invoke<void>("set_capture_double_shift", { enabled }),
  setSoundOnCapture: (enabled: boolean) =>
    invoke<void>("set_sound_on_capture", { enabled }),
  getSoundOnCapture: () => invoke<boolean>("get_sound_on_capture"),
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
  downloadModelCancel: () => invoke<void>("download_model_cancel"),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  getMicrophones: () => invoke<string[]>("get_microphones"),
  setMicrophone: (name: string) => invoke<void>("set_microphone", { name }),
  hfCachedModels: () => invoke<CachedHfModel[]>("hf_cached_models"),
  hfListRepo: (repo: string) => invoke<HfFile[]>("hf_list_repo", { repo }),
  hfInstall: (repo: string, file: string, size: number, sha256: string) =>
    invoke<unknown>("hf_install", { repo, file, size, sha256 }),
  hfUseCached: (repo: string, file: string, localPath: string, size: number) =>
    invoke<void>("hf_use_cached", { repo, file, localPath, size }),
  removeCustomModel: (id: string) => invoke<void>("remove_custom_model", { id }),
  dictateStart: () => invoke<void>("dictate_start"),
  dictateStop: () => invoke<void>("dictate_stop"),
  dictateCancel: () => invoke<void>("dictate_cancel"),
};

export const DEFAULT_KEYBINDS: Record<string, string> = {
  capture: "Ctrl+Shift+C",
  panel: "Ctrl+Shift+Space",
  dictate: "Ctrl+Space",
};

let shortcutRecording = false;
export function setShortcutRecording(v: boolean) {
  shortcutRecording = v;
}
export function isShortcutRecording() {
  return shortcutRecording;
}

export type { ModelProgress };