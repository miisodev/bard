export interface Section {
  id: number;
  name: string;
}

export interface Item {
  id: number;
  sectionId: number | null;
  content: string;
  kind: "text" | "image";
  origin: "manual" | "clipboard" | "dictation";
  favorite: number;
  done: number;
  createdAt: number;
  updatedAt: number;
}

export interface Attachment {
  id: number;
  itemId: number;
  position: number;
}

export type Architecture =
  | "whisper"
  | "parakeet"
  | "moonshine"
  | "canary"
  | "sensevoice"
  | "gigaam"
  | "cohere"
  | "qwen3_asr";

export type ModelCompatibility =
  | "unknown"
  | "checking"
  | "compatible"
  | "incompatible"
  | "missing";

/** Compatibility values emitted by early Rust builds before the UI vocabulary landed. */
export type BackendModelCompatibility = ModelCompatibility | "unverified" | "verified";

export interface ModelCapabilities {
  streaming: boolean;
  translate: boolean;
  languageDetect: boolean;
}

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  sizeMb: number;
  downloaded: boolean;
  architecture: Architecture | string;
  /** 0 for a model installed from Hugging Face, which carries no curation. */
  languages: number;
  capabilities: ModelCapabilities;
  /** 0-100, relative to the other Bard models rather than a benchmark. */
  accuracy: number;
  speed: number;
  recommended: boolean;
  role?: string;
  origin?: string;
  hosted?: boolean;
  hostName?: string;
  parameters?: string;
  quantization?: string;
  /** Early Rust field name retained as an optional compatibility alias. */
  quant?: string;
  license?: string;
  compatibility?: BackendModelCompatibility;
  compatibilityMessage?: string;
  sourceRepo?: string;
  sourceRevision?: string;
  sizeBytes?: number;
  localPath?: string;
  active?: boolean;
  family?: string;
  value?: number;
  recommendedRank?: number;
  owned?: boolean;
  catalog?: boolean;
  source?: {
    repo: string;
    revision: string;
    url?: string | null;
    upstreamRepo?: string | null;
    upstreamRevision?: string | null;
    upstreamUrl?: string | null;
    license?: { name: string; url: string } | null;
  };
  host?: {
    provider?: string;
    repository?: string;
    releaseTag?: string | null;
    asset?: string;
    url?: string;
    status?: string;
  } | null;
}

export interface DetectedModel {
  id: string;
  name?: string;
  repo: string;
  file: string;
  size: number;
  localPath: string;
  parameters?: string;
  quantization?: string;
  quant?: string;
  sourceRevision?: string;
  compatibility?: BackendModelCompatibility;
  compatibilityMessage?: string;
  registered?: boolean;
  owned?: boolean;
}

export interface ModelStatus {
  id: string;
  name: string;
  sizeMb: number;
  sizeBytes: number;
  downloaded: boolean;
  architecture: Architecture;
  role?: string | null;
  compatibility: BackendModelCompatibility;
  localPath: string | null;
  owned: boolean;
}

export type OverlayVertical = "top" | "bottom";
export type OverlayHorizontal = "left" | "middle" | "right";
export type OverlayPosition = `${OverlayVertical}-${OverlayHorizontal}`;

/** The timeline's order; newest first is the default. */
export type SortOrder = "newest" | "oldest" | "favorites" | "completed";

export interface AppState {
  sections: Section[];
  items: Item[];
  attachments: Attachment[];
  activeSectionId: number | null;
  theme: string;
  appVersion: string;
  onboardingCompleted: boolean;
  autostartEnabled: boolean;
  captureDoubleShift: boolean;
  modelId: string;
  modelReady: boolean;
  micReady: boolean;
  activeItemId: number | null;
  license: LicenseStatus;
  /** "direct" for the miiso.dev download, "store" for the Microsoft Store copy. */
  channel: "direct" | "store";
  /** "theme" follows Light/Dark; "light", "dark" and "custom" are chosen. */
  background: "theme" | "light" | "dark" | "custom";
  backgroundColor: string;
  sort: SortOrder;
}

export interface LicenseStatus {
  state: "trial" | "active" | "restricted" | "unavailable" | string;
  canUse: boolean;
  keyConfigured: boolean;
  trialStartedAt: number | null;
  trialExpiresAt: number | null;
  expiresAt: number | null;
  lastCheckedAt: number | null;
  message: string | null;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
  publishedAt: string | null;
}

export interface ModelProgress {
  id: string;
  received: number;
  total: number;
  percent: number;
  phase?: string;
  message?: string;
  speedBytesPerSecond?: number;
  etaSeconds?: number;
  error?: string;
}

export interface HfFile {
  path: string;
  size: number;
  sha256: string;
  quant?: string;
  sourceRepo?: string;
  sourceRevision?: string;
  compatibility?: BackendModelCompatibility;
  compatibilityMessage?: string;
}

export interface CachedHfModel {
  id?: string;
  repo: string;
  file: string;
  size: number;
  localPath: string;
  quant?: string;
  compatibility?: BackendModelCompatibility;
  registered?: boolean;
  owned?: boolean;
}

export interface ModelCompatibilityResult {
  path?: string;
  status?: BackendModelCompatibility;
  message?: string;
  compatibility?: BackendModelCompatibility;
  compatibilityMessage?: string;
}

export type HfInstallResult = string | { id: string } | null;

export interface PostprocessingSettings {
  enabled: boolean;
  modelId?: string;
  modelIds?: string[];
  modelName?: string;
  keyConfigured?: boolean;
  /** Compatibility aliases used by early backend status shapes. */
  apiKeyConfigured?: boolean;
  hasApiKey?: boolean;
  hasValidModel?: boolean;
  ready?: boolean;
  statusMessage?: string;
}

export interface CrashReport {
  path: string;
  contents: string;
}

export interface SavePostprocessingInput {
  enabled: boolean;
  modelId?: string;
  modelIds?: string[];
  /** Sent only while saving. It is never returned or placed in AppState. */
  apiKey?: string;
  clearKey?: boolean;
}

export interface PostprocessingTestResult {
  ok: boolean;
  message?: string;
  modelName?: string;
}

export type PostprocessingTestResponse = boolean | PostprocessingTestResult;
