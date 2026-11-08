import { describe, expect, it } from "vitest";
import {
  ago,
  fileFolder,
  fileName,
  filePaths,
  filterShelf,
  insertCaptured,
  itemKind,
  itemPreview,
  listCommand,
  matchesFilter,
  nextIndex,
  TYPE_FILTERS,
  type KeyShape,
} from "./shelf";
import type { Item } from "./types";

function item(overrides: Partial<Item>): Item {
  return {
    id: 1,
    sectionId: null,
    content: "a note",
    kind: "text",
    origin: "clipboard",
    favorite: 0,
    done: 0,
    createdAt: 0,
    updatedAt: 0,
    pinnedAt: null,
    ...overrides,
  };
}

const key = (overrides: Partial<KeyShape>): KeyShape => ({
  key: "",
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  inSearch: false,
  ...overrides,
});

describe("itemKind", () => {
  it("names each kind once", () => {
    expect(itemKind(item({ kind: "image", origin: "clipboard" }))).toBe("image");
    expect(itemKind(item({ kind: "image", origin: "manual" }))).toBe("draw");
    expect(itemKind(item({ kind: "file", content: "/a/b.pdf" }))).toBe("file");
    expect(itemKind(item({ origin: "dictation" }))).toBe("dictation");
    expect(itemKind(item({ content: "#2c63e6" }))).toBe("color");
    expect(itemKind(item({ content: "https://miiso.dev/bard" }))).toBe("link");
    expect(itemKind(item({ content: "plain words" }))).toBe("text");
  });
});

describe("filters", () => {
  it("put every item in exactly one bucket besides All", () => {
    const kinds = ["text", "image", "link", "color", "dictation", "draw", "file"] as const;
    for (const kind of kinds) {
      const buckets = TYPE_FILTERS.filter((f) => f.id !== "all" && matchesFilter(kind, f.id));
      expect(buckets, kind).toHaveLength(1);
    }
  });

  it("count every filter in one pass and honour the query", () => {
    const items = [
      item({ id: 1, content: "hello world" }),
      item({ id: 2, content: "https://example.com/hello" }),
      item({ id: 3, kind: "image", content: "" }),
      item({ id: 4, kind: "file", content: "/home/you/hello.txt" }),
    ];
    const all = filterShelf(items, "", "all");
    expect(all.visible.map((i) => i.id)).toEqual([1, 2, 3, 4]);
    expect(all.counts).toEqual({ all: 4, text: 1, link: 1, image: 1, file: 1 });
    const hello = filterShelf(items, "HELLO", "link");
    expect(hello.visible.map((i) => i.id)).toEqual([2]);
    expect(hello.counts.all).toBe(3);
  });
});

describe("insertCaptured", () => {
  it("lands after the pins and replaces an older copy of itself", () => {
    const pinned = item({ id: 1, pinnedAt: 5 });
    const older = item({ id: 2 });
    const fresh = item({ id: 3 });
    expect(insertCaptured([pinned, older], fresh).map((i) => i.id)).toEqual([1, 3, 2]);
    expect(insertCaptured([pinned, fresh, older], fresh).map((i) => i.id)).toEqual([1, 3, 2]);
    expect(insertCaptured([], fresh).map((i) => i.id)).toEqual([3]);
  });
});

describe("previews", () => {
  it("read as one short line", () => {
    expect(itemPreview(item({ content: "  a\n b  " }))).toBe("a b");
    expect(itemPreview(item({ content: "x".repeat(60) }))).toBe(`${"x".repeat(48)}…`);
    expect(itemPreview(item({ kind: "image", content: "" }))).toBe("image");
  });

  it("name files by their first file and a count", () => {
    const files = item({ kind: "file", content: "C:\\Docs\\Brief.pdf\nC:\\Docs\\Sheet.xlsx" });
    expect(filePaths(files)).toHaveLength(2);
    expect(itemPreview(files)).toBe("Brief.pdf and 1 more");
    expect(fileName("/home/you/notes.md")).toBe("notes.md");
    expect(fileFolder("/home/you/notes.md")).toBe("you");
  });
});

describe("ago", () => {
  it("stays short", () => {
    const now = 1_000_000_000;
    expect(ago(now - 10_000, now)).toBe("now");
    expect(ago(now - 5 * 60_000, now)).toBe("5m");
    expect(ago(now - 3 * 3_600_000, now)).toBe("3h");
    expect(ago(now - 2 * 86_400_000, now)).toBe("2d");
    expect(ago(now - 21 * 86_400_000, now)).toBe("3w");
  });
});

describe("keys", () => {
  it("move and clamp without wrapping", () => {
    expect(nextIndex(-1, 1, 5)).toBe(0);
    expect(nextIndex(-1, -1, 5)).toBe(4);
    expect(nextIndex(4, 1, 5)).toBe(4);
    expect(nextIndex(0, -1, 5)).toBe(0);
    expect(nextIndex(0, 1, 0)).toBe(-1);
  });

  it("copy with Enter and stay with Shift+Enter", () => {
    expect(listCommand(key({ key: "Enter" }))).toEqual({ type: "copy", hide: true });
    expect(listCommand(key({ key: "Enter", shiftKey: true }))).toEqual({ type: "copy", hide: false });
  });

  it("leave typing alone while the search field has focus", () => {
    expect(listCommand(key({ key: "Backspace", inSearch: true }))).toBeNull();
    expect(listCommand(key({ key: "p", ctrlKey: true, inSearch: true }))).toBeNull();
    expect(listCommand(key({ key: "Backspace" }))).toEqual({ type: "delete" });
    expect(listCommand(key({ key: "p", ctrlKey: true }))).toEqual({ type: "pin" });
  });

  it("always reach search, the guide and Escape", () => {
    expect(listCommand(key({ key: "f", ctrlKey: true, inSearch: true }))).toEqual({ type: "search" });
    expect(listCommand(key({ key: "/", ctrlKey: true, inSearch: true }))).toEqual({ type: "guide" });
    expect(listCommand(key({ key: "Escape", inSearch: true }))).toEqual({ type: "escape" });
    expect(listCommand(key({ key: "?" }))).toEqual({ type: "guide" });
  });
});
