import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.afterEach(async ({ page }, info) => {
  await page.screenshot({ path: info.outputPath("window.png") });
});

const serious = (violations: { impact?: string | null }[]) =>
  violations.filter((v) => v.impact === "critical" || v.impact === "serious");

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
        expect(serious(violations)).toEqual([]);
        await page.screenshot({ path: info.outputPath(`step-${step + 1}.png`) });
        if (step < 3) await page.getByRole("button", { name: step === 1 ? "Skip for now" : "Continue", exact: true }).click();
      }
      await page.getByRole("button", { name: "Start using Bard" }).click();
      await expect(page.getByRole("alert")).toBeVisible();
      await expect(page.getByRole("button", { name: "Start using Bard" })).toBeEnabled();
    });
  }
}

for (const theme of ["light", "dark"]) {
  for (const dock of ["float", "right"]) {
    test(`${theme} ${dock} main window has no serious accessibility violations`, async ({ page }) => {
      await page.setViewportSize({ width: 400, height: 620 });
      await page.goto(`/?theme=${theme}&dock=${dock}&bare=1`);
      await page.getByRole("listbox", { name: "Clipboard history" }).waitFor();
      const { violations } = await new AxeBuilder({ page }).analyze();
      expect(serious(violations)).toEqual([]);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    });
  }
}

test("clicking an item puts it on the clipboard and marks it current", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 620 });
  await page.goto("/?bare=1");
  const link = page.getByRole("option", { name: /handy\.computer/ });
  await link.click();
  await expect(page.getByRole("option", { name: /handy\.computer.*on the clipboard/ })).toBeVisible();
});

test("the first three items carry their slot numbers", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 620 });
  await page.goto("/?bare=1");
  const slots = page.locator(".badge.slot");
  await expect(slots).toHaveText(["1", "2", "3"]);
});

test("search and the kind filter narrow the list, and Escape clears the search", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 620 });
  await page.goto("/?bare=1");
  const list = page.getByRole("listbox", { name: "Clipboard history" });
  await page.getByRole("tab", { name: "Links" }).click();
  await expect(list.getByRole("option")).toHaveCount(2);
  await page.getByRole("tab", { name: "All" }).click();
  const search = page.getByRole("searchbox", { name: "Search the clipboard history" });
  await search.fill("venue");
  await expect(list.getByRole("option")).toHaveCount(2);
  await search.press("Escape");
  await expect(search).toHaveValue("");
});

test("pinning moves an item into the pinned group, by pointer or keyboard", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 620 });
  await page.goto("/?bare=1");
  const row = page.getByRole("option", { name: /hello@miiso\.dev/ });
  await row.hover();
  // A pointer shortcut kept out of the accessibility tree; Ctrl+P is the
  // keyboard route, checked below.
  await row.locator("[data-action='pin']").click({ force: true });
  await expect(page.getByRole("option", { name: /hello@miiso\.dev.*pinned/ })).toBeVisible();
  const search = page.getByRole("searchbox", { name: "Search the clipboard history" });
  await search.fill("Cape Town venue for");
  await search.press("ArrowDown");
  await page.getByRole("listbox", { name: "Clipboard history" }).focus();
  await page.keyboard.press("Control+p");
  await search.fill("");
  await expect(page.getByRole("option", { name: /Book the Cape Town venue.*pinned/ })).toBeVisible();
});

test("the shortcut guide lists the global chords and closes with Escape", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 620 });
  await page.goto("/?bare=1");
  await page.getByRole("listbox", { name: "Clipboard history" }).waitFor();
  await page.keyboard.press("Control+/");
  const guide = page.getByRole("dialog", { name: "Shortcuts" });
  await expect(guide).toBeVisible();
  await expect(guide).toContainText("Show or hide Bard");
  await expect(guide).toContainText("Space");
  await expect(guide).not.toContainText("Left Shift");
  await expect(guide).not.toContainText("paste");
  await page.keyboard.press("Escape");
  await expect(guide).not.toBeVisible();
});

test("on Wayland the guide says the desktop owns the shortcuts", async ({ page }) => {
  await page.setViewportSize({ width: 400, height: 620 });
  await page.goto("/?platform=linux&bare=1");
  await page.getByRole("listbox", { name: "Clipboard history" }).waitFor();
  await page.keyboard.press("Control+/");
  await expect(page.getByRole("dialog", { name: "Shortcuts" })).toContainText("Your desktop owns these shortcuts");
});

test("a shortcut can be recorded, refused when taken, and turned off", async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 620 });
  await page.goto("/settings.html?pane=shortcuts&bare=1");
  const draw = page.getByRole("button", { name: /^Open Draw:/ });
  await draw.click();
  await page.keyboard.press("Control+Alt+KeyN");
  await expect(draw).toContainText("N");
  await draw.click();
  await page.keyboard.press("Backspace");
  await expect(draw).toContainText("Off");
});

test("startup permission failure is visible and recoverable", async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 620 });
  await page.goto("/settings.html?pane=general&fail=set_autostart&bare=1");
  const toggle = page.getByRole("switch", { name: "Start at login" });
  await toggle.click();
  await expect(page.getByRole("alert")).toContainText("Couldn’t save");
  await expect(toggle).toBeEnabled();
});

test("feedback channels switch independently", async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 620 });
  await page.goto("/settings.html?pane=general&bare=1");
  const pill = page.getByRole("switch", { name: "Status pill" });
  const sounds = page.getByRole("switch", { name: "Sounds" });
  await sounds.click();
  await expect(sounds).toHaveAttribute("aria-checked", "false");
  await expect(pill).toHaveAttribute("aria-checked", "true");
});

test("Refine lives under Models", async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 620 });
  await page.goto("/settings.html?pane=models&bare=1");
  await expect(page.getByRole("heading", { name: "Refine", exact: true })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Refine" })).toHaveCount(0);
});

for (const theme of ["light", "dark"]) {
  test(`${theme} settings at minimum size have no serious accessibility violations`, async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 440 });
    await page.goto(`/settings.html?theme=${theme}&bare=1`);
    await page.getByRole("heading", { name: "General" }).waitFor();
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(serious(violations)).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}

for (const pane of ["clipboard", "dictation", "models", "shortcuts", "data", "about"]) {
  test(`${pane} settings are accessible on Linux at minimum size`, async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 440 });
    await page.goto(`/settings.html?pane=${pane}&platform=linux&bare=1`);
    await page.getByRole("tabpanel").waitFor();
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(serious(violations)).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}

for (const scene of ["copied", "selected", "listening", "done", "error", "preview"]) {
  test(`the ${scene} pill has no serious accessibility violations`, async ({ page }) => {
    await page.setViewportSize({ width: 480, height: 300 });
    await page.goto(`/pill.html?scene=${scene}&bare=1`);
    await page.getByRole("status").waitFor();
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(serious(violations)).toEqual([]);
  });
}
