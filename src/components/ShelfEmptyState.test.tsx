import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ShelfEmptyState from "./ShelfEmptyState";

describe("empty shelf recovery", () => {
  it("offers a way out of a search even when the history is empty", () => {
    const html = renderToStaticMarkup(<ShelfEmptyState query="missing" filter="all" favoritesOnly={false} empty onReset={() => {}} />);
    expect(html).toContain("No matches");
    expect(html).toContain("Clear filters");
    expect(html).not.toContain("Your shelf starts here");
  });

  it("offers recovery from combined type and favorite filters", () => {
    const html = renderToStaticMarkup(<ShelfEmptyState query="" filter="image" favoritesOnly empty={false} onReset={() => {}} />);
    expect(html).toContain("Clear filters");
    expect(html).toContain("favorites");
  });

  it("teaches first capture only on an unfiltered, empty shelf", () => {
    const html = renderToStaticMarkup(<ShelfEmptyState query="" filter="all" favoritesOnly={false} empty onReset={() => {}} />);
    expect(html).toContain("Your shelf starts here");
    expect(html).not.toContain("Clear filters");
  });
});
