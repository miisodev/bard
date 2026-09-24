import { invoke } from "@tauri-apps/api/core";
import type {
  AppState,
  CachedHfModel,
  DetectedModel,
  HfFile,
  HfInstallResult,
  ModelCompatibility,
  ModelCompatibilityResult,
  ModelInfo,
  ModelProgress,
  ModelStatus,
  OverlayPosition,
  CrashReport,
  PostprocessingSettings,
  PostprocessingTestResponse,
  SavePostprocessingInput,
  LicenseStatus,
  UpdateInfo,
} from "./types";

export const api = {
  getState: () => invoke<AppState>("get_state"),
  getSettingStr: (key: string) => invoke<string>("get_setting_str", { key }),
  addItem: (content: string) => invoke<void>("add_item", { content }),
  openDraw: () => invoke<void>("open_draw"),
  toggleDraw: () => invoke<void>("toggle_draw"),
  openSettings: () => invoke<void>("open_settings"),
  toggleSettings: () => invoke<void>("toggle_settings"),
  addImage: (dataBase64: string, ext: string) =>
    invoke<number>("add_image", { dataBase64, ext }),
  deleteItem: (id: number) => invoke<void>("delete_item", { id }),
  updateItem: (id: number, content: string) =>
    invoke<void>("update_item", { id, content }),
  setFavorite: (id: number, favorite: boolean) =>
    invoke<void>("set_favorite", { id, favorite }),
  setDone: (id: number, done: boolean) => invoke<void>("set_done", { id, done }),
  clearCompleted: () => invoke<void>("clear_completed"),
  clearHistory: () => invoke<void>("clear_history"),
  clearCaptures: () => invoke<void>("clear_captures"),
  deleteAllData: () => invoke<void>("delete_all_data"),
  copyText: (text: string) => invoke<unknown>("copy_text", { text }),
  copyItem: (id: number) => invoke<unknown>("copy_item", { id }),
  copyAttachment: (id: number) => invoke<unknown>("copy_attachment", { id }),
  itemImageData: (id: number) => invoke<string | null>("item_image_data", { id }),
  attachmentImageData: (id: number) =>
    invoke<string | null>("attachment_image_data", { id }),
  setActiveItem: (id: number | null) => invoke<void>("set_active_item", { id }),
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
  setShortcutRecording: (active: boolean) =>
    invoke<void>("set_shortcut_recording", { active }),
  setOnboardingCompleted: (done: boolean) =>
    invoke<void>("set_onboarding_completed", { done }),
  setAutostart: (enabled: boolean) => invoke<unknown>("set_autostart", { enabled }),
  openStartupSettings: () => invoke<void>("open_startup_settings"),
  openLink: (url: string) => invoke<void>("open_link", { url }),
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

  setRetentionDays: (origin: "clipboard" | "dictation", days: number | null) =>
    invoke<void>("set_retention_days", { origin, days }),
  getRetentionDays: (origin: "clipboard" | "dictation") =>
    invoke<number | null>("get_retention_days", { origin }),
  pruneHistoryNow: () => invoke<void>("prune_history_now"),

  getLicense: () => invoke<LicenseStatus>("get_license"),
  startTrial: () => invoke<LicenseStatus>("start_trial"),
  activateLicense: (key: string) =>
    invoke<LicenseStatus>("activate_license", { key }),
  refreshLicense: () => invoke<LicenseStatus>("refresh_license"),
  deactivateLicense: () => invoke<LicenseStatus>("deactivate_license"),
  clearLicense: () => invoke<LicenseStatus>("clear_license"),
  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  installUpdate: () => invoke<void>("install_update"),

  // Model inventory and lifecycle.
  getModels: () => invoke<ModelInfo[]>("get_models"),
  getModel: () => invoke<ModelStatus>("get_model"),
  getDetectedModels: () => invoke<DetectedModel[]>("get_detected_models"),
  checkModel: (localPath: string) =>
    invoke<ModelCompatibility | ModelCompatibilityResult>("check_model", { localPath }),
  activateDetectedModel: (id: string) =>
    invoke<void>("activate_detected_model", { id }),
  forgetDetectedModel: (id: string) =>
    invoke<void>("forget_detected_model", { id }),
  setModel: (id: string) => invoke<void>("set_model", { id }),
  downloadModel: (id: string) => invoke<unknown>("download_model", { id }),
  downloadModelCancel: () => invoke<void>("download_model_cancel"),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),

  // Hugging Face import.
  hfListRepo: (repo: string) => invoke<HfFile[]>("hf_list_repo", { repo }),
  hfInstall: (repo: string, file: string, size: number, sha256: string) =>
    invoke<HfInstallResult>("hf_install", { repo, file, size, sha256 }),
  hfUseCached: (repo: string, file: string, localPath: string, size: number) =>
    invoke<void>("hf_use_cached", { repo, file, localPath, size }),

  // Legacy custom-model commands remain for surfaces outside the model manager.
  hfCachedModels: () => invoke<CachedHfModel[]>("hf_cached_models"),
  removeCustomModel: (id: string) => invoke<void>("remove_custom_model", { id }),

  // Optional cloud post-processing. The API key is accepted only by save and
  // is never returned in application state or emitted in an event.
  getPostprocessing: () =>
    invoke<PostprocessingSettings>("get_postprocessing"),
  savePostprocessing: (input: SavePostprocessingInput) =>
    invoke<PostprocessingSettings | void>("save_postprocessing", { ...input }),
  testPostprocessing: () =>
    invoke<PostprocessingTestResponse>("test_postprocessing"),
  testPostprocessingCancel: () => invoke<void>("test_postprocessing_cancel"),
  getCrashReport: () => invoke<CrashReport | null>("get_crash_report"),

  getMicrophones: () => invoke<string[]>("get_microphones"),
  setMicrophone: (name: string) => invoke<void>("set_microphone", { name }),
  setDictationOverlayPosition: (position: OverlayPosition) =>
    invoke<void>("set_dictation_overlay_position", { position }),
  dictateStart: () => invoke<void>("dictate_start"),
  dictateStop: () => invoke<void>("dictate_stop"),
  dictateCancel: () => invoke<void>("dictate_cancel"),
};

/** Chords start unassigned: the double-tap gestures are the defaults. */
export const DEFAULT_KEYBINDS: Record<string, string> = {
  capture: "",
  panel: "",
};

let shortcutRecording = false;
export function setShortcutRecording(v: boolean) {
  shortcutRecording = v;
}
export function isShortcutRecording() {
  return shortcutRecording;
}

export type { ModelProgress };
