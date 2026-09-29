/*
  Browser preview of every Bard window, with the Rust core mocked.

  Dev-only: each entry imports this when it runs outside Tauri, so
  `bun run dev` + http://localhost:1420/?preview renders the real components
  against fixture data. It exists for design work and for Store screenshots —
  a panel that is DWM-composited and always on top is awkward to capture, and a
  fixture is reproducible where a person's live data is neither.

  Query parameters: theme=light|dark, scene=empty|trial|restricted|recording,
  pane=<settings section>.
*/
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import catalog from "../../src-tauri/src/catalog/catalog.json";
import type { AppState, Item, ModelInfo } from "../types";
import "./preview.css";

const params = new URLSearchParams(location.search);
const scene = params.get("scene") ?? "";
const theme = params.get("theme") ?? "light";
const now = Date.now();
const minutes = (n: number) => now - n * 60_000;
/** Licence timestamps are Unix seconds. */
const secs = (ms: number) => Math.floor(ms / 1000);

let nextId = 100;

const items: Item[] = [
  text(1, "Ship the V2 release notes before Friday — keep them under 150 words.", "manual", 3),
  text(2, "https://handy.computer/docs/models", "clipboard", 8),
  text(3, "Dictation is local. The audio never leaves this computer, and Refine only ever sees the finished text.", "dictation", 14),
  text(4, "#2f6fed", "clipboard", 26),
  text(5, "ffmpeg -i take.wav -ar 16000 -ac 1 take-16k.wav", "clipboard", 41, { favorite: 1 }),
  text(6, "Call Lerato about the Cape Town launch venue", "manual", 75, { done: 1 }),
  text(7, "Remember: the three tests. Copper, Handy, and would Apple ship this.", "dictation", 180),
  text(8, "hello@miiso.dev", "clipboard", 320),
];

function text(
  id: number,
  content: string,
  origin: Item["origin"],
  age: number,
  extra: Partial<Item> = {},
): Item {
  return {
    id,
    sectionId: null,
    content,
    kind: "text",
    origin,
    favorite: 0,
    done: 0,
    createdAt: minutes(age),
    updatedAt: minutes(age),
    ...extra,
  };
}

const license: AppState["license"] =
  scene === "restricted"
    ? {
        state: "restricted",
        canUse: false,
        keyConfigured: false,
        trialStartedAt: secs(now - 15 * 864e5),
        trialExpiresAt: secs(now - 864e5),
        expiresAt: null,
        lastCheckedAt: secs(now),
        message: "Your 14-day trial has ended.",
      }
    : scene === "trial"
      ? {
          state: "trial",
          canUse: true,
          keyConfigured: false,
          trialStartedAt: secs(now - 3 * 864e5),
          trialExpiresAt: secs(now + 11 * 864e5),
          expiresAt: null,
          lastCheckedAt: secs(now),
          message: null,
        }
      : {
          state: "active",
          canUse: true,
          keyConfigured: true,
          trialStartedAt: null,
          trialExpiresAt: null,
          expiresAt: secs(now + 340 * 864e5),
          lastCheckedAt: secs(now),
          message: null,
          activationCount: 2,
          activationLimit: 3,
        };

const state: AppState = {
  sections: [
    { id: 1, name: "Inbox" },
    { id: 2, name: "Launch" },
  ],
  items: scene === "empty" ? [] : items,
  attachments: [],
  activeSectionId: null,
  theme,
  appVersion: "2.0.0",
  onboardingCompleted: scene !== "onboarding",
  autostartEnabled: true,
  captureDoubleShift: true,
  modelId: "whisper-small",
  modelReady: true,
  micReady: true,
  activeItemId: null,
  license,
  channel: "direct",
  background: "theme",
  backgroundColor: "#e6e7ea",
  sort: "newest",
};

interface CatalogRow {
  id: string;
  name: string;
  description: string;
  role?: string;
  architecture: string;
  family?: string;
  parameters?: string;
  languages: number;
  capabilities: ModelInfo["capabilities"];
  accuracy: number;
  speed: number;
  value?: number;
  recommended: boolean;
  recommendedRank?: number;
  file: { sizeBytes: number; quant?: string };
  source?: ModelInfo["source"];
  host?: ModelInfo["host"];
}

const models: ModelInfo[] = (catalog.models as CatalogRow[]).map((row) => ({
  ...row,
  sizeMb: Math.round(row.file.sizeBytes / 1_048_576),
  sizeBytes: row.file.sizeBytes,
  quantization: row.file.quant,
  downloaded: row.id === "whisper-small",
  active: row.id === "whisper-small",
  compatibility: row.id === "whisper-small" ? "compatible" : "unknown",
  catalog: true,
  hosted: true,
}));

const settings: Record<string, string> = {
  dictation_overlay_position: "bottom-middle",
  microphone: "",
};

function handle(cmd: string, args: Record<string, unknown> = {}): unknown {
  switch (cmd) {
    case "get_state":
      return structuredClone(state);
    case "get_appearance":
      return {
        theme,
        background: "theme",
        backgroundColor: "#e6e7ea",
        drawBackground: "theme",
        drawBackgroundColor: "#ffffff",
      };
    case "get_license":
    case "refresh_license":
      return license;
    case "get_models":
      return models;
    case "get_model":
      return {
        id: "whisper-small",
        name: "Bard Fast",
        sizeMb: 202,
        sizeBytes: 212107328,
        downloaded: true,
        architecture: "whisper",
        compatibility: "compatible",
        localPath: "C:\\Users\\you\\AppData\\Local\\com.miisodev.bard\\models\\whisper-small-Q6_K.gguf",
        owned: true,
      };
    case "get_detected_models":
    case "hf_cached_models":
    case "get_custom_models":
      return [];
    case "get_postprocessing":
      return { enabled: false, keyConfigured: false, modelIds: [] };
    case "get_microphones":
      return ["Default microphone", "Studio USB Mic"];
    case "get_setting_str":
      return settings[String(args.key)] ?? "";
    case "get_retention_days":
      return args.origin === "dictation" ? 30 : 90;
    case "get_export_dir":
      return "C:\\Users\\you\\Documents\\Bard";
    case "get_sound_on_capture":
      return true;
    case "get_crash_report":
    case "check_for_update":
      return null;
    case "add_item": {
      state.items.unshift(text(nextId++, String(args.content), "manual", 0));
      return null;
    }
    case "set_done": {
      const item = state.items.find((i) => i.id === args.id);
      if (item) item.done = args.done ? 1 : 0;
      return null;
    }
    case "set_favorite": {
      const item = state.items.find((i) => i.id === args.id);
      if (item) item.favorite = args.favorite ? 1 : 0;
      return null;
    }
    case "delete_item":
      state.items = state.items.filter((i) => i.id !== args.id);
      return null;
    case "set_sort":
      state.sort = args.order as AppState["sort"];
      return null;
    case "set_active_item":
      state.activeItemId = (args.id as number | null) ?? null;
      return null;
    default:
      return null;
  }
}

export function install(label: "main" | "settings" | "draw" | "overlay") {
  mockWindows(label, "main", "settings", "draw", "overlay");
  mockIPC((cmd, payload) => handle(cmd, (payload ?? {}) as Record<string, unknown>), {
    shouldMockEvents: true,
  });
  document.documentElement.dataset.theme = theme;
  document.documentElement.dataset.preview = label;
}
