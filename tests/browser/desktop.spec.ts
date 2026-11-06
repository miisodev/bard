import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.afterEach(async ({ page }, info) => {
  await page.screenshot({ path: info.outputPath("window.png") });
});

for (const theme of ["light", "dark"]) {
  for (const size of [{ width: 620, height: 640 }, { width: 480, height: 440 }]) {
    test(`${theme} onboarding keeps each step above its actions at ${size.width}×${size.height}`, async ({ page }, info) => {
      await page.setViewportSize(size);
      await page.goto(`/?scene=onboarding&platform=linux&theme=${theme}&bare=1&fail=set_autostart`);
      const headings = ["Clipboard, Draw and Dictate.", "Offline voice", "Make it yours", "You're set"];
      for (let step = 0; step < headings.length; step++) {
        await expect(page.getByRole("heading", { name: headings[step], exact: true })).toBeVisible();
        const layout = await page.evaluate(() => {
          const copy = document.querySelector(".onb-copy")!.getBoundingClientRect();
          const body = document.querySelector(".onb-step")!.getBoundingClientRect();
          const foot = document.querySelector(".onb-foot")!.getBoundingClientRect();
          return { copyBottom: copy.bottom, bodyTop: body.top, bodyBottom: body.bottom, footTop: foot.top, footBottom: foot.bottom, width: document.documentElement.scrollWidth, viewport: innerWidth, height: innerHeight };
        });
        expect(layout.bodyTop).toBeGreaterThanOrEqual(layout.copyBottom);
        expect(layout.footTop).toBeGreaterThanOrEqual(layout.bodyBottom);
        expect(layout.footBottom).toBeLessThanOrEqual(layout.height);
        expect(layout.width).toBeLessThanOrEqual(layout.viewport);
        const { violations } = await new AxeBuilder({ page }).analyze();
        expect(violations.filter((v) => v.impact === "critical" || v.impact === "serious")).toEqual([]);
        await page.screenshot({ path: info.outputPath(`step-${step + 1}.png`) });
        if (step < 3) await page.getByRole("button", { name: step === 1 ? "Skip for now" : "Continue", exact: true }).click();
      }
      await page.getByRole("button", { name: "Start using Bard" }).click();
      await expect(page.getByRole("alert")).toBeVisible();
      await expect(page.getByRole("button", { name: "Start using Bard" })).toBeEnabled();
    });
  }
}

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
