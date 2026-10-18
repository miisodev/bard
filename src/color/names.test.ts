import { describe, expect, it } from "vitest";
import colornames from "./colornames.bestof.json";
import { colorName } from "./names";

describe("colorName", () => {
  it("names a colour the dataset has exactly", () => {
    expect(colorName("#c93f38")).toBe("100 Mph");
    expect(colorName("#191970")).toBe("20000 Leagues Under the Sea");
    expect(colorName("#ffffff")).toBe("White");
  });

  it("reads hex in any case, with or without the short form", () => {
    expect(colorName("#C93F38")).toBe("100 Mph");
    expect(colorName("  #FFF  ")).toBe("White");
    expect(colorName("#f80")).toBe(colorName("#ff8800"));
  });

  it("finds the family for a colour the dataset does not have", () => {
    expect(colorName("#d2691e")).toMatch(/chocolate|cocoa/i);
    expect(colorName("#2f6fed")).toMatch(/blue/i);
  });

  it("answers Unknown for anything that is not a hex colour", () => {
    const invalid = [
      "",
      "  ",
      "red",
      "#12345",
      "#1234567",
      "#12345678",
      "#gggggg",
      "rgb(210, 105, 30)",
      "d2691e",
    ];
    for (const input of invalid) {
      expect(colorName(input)).toBe("Unknown");
    }
  });

  it("never throws, whatever the caller passes", () => {
    expect(colorName(undefined as unknown as string)).toBe("Unknown");
    expect(colorName(42 as unknown as string)).toBe("Unknown");
  });

  it("returns the same name on every ask", () => {
    const name = colorName("#d2691e");
    expect(colorName("#d2691e")).toBe(name);
    expect(colorName("  #D2691E  ")).toBe(name);
    expect(colorName("#d2691e")).toBe(name);
  });
});

describe("the bundled colour dataset", () => {
  it("holds more than a thousand unique names", () => {
    const names = new Set(colornames.map((color) => color.name));
    expect(names.size).toBeGreaterThan(1000);
  });

  it("holds only six-digit hex values", () => {
    expect(colornames.every((color) => /^#[0-9a-f]{6}$/.test(color.hex))).toBe(true);
  });
});
