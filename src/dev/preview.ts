/*
  Browser preview of every Bard window, with the Rust core mocked.

  Dev-only: each entry imports this when it runs outside Tauri, so
  `bun run dev` + http://localhost:1420/?preview renders the real components
  against fixture data. It exists for design work and for Store screenshots —
  a shelf that is DWM-composited and always on top is awkward to capture, and a
  fixture is reproducible where a person's live data is neither.

  Query parameters: theme=light|dark, scene=empty|onboarding|paused
  (main), scene=copied|selected|listening|working|done|preview|error|paused
  (pill), pane=<settings section>, dock=float|left|right, platform=linux.
*/
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Shape } from "../draw/shapes";
import catalog from "../../src-tauri/src/catalog/catalog.json";
import { version as appVersion } from "../../package.json";
import type {
  AppState,
  Item,
  ModelInfo,
  PackSummary,
  PillNote,
  ShortcutState,
  TranscriptOptions,
} from "../types";
import "./preview.css";

const params = new URLSearchParams(location.search);
const scene = params.get("scene") ?? "";
const theme = params.get("theme") ?? "light";
const dockParam = params.get("dock");
const dock: AppState["dock"] = dockParam === "left" || dockParam === "right" ? dockParam : "float";
const now = Date.now();
const minutes = (n: number) => now - n * 60_000;

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
  text(2, "https://handy.computer/docs/models", "clipboard", 1),
  text(13, "#6f4a4a", "clipboard", 60),
  shot(11, "clipboard", 2),
  shot(12, "manual", 34),
  text(3, "Dictation is local. The audio never leaves this computer.", "dictation", 14),
  text(1, "Ship the release notes before Friday — keep them under 150 words.", "manual", 9),
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

/**
 * A long history, for the browser preview only.
 *
 * The shelf draws a screenful at a time and its row height is measured, so
 * the interesting question about a five hundred item history is how the list
 * behaves when it is scrolled and searched. `?stress=500` builds one, with
 * the same shapes the real fixture carries, so the cost of the window and of
 * the filter can be measured rather than assumed.
 */
const stress = Number(params.get("stress") ?? "0");
if (Number.isFinite(stress) && stress > 0) {
  const shapes = [
    (n: number) => `https://example.test/releases/v${n}.0/download`,
    (n: number) => `cargo build --release --package workspace-${n}`,
    (n: number) => `The quick brown fox jumps over the lazy dog, number ${n}`,
    (n: number) => `#${(n * 2654435761 % 0xffffff).toString(16).padStart(6, "0")}`,
  ];
  for (let n = 0; n < stress; n++) {
    items.push({
      id: 1000 + n,
      sectionId: null,
      content: shapes[n % shapes.length](n),
      kind: "text",
      origin: "clipboard",
      favorite: 0,
      done: 0,
      createdAt: now - n * 60_000,
      updatedAt: now - n * 60_000,
      pinnedAt: null,
    });
  }
}

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
      // ?kept=1: the desktop granted Dictate a chord it uses itself.
      if (portal && id === "dictate" && params.has("kept")) {
        return { id, label, chord, display: "Ctrl+Alt+D", defaultChord, status: "taken", conflict: "show desktop" };
      }
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
    case "get_stay_open":
      return false;
    case "set_stay_open":
      return Boolean(args.stay);
    case "hold_windows":
      return undefined;
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
    case "get_microphones":
      return ["Default microphone", "Studio USB Mic"];
    case "get_setting_str":
      return settings[String(args.key)] ?? "";
    case "get_dictation_options":
      return {
        trailingSpace: settings.dictation_trailing_space !== "0",
        fillers: settings.dictation_fillers === "1",
        silenceMs: Number(settings.dictation_silence_ms ?? 1600),
      };
    case "set_dictation_options": {
      const next = (args.options ?? {}) as Partial<TranscriptOptions>;
      settings.dictation_trailing_space = next.trailingSpace ? "1" : "0";
      settings.dictation_fillers = next.fillers ? "1" : "0";
      settings.dictation_silence_ms = String(next.silenceMs ?? 1600);
      return null;
    }
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

export function install(label: "main" | "settings" | "draw" | "pill" | "guide") {
  mockWindows(label, "main", "settings", "draw", "pill", "guide");
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

const RAIL: PillNote["rail"] = [
  { kind: "text", label: "ffmpeg -i take.wav -ar 16000 -ac 1 take-16k.wav", pinned: true },
  { kind: "text", label: "https://handy.computer/docs/models", pinned: false },
  { kind: "image", label: "Image", pinned: false },
];

const PILL_SCENES: Record<string, PillNote> = {
  copied: { kind: "copy", text: "Copied", detail: "https://handy.computer/docs/models", sticky: true, holdMs: 0 },
  image: { kind: "copy", text: "Image copied", sticky: true, holdMs: 0 },
  selected: { kind: "select", text: "On your clipboard", detail: "https://handy.computer/docs/models", sticky: true, holdMs: 0 },
  cycle: { kind: "select", text: "On your clipboard", detail: "https://handy.computer/docs/models", slot: 2, slots: 3, rail: RAIL, sticky: true, holdMs: 0 },
  listening: { kind: "listen", text: "Listening", sticky: true, holdMs: 0 },
  working: { kind: "work", text: "Transcribing", sticky: true, holdMs: 0 },
  done: {
    kind: "done",
    text: "Transcript on your clipboard",
    sticky: true,
    holdMs: 0,
    preview: {
      kind: "text",
      text: "Send the release notes to the team before noon, and book the venue for the Cape Town evening on the twelfth.",
      length: 108,
      files: [],
    },
  },
  paused: { kind: "pause", text: "Capture paused", sticky: true, holdMs: 0 },
  resumed: { kind: "resume", text: "Capture on", sticky: true, holdMs: 0 },
  error: { kind: "error", text: "Nothing in slot 3 yet", sticky: true, holdMs: 0 },
  preview: {
    kind: "preview",
    text: "Clipboard",
    slot: 2,
    slots: 3,
    rail: RAIL,
    sticky: true,
    holdMs: 0,
    preview: {
      kind: "text",
      text: "Dictation is local. The audio never leaves this computer.\n\nKeep the release notes under 150 words, and lead with what changed for the person using it.",
      length: 152,
      files: [],
    },
  },
  empty: { kind: "preview", text: "Clipboard", sticky: true, holdMs: 0, preview: { kind: "empty", files: [] } },
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
