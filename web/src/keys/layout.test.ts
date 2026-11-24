import { describe, expect, it } from "vitest";
import { isLayoutId, LAYOUTS, layoutFromLanguages, xkbOf } from "./layout";

describe("layoutFromLanguages", () => {
  it("gives the layout of the language", () => {
    expect(layoutFromLanguages(["de-DE"])).toBe("de");
    expect(layoutFromLanguages(["fr"])).toBe("fr");
    expect(layoutFromLanguages(["en-US"])).toBe("us");
    expect(layoutFromLanguages(["sv-SE"])).toBe("se");
  });

  it("prefers the layout of the region", () => {
    expect(layoutFromLanguages(["de-AT"])).toBe("at");
    expect(layoutFromLanguages(["de-CH"])).toBe("ch");
    expect(layoutFromLanguages(["fr-CH"])).toBe("ch(fr)");
    expect(layoutFromLanguages(["en-GB"])).toBe("gb");
    expect(layoutFromLanguages(["pt-BR"])).toBe("br");
    expect(layoutFromLanguages(["es-ES"])).toBe("es");
    expect(layoutFromLanguages(["es-MX"])).toBe("latam");
    expect(layoutFromLanguages(["sr-Latn-RS"])).toBe("rs");
  });

  it("takes the first language with a layout", () => {
    expect(layoutFromLanguages(["xx", "it-IT", "en"])).toBe("it");
  });

  it("falls back to US", () => {
    expect(layoutFromLanguages([])).toBe("us");
    expect(layoutFromLanguages(["xx-YY"])).toBe("us");
  });

  it("gives only layouts of the list", () => {
    const ids = new Set(LAYOUTS.map((layout) => layout.id));
    for (const language of ["de", "de-at", "fr-ch", "es", "nl", "ar", "ja", "uk", "sq", "en-ie"]) {
      expect(ids.has(layoutFromLanguages([language]))).toBe(true);
    }
  });
});

describe("layout ids", () => {
  it("splits the layout and the variant", () => {
    expect(xkbOf("de")).toEqual({ layout: "de", variant: "" });
    expect(xkbOf("ch(fr)")).toEqual({ layout: "ch", variant: "fr" });
  });

  it("accepts only XKB names", () => {
    expect(isLayoutId("us(intl)")).toBe(true);
    expect(isLayoutId("de,us")).toBe(false);
    expect(isLayoutId("US")).toBe(false);
    expect(isLayoutId(3)).toBe(false);
    expect(LAYOUTS.every((layout) => isLayoutId(layout.id))).toBe(true);
  });
});
