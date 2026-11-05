import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.afterEach(async ({ page }, info) => {
  await page.screenshot({ path: info.outputPath("window.png") });
});

test("Wayland shortcut help describes desktop permissions", async ({ page }) => {
  await page.goto("/?platform=linux&bare=1");
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
  await page.goto("/settings.html?pane=general&fail=set_autostart&bare=1");
  const toggle = page.getByRole("switch", { name: "Start at login" });
  await toggle.click();
  await expect(page.getByRole("alert")).toContainText("Couldn’t save");
  await expect(toggle).toBeEnabled();
});

test("a refused note creation preserves its draft", async ({ page }) => {
  await page.goto("/?bare=1&fail=add_item");
  const draft = page.getByRole("textbox", { name: "New note" });
  await draft.fill("Keep this draft when the disk refuses a write");
  await page.getByRole("button", { name: "Add note", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: "Couldn’t save" })).toBeVisible();
  await expect(draft).toHaveValue("Keep this draft when the disk refuses a write");
  await expect(draft).toBeEditable();
});

test("a refused edit keeps the editor open and Escape returns to the shelf", async ({ page }) => {
  await page.goto("/?bare=1&fail=update_item");
  const card = page.getByRole("option").first();
  await card.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Edit", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Edit note" });
  await dialog.getByRole("textbox").fill("Keep the edited draft");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog.getByRole("alert")).toBeVisible();
  await expect(dialog.getByRole("textbox")).toHaveValue("Keep the edited draft");
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await expect(page.getByRole("listbox", { name: "Cards" })).toBeVisible();
});

for (const theme of ["light", "dark"]) {
  test(`${theme} settings at minimum size have no serious accessibility violations`, async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 440 });
    await page.goto(`/settings.html?theme=${theme}&bare=1`);
    await page.getByRole("heading", { name: "General" }).waitFor();
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.filter((v) => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}

for (const pane of ["capture", "dictation", "models", "postprocessing", "shortcuts", "data", "about"]) {
  test(`${pane} settings are accessible on Linux at minimum size`, async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 440 });
    await page.goto(`/settings.html?pane=${pane}&platform=linux&bare=1`);
    await page.getByRole("tabpanel").waitFor();
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.filter((v) => v.impact === "critical" || v.impact === "serious")).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}
