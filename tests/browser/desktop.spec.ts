import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.afterEach(async ({ page }, info) => {
  await page.screenshot({ path: info.outputPath("window.png") });
});

test("Wayland shortcut help describes desktop permissions", async ({ page }) => {
  await page.goto("/?platform=linux");
  await page.getByRole("listbox", { name: "Cards" }).waitFor();
  await page.keyboard.press("Control+/");
  const guide = page.getByRole("dialog", { name: "Shortcuts" });
  await expect(guide).toBeVisible();
  await expect(guide).toContainText("Your desktop assigns the global shortcuts");
  await expect(guide).not.toContainText("Left Shift");
  await page.keyboard.press("Escape");
  await expect(guide).not.toBeVisible();
});

test("startup permission failure is visible and recoverable", async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 620 });
  await page.goto("/settings.html?pane=general&fail=set_autostart");
  const toggle = page.getByRole("switch", { name: "Start at login" });
  await toggle.click();
  await expect(page.getByRole("alert")).toContainText("Couldn’t save");
  await expect(toggle).toBeEnabled();
});

for (const theme of ["light", "dark"]) {
  test(`${theme} settings at minimum size have no serious accessibility violations`, async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 440 });
    await page.goto(`/settings.html?theme=${theme}`);
    await page.getByRole("heading", { name: "General" }).waitFor();
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.filter((v) => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}

for (const pane of ["capture", "dictation", "models", "postprocessing", "shortcuts", "data", "about"]) {
  test(`${pane} settings are accessible on Linux at minimum size`, async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 440 });
    await page.goto(`/settings.html?pane=${pane}&platform=linux`);
    await page.getByRole("tabpanel").waitFor();
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.filter((v) => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}
