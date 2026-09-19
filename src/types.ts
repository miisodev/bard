export interface Section {
  id: number;
  name: string;
}

export interface Item {
  id: number;
  sectionId: number | null;
  content: string;
  kind: "text" | "image";
  imagePath: string | null;
  favorite: number;
  done: number;
  createdAt: number;
  updatedAt: number;
}

export interface Attachment {
  id: number;
  itemId: number;
  path: string;
  position: number;
}

export type Architecture =
  | "whisper"
  | "parakeet"
  | "moonshine"
  | "canary"
  | "sensevoice"
  | "gigaam"
  | "cohere";

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
  architecture: Architecture;
  /** 0 for a model installed from Hugging Face, which carries no curation. */
  languages: number;
  capabilities: ModelCapabilities;
  /** 0-100, relative to the other catalog entries rather than a benchmark. */
  accuracy: number;
  speed: number;
  recommended: boolean;
}

export interface ModelStatus {
  id: string;
  name: string;
  sizeMb: number;
  downloaded: boolean;
}

export interface AppState {
  sections: Section[];
  items: Item[];
  attachments: Attachment[];
  activeSectionId: number | null;
  theme: string;
  appVersion: string;
  keybinds: string;
  onboardingCompleted: boolean;
  autostartEnabled: boolean;
  captureDoubleShift: boolean;
  modelId: string;
  modelReady: boolean;
  micReady: boolean;
}

export interface ModelProgress {
  id: string;
  received: number;
  total: number;
  percent: number;
}

export interface HfFile {
  path: string;
  size: number;
  sha256: string;
}

export interface CachedHfModel {
  repo: string;
  file: string;
  size: number;
  localPath: string;
}