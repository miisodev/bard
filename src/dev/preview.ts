/*
  Browser preview of every Bard window, with the Rust core mocked.

  Dev-only: each entry imports this when it runs outside Tauri, so
  `bun run dev` + http://localhost:1420/?preview renders the real components
  against fixture data. It exists for design work and for Store screenshots —
  a shelf that is DWM-composited and always on top is awkward to capture, and a
  fixture is reproducible where a person's live data is neither.

  Query parameters: theme=light|dark, scene=empty|trial|restricted|recording|review,
  pane=<settings section>, edge=bottom|top|left|right, review=error (scene=review
  only, to open the sheet in its refused state).
*/
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Shape } from "../draw/shapes";
import catalog from "../../src-tauri/src/catalog/catalog.json";
import { version as appVersion } from "../../package.json";
import type { AppState, Item, ModelInfo, PackSummary } from "../types";
import "./preview.css";

const params = new URLSearchParams(location.search);
const scene = params.get("scene") ?? "";
const theme = params.get("theme") ?? "light";
const edgeParam = params.get("edge");
const shelfEdge: AppState["shelfEdge"] =
  edgeParam === "top" || edgeParam === "left" || edgeParam === "right" ? edgeParam : "bottom";
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
  appVersion,
  onboardingCompleted: scene !== "onboarding",
  autostartEnabled: true,
  captureDoubleShift: true,
  modelId: "parakeet-tdt-0.6b-v3",
  modelReady: true,
  micReady: true,
  activeItemId: null,
  license,
  channel: params.get("channel") === "flatpak" ? "flatpak" : params.get("channel") === "store" ? "store" : "direct",
  platform: params.get("platform") ?? "windows",
  desktop: { session: params.get("platform") === "linux" ? "wayland" : "windows", shortcutsReady: false, inputReady: false },
  background: "theme",
  backgroundColor: "#e6e7ea",
  sort: "newest",
  shelfEdge,
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
  dictation_overlay_position: "bottom-middle",
  microphone: "",
  // Only the review scene turns the sheet on; every other scene shows the
  // shipped default, paste directly.
  dictation_review: scene === "review" ? "1" : "0",
};

/*
  The take the review scene opens: Rust would have inserted it when the
  transcript landed, and each command writes the edited body back onto it.
*/
const REVIEW_TEXT =
  "Dictation is local. The audio never leaves this computer, and Refine only ever sees the finished text.";
let reviewTake: Item | null = null;

/** The clipboard when the browser allows it; the preview does not design the refusal. */
function writeClipboard(text: string) {
  void navigator.clipboard?.writeText(text).catch(() => undefined);
}

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
    case "get_sound_on_capture":
      return true;
    // The v2.1 Draw commands the preview cannot honestly run: no native
    // Save-As dialog, no paste into another app, no clipboard pixels. The
    // chrome around them is what the preview is for, and every one is a
    // no-op here rather than a fake.
    case "set_draw_prefs":
    case "send_drawing":
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
    case "set_shelf_edge": {
      state.shelfEdge = args.edge as AppState["shelfEdge"];
      void emit("shelf-edge", args.edge);
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
    case "shelf_hidden":
    case "hide_panel":
    case "paste_back":
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
      return null;
    }
    case "delete_item":
      state.items = state.items.filter((i) => i.id !== args.id);
      // The rows cascade: a deleted item leaves every pack it was in.
      for (const pack of packs) pack.itemIds = pack.itemIds.filter((id) => id !== args.id);
      return null;
    case "set_sort":
      state.sort = args.order as AppState["sort"];
      return null;
    case "set_active_item":
      state.activeItemId = (args.id as number | null) ?? null;
      return null;
    // The review sheet, as Rust runs it: every command carries the edited
    // body back onto the take. Rust hides the overlay when one lands; the
    // preview cannot hide its own window, so the sheet stays put to be
    // looked at, which is what this scene is for.
    case "dictation_confirm":
    case "dictation_copy":
    case "dictation_dismiss": {
      const body = String(args.text ?? "");
      if (reviewTake) reviewTake.content = body;
      if (cmd === "dictation_copy") writeClipboard(body);
      if (cmd === "dictation_confirm") void emit("dictation-pasted");
      if (cmd === "dictation_dismiss") void emit("dictation-cancelled");
      return null;
    }
    case "set_dictation_review":
      settings.dictation_review = args.enabled ? "1" : "0";
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
  if (params.has("bare")) document.documentElement.dataset.bare = "";
  if (label === "overlay" && scene === "recording") playRecording();
  if (label === "overlay" && scene === "review") playReview();
  if (label === "draw" && scene === "sketch") {
    (window as { __bardPreviewShapes?: Shape[] }).__bardPreviewShapes = SKETCH;
  }
}

/** A take in progress: the pill listening, its meter moving like speech. */
function playRecording() {
  let t = 0;
  window.setTimeout(() => {
    void emit("dictation-overlay", "listening");
    // A meter already mid-sentence, so a still frame shows speech.
    for (let i = 0; i < 12; i += 1) {
      void emit("mic-level", [0.35 + 0.55 * Math.abs(Math.sin(i * 0.9) * Math.cos(i * 0.37)), 0.9]);
    }
  }, 150);
  window.setInterval(() => {
    t += 1;
    const level = 0.35 + 0.45 * Math.abs(Math.sin(t / 2.3) * Math.cos(t / 5.1)) + 0.15 * Math.random();
    void emit("mic-level", [level, 0.9]);
  }, 60);
}

/** A finished take, opening the review sheet the way Rust does. */
function playReview() {
  window.setTimeout(() => {
    const take = text(nextId++, REVIEW_TEXT, "dictation", 0);
    reviewTake = take;
    state.items.unshift(take);
    void emit("dictation-review", { text: REVIEW_TEXT });
    // `review=error` opens the sheet already refused, for designing retry.
    if (params.get("review") === "error") {
      window.setTimeout(
        () => void emit("dictation-review-error", "Bard couldn't reach the app it started in."),
        400,
      );
    }
  }, 150);
  // Escape is Rust's, globally; the preview stands that hook in so the
  // sheet's close path is real in the browser.
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") void emit("dictation-cancelled");
  });
}

const INK = "#1d1d1f";
const BLUE = "#2f6fed";
const RED = "#e5484d";

/** A small flow sketch: the kind of diagram Draw is for. */
const SKETCH: Shape[] = [
  { kind: "rect", id: 1, color: INK, width: 2.5, a: { x: 70, y: 170 }, b: { x: 240, y: 250 } },
  { kind: "text", id: 2, color: INK, size: 20, at: { x: 104, y: 202 }, text: "Copy / Say" },
  { kind: "arrow", id: 3, color: BLUE, width: 3, a: { x: 252, y: 210 }, b: { x: 382, y: 210 } },
  { kind: "ellipse", id: 4, color: INK, width: 2.5, a: { x: 392, y: 160 }, b: { x: 606, y: 262 } },
  { kind: "text", id: 5, color: INK, size: 20, at: { x: 450, y: 202 }, text: "One timeline" },
  { kind: "arrow", id: 6, color: BLUE, width: 3, a: { x: 498, y: 272 }, b: { x: 498, y: 372 } },
  { kind: "rect", id: 7, color: INK, width: 2.5, a: { x: 400, y: 382 }, b: { x: 600, y: 452 } },
  { kind: "text", id: 8, color: INK, size: 20, at: { x: 432, y: 410 }, text: "Paste anywhere" },
  {
    kind: "pen",
    id: 9,
    color: RED,
    width: 3,
    points: Array.from({ length: 48 }, (_, i) => ({
      x: 96 + i * 3.2,
      y: 282 + Math.sin(i / 3.2) * 5 + (i > 40 ? (i - 40) * -1.5 : 0),
    })),
  },
  { kind: "text", id: 10, color: RED, size: 16, at: { x: 96, y: 326 }, text: "Left Shift ×2  ·  Ctrl ×2" },
];
