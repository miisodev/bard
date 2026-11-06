import { expect, test, type Page } from "@playwright/test";

const shots = ["hero", "capture", "dictate", "draw", "models", "dark"];

/** Wait until the stage and every window inside it have painted with their fonts. */
async function staged(page: Page, shot: string) {
  await page.goto(`/src/dev/stage.html?shot=${shot}`);
  await expect(page.locator("h1")).not.toBeEmpty();
  await page.locator("iframe").first().waitFor();
  for (const frame of page.frames()) {
    if (frame === page.mainFrame()) continue;
    await frame.locator("#root > *").first().waitFor();
    await frame.evaluate(() => document.fonts.ready);
  }
  await page.evaluate(() => document.fonts.ready);
}

// The launch gallery: 16:9 at 1920 × 1080.
test.describe("launch gallery", () => {
  // The hero frame composes three live windows; give the set room to paint.
  test.describe.configure({ timeout: 90_000 });
  test.use({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1.2 });
  for (const [index, shot] of shots.entries()) {
    test(`renders ${shot} from the product windows`, async ({ page }, info) => {
      await staged(page, shot);
      const png = await page.screenshot({ path: info.outputPath(`${index + 1}-${shot}.png`) });
      expect(png.readUInt32BE(16)).toBe(1920);
      expect(png.readUInt32BE(20)).toBe(1080);
    });
  }
});

// The Microsoft Store set: the same frames at 3200 × 1800.
test.describe("store screenshots", () => {
  test.describe.configure({ timeout: 90_000 });
  test.use({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 2 });
  for (const [index, shot] of shots.entries()) {
    test(`renders ${shot} for the Store`, async ({ page }, info) => {
      await staged(page, shot);
      const png = await page.screenshot({ path: info.outputPath(`Screenshot ${index + 1} - ${shot}.png`) });
      expect(png.readUInt32BE(16)).toBe(3200);
      expect(png.readUInt32BE(20)).toBe(1800);
    });
  }
});
