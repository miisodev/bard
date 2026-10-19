import { invoke } from "@tauri-apps/api/core";
import type { Appearance, BackgroundMode } from "./appearance";
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
  PackDetail,
  PackSummary,
  PostprocessingSettings,
  PostprocessingTestResponse,
  SavePostprocessingInput,
  LicenseStatus,
  ShelfEdge,
  SortOrder,
  UpdateInfo,
} from "./types";

export const api = {
  getState: () => invoke<AppState>("get_state"),
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
  setDone: (id: number, done: boolean) => invoke<void>("set_done", { id, done }),
  clearCompleted: () => invoke<void>("clear_completed"),
  clearHistory: () => invoke<void>("clear_history"),
  clearCaptures: () => invoke<void>("clear_captures"),
  deleteAllData: () => invoke<void>("delete_all_data"),
  copyText: (text: string) => invoke<unknown>("copy_text", { text }),
  copyItem: (id: number) => invoke<unknown>("copy_item", { id }),
  /** Hide the panel, refocus the app it was opened over, and paste. */
  pasteBack: () => invoke<void>("paste_back"),
  copyAttachment: (id: number) => invoke<unknown>("copy_attachment", { id }),
  itemImageData: (id: number) => invoke<string | null>("item_image_data", { id }),
  attachmentImageData: (id: number) =>
    invoke<string | null>("attachment_image_data", { id }),
  /** A card's cover: an asset URL for the file, not the file in base64. */
  itemCoverUrl: (id: number) => invoke<string | null>("item_cover_url", { id }),
  attachmentCoverUrl: (id: number) =>
    invoke<string | null>("attachment_cover_url", { id }),
  setActiveItem: (id: number | null) => invoke<void>("set_active_item", { id }),
  /** Dock the shelf to another edge. Rust re-anchors and echoes `shelf-edge`. */
  setShelfEdge: (edge: ShelfEdge) => invoke<void>("set_shelf_edge", { edge }),
  /** The slide-out is over; Rust may hide and park the window now. */
  shelfHidden: () => invoke<void>("shelf_hidden"),
  attachImage: (itemId: number, dataBase64: string, ext: string) =>
    invoke<number>("attach_image", { itemId, dataBase64, ext }),
  detachAttachment: (id: number) => invoke<void>("detach_attachment", { id }),
  addNoteToImage: (id: number) => invoke<void>("add_note_to_image", { id }),
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
  /** Save to Bard and paste it into the app behind — Send. */
  sendDrawing: () => invoke<void>("send_drawing"),
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
  setCaptureDoubleShift: (enabled: boolean) =>
    invoke<void>("set_capture_double_shift", { enabled }),
  setSoundOnCapture: (enabled: boolean) =>
    invoke<void>("set_sound_on_capture", { enabled }),
  getSoundOnCapture: () => invoke<boolean>("get_sound_on_capture"),
  captureNow: () => invoke<void>("capture_now"),
  showPanel: () => invoke<void>("show_panel"),
  hidePanel: () => invoke<void>("hide_panel"),
  togglePanel: () => invoke<void>("toggle_panel"),

  // Packs: named groups of kept items, pasted together as one block of text.
  // Rust refuses with a sentence written for a person, so callers surface the
  // rejection message rather than a code.
  listPacks: () => invoke<PackSummary[]>("list_packs"),
  getPack: (id: number) => invoke<PackDetail>("get_pack", { id }),
  createPack: (name: string, itemIds: number[]) =>
    invoke<PackSummary>("create_pack", { name, itemIds }),
  renamePack: (id: number, name: string) => invoke<void>("rename_pack", { id, name }),
  deletePack: (id: number) => invoke<void>("delete_pack", { id }),
  packAddItems: (id: number, itemIds: number[]) =>
    invoke<void>("pack_add_items", { id, itemIds }),
  packRemoveItems: (id: number, itemIds: number[]) =>
    invoke<void>("pack_remove_items", { id, itemIds }),
  packReorder: (id: number, itemIds: number[]) =>
    invoke<void>("pack_reorder", { id, itemIds }),

  setRetentionDays: (origin: "clipboard" | "dictation", days: number | null) =>
    invoke<void>("set_retention_days", { origin, days }),
  getRetentionDays: (origin: "clipboard" | "dictation") =>
    invoke<number | null>("get_retention_days", { origin }),
  pruneHistoryNow: () => invoke<number>("prune_history_now"),

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

  // The review sheet. Every command carries the body as it stands — the edits
  // travel with the action — and Rust owns the window, so a resolved command
  // needs no reply beyond its promise.
  dictationConfirm: (text: string) => invoke<void>("dictation_confirm", { text }),
  dictationCopy: (text: string) => invoke<void>("dictation_copy", { text }),
  dictationDismiss: (text: string) => invoke<void>("dictation_dismiss", { text }),
  /** Off pastes when a take finishes; on opens the review sheet first. */
  setDictationReview: (enabled: boolean) =>
    invoke<void>("set_dictation_review", { enabled }),
};

export type { ModelProgress };
