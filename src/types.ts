export interface Item {
  id: number;
  content: string;
  kind: "text" | "image";
  imagePath: string | null;
  favorite: number;
  createdAt: number;
  updatedAt: number;
}

export interface ModelInfo {
  id: string;
  name: string;
  sizeMb: number;
  downloaded: boolean;
}

export interface ModelStatus {
  id: string;
  name: string;
  sizeMb: number;
  downloaded: boolean;
}

export interface AppState {
  items: Item[];
  theme: string;
  appVersion: string;
  keybinds: string;
  onboardingCompleted: boolean;
  autostartEnabled: boolean;
  captureDoubleShift: boolean;
  modelId: string;
  modelReady: boolean;
  providerConfigured: boolean;
  micReady: boolean;
}

export interface ModelProgress {
  id: string;
  received: number;
  total: number;
  percent: number;
}