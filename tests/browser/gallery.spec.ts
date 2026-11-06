import { expect, test } from "@playwright/test";

test.use({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1.2 });

const shots = ["hero", "capture", "dictate", "draw", "models", "dark"];
for (const [index, shot] of shots.entries()) {
  test(`launch gallery renders ${shot} from the product windows`, async ({ page }, info) => {
    await page.goto(`/src/dev/stage.html?shot=${shot}`);
    await expect(page.locator("h1")).not.toBeEmpty();
    await page.locator("iframe").first().waitFor();
    for (const frame of page.frames()) {
      if (frame === page.mainFrame()) continue;
      await frame.locator("#root > *").first().waitFor();
      await frame.evaluate(() => document.fonts.ready);
    }
    await page.evaluate(() => document.fonts.ready);
    const png = await page.screenshot({ path: info.outputPath(`${index + 1}-${shot}.png`) });
    expect(png.readUInt32BE(16)).toBe(1920);
    expect(png.readUInt32BE(20)).toBe(1080);
  });
}
