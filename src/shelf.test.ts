import { describe, expect, it } from "vitest";
import {
  clampPage,
  colorChannels,
  escapeOutcome,
  filterItems,
  filterShelf,
  insertCaptured,
  itemKind,
  itemPreview,
  matchesFilter,
  nextIndex,
  PACK_NO_TEXT,
  packText,
  packsZoneView,
  pageCount,
  pageItems,
  pageNumbers,
  pageSize,
  rovingPack,
  shelfCommand,
  toHex,
  toHsl,
  toHslText,
  toRgbText,
  TYPE_FILTERS,
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

describe("itemKind", () => {
  it("separates the six kinds", () => {
    expect(itemKind(item({}))).toBe("text");
    expect(itemKind(item({ kind: "image", origin: "clipboard" }))).toBe("image");
    expect(itemKind(item({ kind: "image", origin: "manual" }))).toBe("draw");
    expect(itemKind(item({ origin: "dictation" }))).toBe("dictation");
    expect(itemKind(item({ content: "#2f6fed" }))).toBe("color");
    expect(itemKind(item({ content: "rgb(47, 111, 237)" }))).toBe("color");
    expect(itemKind(item({ content: "https://miiso.dev/docs" }))).toBe("link");
  });

  it("puts a colour or a link ahead of the plain text it also is", () => {
    expect(itemKind(item({ content: "https://miiso.dev", origin: "dictation" }))).toBe("dictation");
    expect(itemKind(item({ content: "#2f6fed", origin: "dictation" }))).toBe("dictation");
  });
});

describe("matchesFilter", () => {
  it("buckets dictation as text and drawings as image", () => {
    expect(matchesFilter("dictation", "text")).toBe(true);
    expect(matchesFilter("draw", "image")).toBe(true);
    expect(matchesFilter("text", "image")).toBe(false);
  });

  it("gives links and colours their own chips only", () => {
    expect(matchesFilter("link", "link")).toBe(true);
    expect(matchesFilter("link", "text")).toBe(false);
    expect(matchesFilter("color", "color")).toBe(true);
    expect(matchesFilter("color", "text")).toBe(false);
    expect(matchesFilter("color", "all")).toBe(true);
  });
});

describe("filterItems", () => {
  const items = [
    item({ id: 1, content: "Ship the release notes" }),
    item({ id: 2, content: "https://miiso.dev/docs", favorite: 1 }),
    item({ id: 3, content: "#2f6fed" }),
    item({ id: 4, kind: "image", origin: "manual" }),
    item({ id: 5, content: "dictated line", origin: "dictation", favorite: 1 }),
  ];

  it("returns everything at rest", () => {
    expect(filterItems(items, "", "all", false).map((i) => i.id)).toEqual([1, 2, 3, 4, 5]);
  });

  it("searches content case-insensitively", () => {
    expect(filterItems(items, "RELEASE", "all", false).map((i) => i.id)).toEqual([1]);
    expect(filterItems(items, "nothing here", "all", false)).toEqual([]);
  });

  it("applies the chip and the favourite view together", () => {
    expect(filterItems(items, "", "text", false).map((i) => i.id)).toEqual([1, 5]);
    expect(filterItems(items, "", "all", true).map((i) => i.id)).toEqual([2, 5]);
    expect(filterItems(items, "miiso", "all", true).map((i) => i.id)).toEqual([2]);
    expect(filterItems(items, "", "image", false).map((i) => i.id)).toEqual([4]);
  });
});

describe("filterShelf", () => {
  const items = [
    item({ id: 1, content: "Ship the release notes" }),
    item({ id: 2, content: "https://miiso.dev/docs", favorite: 1 }),
    item({ id: 3, content: "#2f6fed" }),
    item({ id: 4, kind: "image", origin: "manual" }),
    item({ id: 5, content: "dictated line", origin: "dictation", favorite: 1 }),
  ];

  it("answers the same list as filterItems, chip by chip", () => {
    for (const query of ["", "release", "RELEASE", "nothing here"]) {
      for (const favoritesOnly of [false, true]) {
        const view = filterShelf(items, query, "all", favoritesOnly);
        for (const chip of TYPE_FILTERS) {
          expect(filterShelf(items, query, chip.id, favoritesOnly).visible.map((i) => i.id)).toEqual(
            filterItems(items, query, chip.id, favoritesOnly).map((i) => i.id),
          );
        }
        expect(view.visible.map((i) => i.id)).toEqual(
          filterItems(items, query, "all", favoritesOnly).map((i) => i.id),
        );
      }
    }
  });

  it("counts every chip in the same pass, under the same filter", () => {
    const { counts } = filterShelf(items, "", "all", false);
    for (const chip of TYPE_FILTERS) {
      expect(counts[chip.id]).toBe(filterItems(items, "", chip.id, false).length);
    }
    // A query applies to the counts as well: it is the same pass.
    const searched = filterShelf(items, "miiso", "all", false).counts;
    expect(searched.link).toBe(1);
    expect(searched.text).toBe(0);
    expect(searched.all).toBe(1);
  });
});

describe("insertCaptured", () => {
  it("puts a capture after the pins, newest first", () => {
    const items = [
      item({ id: 9, pinnedAt: 30 }),
      item({ id: 4, pinnedAt: 10 }),
      item({ id: 8 }),
      item({ id: 7 }),
    ];
    expect(insertCaptured(items, item({ id: 10 })).map((i) => i.id)).toEqual([9, 4, 10, 8, 7]);
  });

  it("goes last when everything is pinned, and replaces its own id", () => {
    const pinned = [item({ id: 2, pinnedAt: 1 }), item({ id: 1, pinnedAt: 1 })];
    expect(insertCaptured(pinned, item({ id: 3 })).map((i) => i.id)).toEqual([2, 1, 3]);
    expect(insertCaptured(pinned, item({ id: 1 })).map((i) => i.id)).toEqual([2, 1]);
  });
});

describe("pageSize", () => {
  it("counts only fully visible cards", () => {
    // 240-wide cards with a 12 gap in an 800px zone: three fit (744 used).
    expect(pageSize(800, 240, 12)).toBe(3);
    expect(pageSize(251, 240, 12)).toBe(1);
    // Two cards need 240 + 12 + 240; only then does the second fit whole.
    expect(pageSize(492, 240, 12)).toBe(2);
    expect(pageSize(491, 240, 12)).toBe(1);
  });

  it("never returns zero, and survives a bad measurement", () => {
    expect(pageSize(0, 240, 12)).toBe(1);
    expect(pageSize(Number.NaN, 240, 12)).toBe(1);
    expect(pageSize(500, 0, 12)).toBe(1);
  });
});

describe("pagination", () => {
  it("counts pages, at least one", () => {
    expect(pageCount(11, 5)).toBe(3);
    expect(pageCount(10, 5)).toBe(2);
    expect(pageCount(0, 5)).toBe(1);
  });

  it("clamps into range", () => {
    expect(clampPage(7, 11, 5)).toBe(2);
    expect(clampPage(-1, 11, 5)).toBe(0);
    expect(clampPage(1, 0, 5)).toBe(0);
  });

  it("slices one page and nothing beyond it", () => {
    const list = [1, 2, 3, 4, 5, 6, 7];
    expect(pageItems(list, 0, 3)).toEqual([1, 2, 3]);
    expect(pageItems(list, 2, 3)).toEqual([7]);
    expect(pageItems(list, 9, 3)).toEqual([]);
  });

  it("shows every page while they are few", () => {
    expect(pageNumbers(1, 1)).toEqual([1]);
    expect(pageNumbers(4, 7)).toEqual([1, 2, 3, 4, 5, 6, 7]);
  });

  it("elides the middle and keeps the ends", () => {
    expect(pageNumbers(1, 12)).toEqual([1, 2, 3, "gap", 12]);
    expect(pageNumbers(6, 12)).toEqual([1, "gap", 5, 6, 7, 8, "gap", 12]);
    expect(pageNumbers(12, 12)).toEqual([1, "gap", 11, 12]);
  });

  it("never repeats a number and always covers the last page", () => {
    for (let current = 1; current <= 30; current += 1) {
      const numbers = pageNumbers(current, 30).filter((n): n is number => n !== "gap");
      expect(new Set(numbers).size).toBe(numbers.length);
      expect(numbers[0]).toBe(1);
      expect(numbers[numbers.length - 1]).toBe(30);
      expect(numbers).toContain(current);
    }
  });
});

describe("nextIndex", () => {
  it("moves, clamps and starts", () => {
    expect(nextIndex(0, 1, 5)).toBe(1);
    expect(nextIndex(4, 1, 5)).toBe(4);
    expect(nextIndex(0, -1, 5)).toBe(0);
    expect(nextIndex(-1, 1, 5)).toBe(0);
    expect(nextIndex(-1, -1, 5)).toBe(4);
    expect(nextIndex(0, 1, 0)).toBe(-1);
  });
});

describe("itemPreview", () => {
  it("collapses whitespace and truncates at the limit", () => {
    expect(itemPreview(item({ content: "  two\n  lines  " }))).toBe("two lines");
    expect(itemPreview(item({ content: "abcdef" }), 4)).toBe("abcd…");
    expect(itemPreview(item({ content: "abc" }), 4)).toBe("abc");
  });

  it("names what has no text to read", () => {
    expect(itemPreview(item({ kind: "image", content: "" }))).toBe("image");
    expect(itemPreview(item({ content: "   " }))).toBe("empty note");
  });
});

describe("packText", () => {
  const members = [
    item({ id: 1, content: "Ship the release notes" }),
    item({ id: 2, content: "https://miiso.dev/docs" }),
    item({ id: 3, content: "#2f6fed" }),
    item({ id: 4, kind: "image", content: "" }),
    item({ id: 5, kind: "image", origin: "manual", content: "" }),
    item({ id: 6, content: "dictated line", origin: "dictation" }),
    item({ id: 7, content: "   " }),
  ];

  it("joins the text-bearing members in pack order, one per line", () => {
    expect(packText(members)).toBe(
      "Ship the release notes\nhttps://miiso.dev/docs\n#2f6fed\ndictated line",
    );
  });

  it("skips images and drawings, and a note that is only whitespace", () => {
    const text = packText([members[3], members[4]]);
    expect(text).toBe("");
    expect(packText([members[0], members[4], members[6]])).toBe("Ship the release notes");
  });

  it("returns nothing for an empty pack", () => {
    expect(packText([])).toBe("");
    expect(PACK_NO_TEXT).toBe("This pack has no text to paste.");
  });
});

describe("packsZoneView", () => {
  it("paints cards whenever there are any, whatever the read said", () => {
    expect(packsZoneView(3, true, null)).toBe("cards");
    expect(packsZoneView(1, false, "Bard couldn't read your packs.")).toBe("cards");
  });

  it("names a failed read over an empty one", () => {
    expect(packsZoneView(0, false, "Bard couldn't read your packs.")).toBe("error");
    expect(packsZoneView(0, true, "Bard couldn't read your packs.")).toBe("error");
  });

  it("stays blank until the first read lands, then says there are none", () => {
    expect(packsZoneView(0, false, null)).toBe("none");
    expect(packsZoneView(0, true, null)).toBe("empty");
  });
});

describe("rovingPack", () => {
  it("keeps the cursor's pack while it is on the page", () => {
    expect(rovingPack(7, [5, 7, 9])).toBe(7);
  });

  it("falls to the first card when the cursor is elsewhere", () => {
    expect(rovingPack(6, [5, 7, 9])).toBe(5);
    expect(rovingPack(null, [5, 7, 9])).toBe(5);
  });

  it("has no stop on an empty page", () => {
    expect(rovingPack(7, [])).toBeNull();
  });
});

describe("escapeOutcome", () => {
  it("follows the fixed order", () => {
    expect(
      escapeOutcome({ query: "rel", composer: "draft", composerFocused: true, selectionSize: 3 }),
    ).toBe("search");
    expect(
      escapeOutcome({ query: "", composer: "draft", composerFocused: true, selectionSize: 3 }),
    ).toBe("composer");
    expect(
      escapeOutcome({ query: "", composer: "draft", composerFocused: false, selectionSize: 3 }),
    ).toBe("selection");
    expect(
      escapeOutcome({ query: "", composer: "", composerFocused: false, selectionSize: 0 }),
    ).toBe("retract");
  });
});

describe("shelfCommand", () => {
  const key = (k: string, mods: Partial<Parameters<typeof shelfCommand>[0]> = {}) =>
    shelfCommand({ key: k, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods });

  it("reads the quick-paste chords", () => {
    expect(key("3", { ctrlKey: true })).toEqual({ type: "pasteNth", index: 2 });
    expect(key("1", { metaKey: true })).toEqual({ type: "pasteNth", index: 0 });
    expect(key("3", { ctrlKey: true, shiftKey: true })).toBeNull();
    expect(key("0", { ctrlKey: true })).toBeNull();
  });

  it("reads the command chords", () => {
    expect(key("k", { ctrlKey: true })).toEqual({ type: "filters" });
    expect(key("/", { ctrlKey: true })).toEqual({ type: "shortcuts" });
    expect(key("c", { ctrlKey: true })).toEqual({ type: "copy" });
    expect(key("p", { ctrlKey: true })).toEqual({ type: "pin" });
    expect(key("m", { ctrlKey: true, shiftKey: true })).toEqual({ type: "copyMarkdown" });
    expect(key("x", { ctrlKey: true })).toBeNull();
  });

  it("reads movement and paging", () => {
    expect(key("ArrowLeft")).toEqual({ type: "move", delta: -1, extend: false });
    expect(key("ArrowDown")).toEqual({ type: "move", delta: 1, extend: false });
    expect(key("ArrowRight", { shiftKey: true })).toEqual({ type: "move", delta: 1, extend: true });
    expect(key("PageUp")).toEqual({ type: "page", delta: -1 });
    expect(key("PageDown")).toEqual({ type: "page", delta: 1 });
  });

  it("reads the plain commands and leaves everything else alone", () => {
    expect(key("Enter")).toEqual({ type: "paste" });
    expect(key("Delete")).toEqual({ type: "delete" });
    expect(key("Backspace")).toEqual({ type: "delete" });
    expect(key(" ")).toEqual({ type: "toggleDone" });
    expect(key("Escape")).toEqual({ type: "escape" });
    expect(key("a")).toBeNull();
    expect(key("Tab")).toBeNull();
  });
});

describe("colour channels", () => {
  it("reads every hex length, and rgb() and hsl()", () => {
    expect(colorChannels("#2f6fed")).toEqual({ r: 47, g: 111, b: 237 });
    expect(colorChannels("#fff")).toEqual({ r: 255, g: 255, b: 255 });
    expect(colorChannels("#2F6FEDD0")).toEqual({ r: 47, g: 111, b: 237 });
    expect(colorChannels("rgb(47, 111, 237)")).toEqual({ r: 47, g: 111, b: 237 });
    expect(colorChannels("rgba(47, 111, 237, 0.5)")).toEqual({ r: 47, g: 111, b: 237 });
    expect(colorChannels("hsl(221, 84%, 56%)")).toEqual({ r: 49, g: 108, b: 237 });
    expect(colorChannels("#2f6fed ")).not.toBeNull();
  });

  it("refuses what it cannot read", () => {
    expect(colorChannels("oklch(0.5 0.2 260)")).toBeNull();
    expect(colorChannels("#2f6fe")).toBeNull();
    expect(colorChannels("the accent is #2f6fed")).toBeNull();
    expect(colorChannels("")).toBeNull();
  });

  it("formats the three notations", () => {
    const rgb = { r: 111, g: 74, b: 74 };
    expect(toHex(rgb)).toBe("#6f4a4a");
    expect(toRgbText(rgb)).toBe("rgb(111, 74, 74)");
    expect(toHsl(rgb)).toEqual({ h: 0, s: 20, l: 36 });
    expect(toHslText(rgb)).toBe("hsl(0, 20%, 36%)");
    expect(toHsl({ r: 255, g: 255, b: 255 })).toEqual({ h: 0, s: 0, l: 100 });
    expect(toHsl({ r: 0, g: 0, b: 0 })).toEqual({ h: 0, s: 0, l: 0 });
  });
});
