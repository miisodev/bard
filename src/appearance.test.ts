import { describe, expect, it } from "vitest";
import { CANVAS, canvasFor, drawCanvasFor, inkFor, isHexColor } from "./appearance";
import type { Appearance } from "./appearance";

const base: Appearance = {
  theme: "light",
  background: "theme",
  backgroundColor: "#e6e7ea",
  drawBackground: "theme",
  drawBackgroundColor: "#ffffff",
};

describe("appearance", () => {
  it("chooses the ink that reads on a canvas", () => {
    expect(inkFor("#ffffff")).toBe("#1d1d1f");
    expect(inkFor("#101014")).toBe("#f5f5f7");
    expect(inkFor(CANVAS.dark)).toBe("#f5f5f7");
  });

  it("only accepts six-digit hex for a custom background", () => {
    expect(isHexColor("#12abEF")).toBe(true);
    expect(isHexColor("#fff")).toBe(false);
    expect(canvasFor({ ...base, background: "custom", backgroundColor: "red" })).toBeNull();
    expect(canvasFor({ ...base, background: "custom", backgroundColor: "#123456" })).toBe("#123456");
  });

  it("follows the theme until a background is chosen", () => {
    expect(canvasFor(base)).toBeNull();
    expect(canvasFor({ ...base, background: "dark" })).toBe(CANVAS.dark);
    expect(drawCanvasFor({ ...base, drawBackground: "dark" })).toBe("#2a2a2f");
  });
});
