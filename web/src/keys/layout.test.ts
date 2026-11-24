import { afterEach, describe, expect, it, vi } from "vitest";
import { detectLayout, isLayoutId, LAYOUTS, layoutFromMap, resolveLayout, xkbOf } from "./layout";

const US: Record<string, string> = {
  KeyQ: "q",
  KeyW: "w",
  KeyE: "e",
  KeyA: "a",
  KeyS: "s",
  KeyD: "d",
  KeyY: "y",
  KeyZ: "z",
  Digit0: "0",
  Digit1: "1",
  Digit2: "2",
  Digit3: "3",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Backslash: "\\",
};

/** A layout map of the Keyboard API: the US keys with some keys changed. */
const map = (changes: Record<string, string> = {}) => new Map(Object.entries({ ...US, ...changes }));

describe("layoutFromMap", () => {
  it("finds the layout from the characters of the keys", () => {
    expect(layoutFromMap(map())).toBe("us");
    expect(layoutFromMap(map({ Backslash: "#" }))).toBe("gb");
    expect(layoutFromMap(map({ KeyY: "z", KeyZ: "y", Semicolon: "ö", Quote: "ä", BracketLeft: "ü", Backslash: "#" }))).toBe("de");
    expect(layoutFromMap(map({ KeyY: "z", KeyZ: "y", Semicolon: "ö", Quote: "ä", Backquote: "§", Backslash: "$" }))).toBe("ch");
    expect(layoutFromMap(map({ KeyY: "z", KeyZ: "y", Semicolon: "é", Quote: "à", Backquote: "§" }))).toBe("ch(fr)");
    expect(layoutFromMap(map({ KeyQ: "a", KeyA: "q", Digit1: "&", Equal: "=" }))).toBe("fr");
    expect(layoutFromMap(map({ KeyQ: "a", KeyA: "q", Digit1: "&", Equal: "-" }))).toBe("be");
    expect(layoutFromMap(map({ Semicolon: "ö", Quote: "ä", BracketLeft: "å" }))).toBe("se");
    expect(layoutFromMap(map({ Semicolon: "ñ", Equal: "¡" }))).toBe("es");
    expect(layoutFromMap(map({ KeyQ: "й", KeyS: "ы", KeyA: "ф" }))).toBe("ru");
    expect(layoutFromMap(map({ KeyD: "e", KeyS: "o", KeyQ: "'" }))).toBe("us(dvorak)");
  });

  it("ignores the case of a character", () => {
    expect(layoutFromMap(map({ KeyY: "Z", Semicolon: "Ö", Quote: "Ä" }))).toBe("de");
  });

  it("gives null for an unknown layout", () => {
    expect(layoutFromMap(new Map())).toBeNull();
    expect(layoutFromMap(map({ KeyQ: "?", Semicolon: "?" }))).toBeNull();
  });
});

describe("detectLayout", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("uses the Keyboard API", async () => {
    vi.stubGlobal("navigator", { keyboard: { getLayoutMap: async () => map({ Backslash: "#" }) } });
    expect(await detectLayout()).toBe("gb");
    expect(await resolveLayout("auto")).toBe("gb");
  });

  it("gives US with no Keyboard API, and does not guess from the language", async () => {
    vi.stubGlobal("navigator", { language: "de-AT", languages: ["de-AT"] });
    expect(await detectLayout()).toBeNull();
    expect(await resolveLayout("auto")).toBe("us");
  });

  it("gives US when the browser refuses the call", async () => {
    vi.stubGlobal("navigator", { keyboard: { getLayoutMap: () => Promise.reject(new Error("SecurityError")) } });
    expect(await resolveLayout("auto")).toBe("us");
  });

  it("keeps a layout that the user selected", async () => {
    vi.stubGlobal("navigator", { keyboard: { getLayoutMap: async () => map({ Backslash: "#" }) } });
    expect(await resolveLayout("ch(fr)")).toBe("ch(fr)");
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
