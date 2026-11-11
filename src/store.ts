import { invoke } from "@tauri-apps/api/core";
import type { Appearance, BackgroundMode } from "./appearance";
import type {
  AppState,
  BackupSummary,
  ModelInfo,
  ModelProgress,
  CrashReport,
  RestoreOutcome,
  SortOrder,
  UpdateInfo,
  Dock,
  FeedbackChannels,
  ShortcutState,
  TranscriptOptions,
} from "./types";

export const api = {
  getState: () => invoke<AppState>("get_state"),
  configureDesktop: (feature: "shortcuts" | "edit-shortcuts") => invoke<void>("configure_desktop", { feature }),
  getSettingStr: (key: string) => invoke<string>("get_setting_str", { key }),
  addItem: (content: string) => invoke<void>("add_item", { content }),
  openDraw: () => invoke<void>("open_draw"),
  openSettings: () => invoke<void>("open_settings"),
  toggleCompanion: (label: "draw" | "settings") => invoke<void>("toggle_companion", { label }),
  setSort: (order: SortOrder) => invoke<void>("set_sort", { order }),
  addImage: (dataBase64: string, ext: string) =>
    invoke<number>("add_image", { dataBase64, ext }),
  deleteItem: (id: number) => invoke<void>("delete_item", { id }),
  updateItem: (id: number, content: string) =>
    invoke<void>("update_item", { id, content }),
  setFavorite: (id: number, favorite: boolean) =>
    invoke<void>("set_favorite", { id, favorite }),
  setPinned: (id: number, pinned: boolean) =>
    invoke<void>("set_pinned", { id, pinned }),
  clearHistory: () => invoke<void>("clear_history"),
  clearCaptures: () => invoke<void>("clear_captures"),
  deleteAllData: () => invoke<void>("delete_all_data"),
  copyItem: (id: number) => invoke<unknown>("copy_item", { id }),
  itemImageData: (id: number) => invoke<string | null>("item_image_data", { id }),
  /** A card's cover: an asset URL for the file, not the file in base64. */
  itemCoverUrl: (id: number) => invoke<string | null>("item_cover_url", { id }),
  exportMarkdown: () => invoke<string>("export_markdown"),
  setExportDir: (path: string) => invoke<void>("set_export_dir", { path }),
  getExportDir: () => invoke<string>("get_export_dir"),
  setTheme: (theme: string) => invoke<void>("set_theme", { theme }),
  setOnboardingCompleted: (done: boolean) =>
    invoke<void>("set_onboarding_completed", { done }),
  setAutostart: (enabled: boolean) => invoke<unknown>("set_autostart", { enabled }),
  openStartupSettings: () => invoke<void>("open_startup_settings"),
  openLink: (url: string) => invoke<void>("open_link", { url }),
  getAppearance: () => invoke<Appearance>("get_appearance"),
  setDrawBackground: (mode: BackgroundMode, color?: string) =>
    invoke<void>("set_draw_background", { mode, color: color ?? null }),

  // Draw (v2.1). The Rust lane owns these commands; the names and argument
  // keys here are the merge contract. `get_appearance` returns the saved
  // drawTool / drawColor / drawWidth alongside the page settings.
  /** The SVG goes to the native Save-As dialog; null means it was dismissed. */
  saveSvg: (svg: string, suggestedName: string) =>
    invoke<string | null>("save_svg", { svg, suggestedName }),
  /** Open Draw with an image item already on the page; Rust emits `draw-seed`. */
  openDrawWithImage: (itemId: number) => invoke<void>("open_draw_with_image", { itemId }),
  /** Remember the tool, colour and width Draw opens with. */
  setDrawPrefs: (tool: string, color: string, width: number) =>
    invoke<void>("set_draw_prefs", { tool, color, width }),
  /**
   * The Draw clipboard-image read. The v2.1 contract names no Rust route for
   * clipboard pixels — none of send_drawing, save_svg, open_draw_with_image or
   * set_draw_prefs carries one — so this is the single seam that waits for the
   * lane that owns Rust. It is expected to answer an image data URL, or null
   * when the clipboard holds no picture.
   */
  clipboardImage: () => invoke<string | null>("clipboard_image_data"),

  setBackground: (mode: BackgroundMode, color?: string) =>
    invoke<void>("set_background", { mode, color: color ?? null }),
  showPanel: () => invoke<void>("show_panel"),
  hidePanel: () => invoke<void>("hide_panel"),

  // Packs: named groups of kept items, pasted together as one block of text.
  // Rust refuses with a sentence written for a person, so callers surface the
  // rejection message rather than a code.
  setRetentionDays: (origin: "clipboard" | "dictation", days: number | null) =>
    invoke<void>("set_retention_days", { origin, days }),
  getRetentionDays: (origin: "clipboard" | "dictation") =>
    invoke<number | null>("get_retention_days", { origin }),
  pruneHistoryNow: () => invoke<number>("prune_history_now"),

  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  installUpdate: () => invoke<void>("install_update"),

  // Model inventory and lifecycle.
  getModels: () => invoke<ModelInfo[]>("get_models"),
  setModel: (id: string) => invoke<void>("set_model", { id }),
  downloadModel: (id: string) => invoke<unknown>("download_model", { id }),
  downloadModelCancel: () => invoke<void>("download_model_cancel"),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),

  // Hugging Face import.

  // Legacy custom-model commands remain for surfaces outside the model manager.

  // Optional cloud post-processing. The API key is accepted only by save and
  // is never returned in application state or emitted in an event.
  getCrashReport: () => invoke<CrashReport | null>("get_crash_report"),
  /** Versions, storage state, the latest crash report and the lifecycle tail,
   * copied to the clipboard. Local only; nothing leaves the machine. */
  copyDiagnosticReport: () => invoke<void>("copy_diagnostic_report"),
  /** Write a backup folder under the chosen destination. */
  backupNow: (destination: string) =>
    invoke<BackupSummary>("backup_now", { destination }),
  /**
   * Replace the database and images from a backup folder. The first call
   * answers `needsConfirmation` and changes nothing; call again with
   * `confirmed` to proceed.
   */
  restoreFromBackup: (source: string, confirmed: boolean) =>
    invoke<RestoreOutcome>("restore_from_backup", { source, confirmed }),

  getMicrophones: () => invoke<string[]>("get_microphones"),
  setMicrophone: (name: string) => invoke<void>("set_microphone", { name }),

  // v3: dock, feedback channels, capture pause and global shortcuts.
  setDock: (dock: Dock) => invoke<void>("set_dock", { dock }),
  getStayOpen: () => invoke<boolean>("get_stay_open"),
  setStayOpen: (stay: boolean) => invoke<boolean>("set_stay_open", { stay }),
  holdWindows: (hold: boolean) => invoke<void>("hold_windows", { hold }),
  getFeedback: () => invoke<FeedbackChannels>("get_feedback"),
  setFeedback: (pill: boolean, sound: boolean) =>
    invoke<FeedbackChannels>("set_feedback", { pill, sound }),
  setCapturePaused: (paused: boolean) => invoke<boolean>("set_capture_paused", { paused }),
  /** Show an error in the status pill (and sound it, if sounds are on). */
  notifyError: (text: string) => invoke<void>("notify_error", { text }),
  /** The choices that shape the finished transcript. */
  getDictationOptions: () => invoke<TranscriptOptions>("get_dictation_options"),
  setDictationOptions: (options: TranscriptOptions) =>
    invoke<void>("set_dictation_options", { options }),

  getShortcuts: () => invoke<ShortcutState>("get_shortcuts"),
  setShortcut: (id: string, chord: string) =>
    invoke<ShortcutState>("set_shortcut", { id, chord }),
  resetShortcuts: () => invoke<ShortcutState>("reset_shortcuts"),
};

export type { ModelProgress };
