import { describe, expect, it } from "vitest";
import { colorValue, linkParts } from "./content";

describe("colorValue", () => {
  it("recognises a hex colour on its own", () => {
    expect(colorValue("#2f6fed")).toBe("#2f6fed");
    expect(colorValue("  #FFF \n")).toBe("#FFF");
    expect(colorValue("rgb(47, 111, 237)")).toBe("rgb(47, 111, 237)");
  });

  it("leaves prose that mentions a colour alone", () => {
    expect(colorValue("the accent is #2f6fed")).toBeNull();
    expect(colorValue("#2f6fedd0x")).toBeNull();
    expect(colorValue("#hashtag")).toBeNull();
  });
});

describe("linkParts", () => {
  it("puts the site first and drops www", () => {
    expect(linkParts("https://www.handy.computer/docs/models")).toEqual({
      host: "handy.computer",
      rest: "/docs/models",
    });
  });

  it("keeps a query and a fragment, and nothing for a bare root", () => {
    expect(linkParts("https://miiso.dev/?a=1#b")).toEqual({ host: "miiso.dev", rest: "?a=1#b" });
    expect(linkParts("https://miiso.dev/")).toEqual({ host: "miiso.dev", rest: "" });
  });

  it("is not fooled by a sentence containing a link", () => {
    expect(linkParts("see https://miiso.dev")).toBeNull();
    expect(linkParts("https://miiso.dev and more")).toBeNull();
    expect(linkParts("ftp://miiso.dev")).toBeNull();
  });
});
