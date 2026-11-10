/*
  Browser preview of every Bard window, with the Rust core mocked.

  Dev-only: each entry imports this when it runs outside Tauri, so
  `bun run dev` + http://localhost:1420/?preview renders the real components
  against fixture data. It exists for design work and for Store screenshots —
  a shelf that is DWM-composited and always on top is awkward to capture, and a
  fixture is reproducible where a person's live data is neither.

  Query parameters: theme=light|dark, scene=empty|trial|restricted|onboarding
  (main), scene=copied|selected|listening|working|done|preview|error|paused
  (pill), pane=<settings section>, dock=float|left|right, platform=linux.
*/
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Shape } from "../draw/shapes";
import catalog from "../../src-tauri/src/catalog/catalog.json";
import { version as appVersion } from "../../package.json";
import type { AppState, Item, ModelInfo, PackSummary, PillNote, ShortcutState } from "../types";
import "./preview.css";

const params = new URLSearchParams(location.search);
const scene = params.get("scene") ?? "";
const theme = params.get("theme") ?? "light";
const dockParam = params.get("dock");
const dock: AppState["dock"] = dockParam === "left" || dockParam === "right" ? dockParam : "float";
const now = Date.now();
const minutes = (n: number) => now - n * 60_000;
/** Licence timestamps are Unix seconds. */
const secs = (ms: number) => Math.floor(ms / 1000);

let nextId = 100;

/**
 * A stand-in capture for the preview's image cards: a soft gradient drawn
 * inline, so the fixture needs no binary and still exercises the cover crop.
 */
function imageData(start: string, end: string, label: string): string {
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="420">` +
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">` +
    `<stop offset="0" stop-color="${start}"/><stop offset="1" stop-color="${end}"/>` +
    `</linearGradient></defs><rect width="640" height="420" fill="url(#g)"/>` +
    `<text x="32" y="382" font-family="Segoe UI" font-size="40" fill="rgba(255,255,255,0.85)">${label}</text>` +
    `</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const IMAGES = new Map<number, string>([
  [11, imageData("#f2b880", "#8a5a2a", "San Francisco")],
  [12, imageData("#8fb3ff", "#3a4fb8", "Flow sketch")],
]);

function shot(id: number, origin: Item["origin"], age: number, caption = ""): Item {
  return { ...text(id, caption, origin, age), kind: "image" };
}

/*
  Ordered so the first page shows one of every kind the shelf knows: a pinned
  command, a link, a colour, a captured image, a drawing and a dictation.
*/
const items: Item[] = [
  text(5, "ffmpeg -i take.wav -ar 16000 -ac 1 take-16k.wav", "clipboard", 41, { favorite: 1, pinnedAt: minutes(30) }),
  text(2, "https://handy.computer/docs/models", "clipboard", 8),
  text(13, "#6f4a4a", "clipboard", 60),
  shot(11, "clipboard", 2),
  shot(12, "manual", 34),
  text(3, "Dictation is local. The audio never leaves this computer, and Refine only ever sees the finished text.", "dictation", 14),
  text(1, "Ship the V2 release notes before Friday — keep them under 150 words.", "manual", 3),
  text(4, "#2f6fed", "clipboard", 26),
  text(6, "Call Lerato about the Cape Town launch venue", "manual", 75, { done: 1 }),
  text(7, "Remember: the three tests. Copper, Handy, and would Apple ship this.", "dictation", 180),
  text(8, "hello@miiso.dev", "clipboard", 320),
  text(9, "Book the Cape Town venue for the twelfth", "dictation", 400),
  text(10, "https://github.com/miisodev/bard-releases", "clipboard", 520),
  {
    ...text(14, "/home/you/Documents/Launch brief.pdf\n/home/you/Documents/Pricing.xlsx", "clipboard", 22),
    kind: "file",
  },
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
    pinnedAt: null,
    createdAt: minutes(age),
    updatedAt: minutes(age),
    ...extra,
  };
}

const license: AppState["license"] =
  scene === "onboarding"
    ? {
        state: "trial_available",
        canUse: false,
        keyConfigured: false,
        trialStartedAt: null,
        trialExpiresAt: null,
        expiresAt: null,
        lastCheckedAt: null,
        message: null,
        activationLimit: 3,
      }
    : scene === "restricted"
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
  appVersion,
  onboardingCompleted: scene !== "onboarding",
  autostartEnabled: true,
  capturePaused: scene === "paused",
  modelId: "parakeet-tdt-0.6b-v3",
  modelReady: true,
  micReady: true,
  // `active=<id>` marks an item as the one on the clipboard, for the gallery.
  activeItemId: params.has("active") ? Number(params.get("active")) : null,
  license,
  channel: params.get("channel") === "flatpak" ? "flatpak" : params.get("channel") === "store" ? "store" : "direct",
  platform: params.get("platform") ?? "windows",
  desktop: { session: params.get("platform") === "linux" ? "wayland" : "windows", shortcutsReady: params.get("platform") === "linux" },
  background: "theme",
  backgroundColor: "#e6e7ea",
  sort: "newest",
  // The pins and then the most recent, exactly as the backend publishes them:
  // the pins in the order they were pinned, then everything else newest first.
  slotIds: [
    ...items.filter((item) => item.pinnedAt !== null).sort((a, b) => a.pinnedAt! - b.pinnedAt!),
    ...items.filter((item) => item.pinnedAt === null).sort((a, b) => b.createdAt - a.createdAt),
  ]
    .slice(0, 3)
    .map((item) => item.id),
  dock,
};

const SHORTCUT_ROWS: [string, string, string][] = [
  ["main", "Show or hide Bard", "Alt+Shift+V"],
  ["dictate", "Dictate (hold to talk, tap to toggle)", "Alt+Shift+Space"],
  ["slot-1", "Copy item 1", "Alt+Shift+1"],
  ["slot-2", "Copy item 2", "Alt+Shift+2"],
  ["slot-3", "Copy item 3", "Alt+Shift+3"],
  ["cycle", "Cycle through items 1–3", "Alt+Shift+C"],
  ["preview", "Preview the clipboard", "Alt+Shift+P"],
  ["pause", "Pause or resume clipboard capture", "Alt+Shift+X"],
  ["draw", "Open Draw", "Alt+Shift+D"],
  ["settings", "Open Settings", "Alt+Shift+Comma"],
  ["guide", "Show the shortcut guide", "Alt+Shift+Slash"],
];

const chords = new Map(SHORTCUT_ROWS.map(([id, , chord]) => [id, chord]));

function shown(chord: string): string {
  return chord.replace("Comma", ",").replace("Slash", "/");
}

function shortcutState(): ShortcutState {
  const portal = params.get("platform") === "linux";
  return {
    backend: portal ? "portal" : "windows",
    editable: !portal,
    rows: SHORTCUT_ROWS.map(([id, label, defaultChord]) => {
      const chord = chords.get(id) ?? "";
      return { id, label, chord, display: shown(chord), defaultChord, status: chord ? "ready" : "off" };
    }),
  };
}

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
  downloaded: row.id === "parakeet-tdt-0.6b-v3",
  active: row.id === "parakeet-tdt-0.6b-v3",
  compatibility: row.id === "parakeet-tdt-0.6b-v3" ? "compatible" : "unknown",
  catalog: true,
  hosted: true,
}));

/*
  Fixture packs over the same items, one of each shape the zone has to carry:
  a mixed pack whose collage shows four kinds, an all-text pack that pastes,
  and a pictures-only pack the paste refuses. The scene decides whether they
  exist at all, as it does for the items.
*/
interface FixturePack {
  id: number;
  name: string;
  itemIds: number[];
}

let nextPackId = 10;

const packs: FixturePack[] =
  scene === "empty"
    ? []
    : [
        { id: 1, name: "Launch kit", itemIds: [1, 2, 11, 13, 5] },
        { id: 2, name: "Release checklist", itemIds: [5, 6, 9] },
        { id: 3, name: "Paper trail", itemIds: [11, 12] },
      ];

/** The summary Rust sends: the leading members, in pack order. */
function packSummary(pack: FixturePack): PackSummary {
  return {
    id: pack.id,
    name: pack.name,
    itemCount: pack.itemIds.length,
    previewItemIds: pack.itemIds.slice(0, 4),
  };
}

/** The members, in pack order, as `get_pack` joins them. */
function packMembers(pack: FixturePack): Item[] {
  return pack.itemIds
    .map((id) => state.items.find((item) => item.id === id))
    .filter((item): item is Item => item !== undefined);
}

/** Appends the ids that exist and are not members yet, as Rust's insert does. */
function appendToPack(pack: FixturePack, ids: number[]): number[] {
  const landed: number[] = [];
  for (const id of ids) {
    if (pack.itemIds.includes(id)) continue;
    if (!state.items.some((item) => item.id === id)) continue;
    pack.itemIds.push(id);
    landed.push(id);
  }
  return landed;
}

function requirePack(id: unknown): FixturePack {
  const pack = packs.find((entry) => entry.id === Number(id));
  if (!pack) throw "That pack no longer exists.";
  return pack;
}

const settings: Record<string, string> = {
  microphone: "",
};

const feedback = { pill: true, sound: true };

function handle(cmd: string, args: Record<string, unknown> = {}): unknown {
  if (params.get("fail") === cmd) throw "Couldn’t save. Your changes are still here. Try again.";
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
        drawTool: "pen",
        drawColor: "#1d1d1f",
        drawWidth: 4,
      };
    case "get_license":
    case "refresh_license":
      return license;
    case "start_trial":
      Object.assign(license, {
        state: "trial", canUse: true,
        trialStartedAt: secs(now), trialExpiresAt: secs(now + 14 * 864e5),
      });
      return structuredClone(license);
    case "get_models":
      return models;
    case "get_model":
      return {
        id: "parakeet-tdt-0.6b-v3",
        name: "Bard Fast",
        sizeMb: 582,
        sizeBytes: 610342240,
        downloaded: true,
        architecture: "parakeet",
        compatibility: "compatible",
        localPath: "C:\\Users\\you\\AppData\\Local\\com.miisodev.bard\\models\\parakeet-tdt-0.6b-v3-Q6_K.gguf",
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
    case "get_feedback":
      return { ...feedback };
    case "set_feedback":
      feedback.pill = Boolean(args.pill);
      feedback.sound = Boolean(args.sound);
      return { ...feedback };
    case "get_shortcuts":
      return shortcutState();
    case "set_shortcut":
      chords.set(String(args.id), String(args.chord ?? ""));
      return shortcutState();
    case "reset_shortcuts":
      for (const [id, , chord] of SHORTCUT_ROWS) chords.set(id, chord);
      return shortcutState();
    case "set_dock":
      state.dock = args.dock as AppState["dock"];
      void emit("panel-shown", state.dock);
      return null;
    case "set_capture_paused":
      state.capturePaused = Boolean(args.paused);
      void emit("refresh");
      return state.capturePaused;
    case "copy_item":
      state.activeItemId = Number(args.id);
      void emit("refresh");
      return null;
    // The v2.1 Draw commands the preview cannot honestly run: no native
    // Save-As dialog, no paste into another app, no clipboard pixels. The
    // chrome around them is what the preview is for, and every one is a
    // no-op here rather than a fake.
    case "set_draw_prefs":
    case "save_svg":
    case "open_draw_with_image":
    case "clipboard_image_data":
      return null;
    case "item_image_data":
    case "item_cover_url":
      return IMAGES.get(Number(args.id)) ?? null;
    // Attachment covers come from the same fixture map; the preview has no
    // attachment files, so an id it does not know is simply no picture.
    case "attachment_cover_url":
      return IMAGES.get(Number(args.id)) ?? null;
    // The packs commands, with the behaviour Rust has: summaries carry the
    // leading members, mutations move the same rows, and a refusal is a
    // sentence rather than a code.
    case "list_packs":
      return packs.map(packSummary);
    case "get_pack": {
      const pack = requirePack(args.id);
      return { pack: packSummary(pack), items: packMembers(pack) };
    }
    case "create_pack": {
      const name = String(args.name ?? "").trim();
      if (!name) throw "Give the pack a name.";
      if ([...name].length > 60) throw "Pack names can be at most 60 characters.";
      const pack: FixturePack = { id: nextPackId++, name, itemIds: [] };
      const added = appendToPack(pack, (args.itemIds as number[]) ?? []);
      packs.push(pack);
      return {
        id: pack.id,
        name: pack.name,
        itemCount: added.length,
        previewItemIds: added.slice(0, 4),
      };
    }
    case "rename_pack": {
      const name = String(args.name ?? "").trim();
      if (!name) throw "Give the pack a name.";
      if ([...name].length > 60) throw "Pack names can be at most 60 characters.";
      requirePack(args.id).name = name;
      return null;
    }
    case "delete_pack": {
      const pack = requirePack(args.id);
      packs.splice(packs.indexOf(pack), 1);
      return null;
    }
    case "pack_add_items": {
      const pack = requirePack(args.id);
      appendToPack(pack, (args.itemIds as number[]) ?? []);
      return null;
    }
    case "pack_remove_items": {
      const pack = requirePack(args.id);
      const ids = (args.itemIds as number[]) ?? [];
      pack.itemIds = pack.itemIds.filter((id) => !ids.includes(id));
      return null;
    }
    case "pack_reorder": {
      const pack = requirePack(args.id);
      const ids = (args.itemIds as number[]) ?? [];
      const current = [...pack.itemIds];
      // Rust refuses a stale order rather than dropping members it did not see.
      const same =
        current.length === ids.length &&
        current.every((id) => ids.includes(id)) &&
        ids.every((id) => current.includes(id));
      if (!same) throw "The pack's items changed. Reopen it and try again.";
      pack.itemIds = ids;
      return null;
    }
    case "clear_captures": {
      // The Rust semantics: clipboard and dictation history go, favourites and
      // hand-made items stay.
      state.items = state.items.filter((i) => i.origin === "manual" || i.favorite !== 0);
      return null;
    }
    // Rust hides the window when this arrives. The preview cannot, and must
    // not: the shelf is the whole surface being looked at.
    case "hide_panel":
    case "notify_error":
      return null;
    case "get_crash_report":
    case "check_for_update":
      return null;
    case "update_item": {
      const item = state.items.find((entry) => entry.id === Number(args.id));
      if (!item) throw "That note is no longer available.";
      item.content = String(args.content);
      void emit("refresh");
      return null;
    }
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
    case "set_pinned": {
      const item = state.items.find((i) => i.id === args.id);
      if (!item) throw "That item no longer exists.";
      if (args.pinned && item.pinnedAt === null) {
        if (state.items.filter((i) => i.pinnedAt !== null).length >= 3) {
          throw "Three items are pinned. Unpin one first.";
        }
        item.pinnedAt = Date.now();
      } else if (!args.pinned) {
        item.pinnedAt = null;
      }
      // Rust returns the timeline pins-first; mirror it so the preview's
      // Ctrl+1–3 and arrow order match the app.
      const pinned = state.items.filter((i) => i.pinnedAt !== null).sort((a, b) => a.pinnedAt! - b.pinnedAt!);
      const rest = state.items.filter((i) => i.pinnedAt === null).sort((a, b) => b.createdAt - a.createdAt);
      state.items = [...pinned, ...rest];
      state.slotIds = [...pinned, ...rest].slice(0, 3).map((i) => i.id);
      return null;
    }
    case "delete_item":
      state.items = state.items.filter((i) => i.id !== args.id);
      // The rows cascade: a deleted item leaves every pack it was in.
      for (const pack of packs) pack.itemIds = pack.itemIds.filter((id) => id !== args.id);
      state.slotIds = state.slotIds.filter((id) => id !== args.id);
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

export function install(label: "main" | "settings" | "draw" | "pill") {
  mockWindows(label, "main", "settings", "draw", "pill");
  mockIPC((cmd, payload) => handle(cmd, (payload ?? {}) as Record<string, unknown>), {
    shouldMockEvents: true,
  });
  document.documentElement.dataset.theme = theme;
  document.documentElement.dataset.preview = label;
  if (params.has("bare")) document.documentElement.dataset.bare = "";
  if (label === "pill") playPill();
  if (label === "draw" && scene === "sketch") {
    (window as { __bardPreviewShapes?: Shape[] }).__bardPreviewShapes = SKETCH;
  }
}

const PILL_SCENES: Record<string, PillNote> = {
  copied: { kind: "copy", text: "Copied", detail: "https://handy.computer/docs/models", sticky: true, holdMs: 0 },
  selected: { kind: "select", text: "On your clipboard", detail: "https://handy.computer/docs/models", slot: 2, slots: 3, sticky: true, holdMs: 0 },
  listening: { kind: "listen", text: "Listening", sticky: true, holdMs: 0 },
  working: { kind: "work", text: "Transcribing", sticky: true, holdMs: 0 },
  done: { kind: "done", text: "Transcript copied", detail: "Book the Cape Town venue for the twelfth", sticky: true, holdMs: 0 },
  paused: { kind: "pause", text: "Clipboard capture paused", sticky: true, holdMs: 0 },
  error: { kind: "error", text: "Nothing in slot 3 yet", sticky: true, holdMs: 0 },
  preview: {
    kind: "preview",
    text: "Clipboard",
    sticky: true,
    holdMs: 0,
    preview: {
      kind: "text",
      text: "Dictation is local. The audio never leaves this computer, and Refine only ever sees the finished text.\n\nKeep the release notes under 150 words, and lead with what changed for the person using it.",
      length: 214,
      files: [],
    },
  },
};

/** One pill note, as Rust would send it; listening also moves the meter. */
function playPill() {
  const note = PILL_SCENES[scene] ?? PILL_SCENES.copied;
  window.setTimeout(() => void emit("pill", note), 150);
  if (note.kind === "listen") {
    let t = 0;
    window.setInterval(() => {
      t += 1;
      const level = 0.12 + 0.18 * Math.abs(Math.sin(t / 2.3) * Math.cos(t / 5.1)) + 0.05 * Math.random();
      void emit("mic-level", [level, 0.9]);
    }, 60);
  }
}

const INK = "#1d1d1f";
const BLUE = "#2c63e6";
const RED = "#e5484d";

/** A small flow sketch: the kind of diagram Draw is for. */
const SKETCH: Shape[] = [
  { kind: "rect", id: 1, color: INK, width: 2.5, a: { x: 70, y: 80 }, b: { x: 240, y: 160 } },
  { kind: "text", id: 2, color: INK, size: 20, at: { x: 104, y: 122 }, text: "Copy / Say" },
  { kind: "arrow", id: 3, color: BLUE, width: 3, a: { x: 252, y: 120 }, b: { x: 382, y: 120 } },
  { kind: "ellipse", id: 4, color: INK, width: 2.5, a: { x: 392, y: 70 }, b: { x: 606, y: 172 } },
  { kind: "text", id: 5, color: INK, size: 20, at: { x: 450, y: 122 }, text: "One timeline" },
  { kind: "arrow", id: 6, color: BLUE, width: 3, a: { x: 498, y: 182 }, b: { x: 498, y: 282 } },
  { kind: "rect", id: 7, color: INK, width: 2.5, a: { x: 384, y: 292 }, b: { x: 614, y: 362 } },
  { kind: "text", id: 8, color: INK, size: 20, at: { x: 412, y: 330 }, text: "On your clipboard" },
  {
    kind: "pen",
    id: 9,
    color: RED,
    width: 3,
    points: Array.from({ length: 48 }, (_, i) => ({
      x: 96 + i * 3.2,
      y: 192 + Math.sin(i / 3.2) * 5 + (i > 40 ? (i - 40) * -1.5 : 0),
    })),
  },
  { kind: "text", id: 10, color: RED, size: 16, at: { x: 96, y: 236 }, text: "Alt+Shift+V  ·  Alt+Shift+Space" },
];
