import { describe, expect, it } from "vitest";
import { updateDelivery } from "./distribution";

describe("update delivery", () => {
  it("never offers a Windows installer to a Linux copy", () => {
    expect(updateDelivery({ platform: "linux", channel: "direct" }).inApp).toBe(false);
    expect(updateDelivery({ platform: "linux", channel: "direct" }).label).toBe("Linux download");
  });
  it("lets each store own its updates", () => {
    expect(updateDelivery({ platform: "windows", channel: "store" }).inApp).toBe(false);
    expect(updateDelivery({ platform: "linux", channel: "flatpak" }).label).toBe("Flathub");
  });
  it("preserves signed in-app updates for direct Windows installations", () => {
    expect(updateDelivery({ platform: "windows", channel: "direct" }).inApp).toBe(true);
    expect(updateDelivery({ channel: "direct" }).inApp).toBe(true);
  });
});
