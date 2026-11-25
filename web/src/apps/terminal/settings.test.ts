import { describe, expect, it } from "vitest";
import { DEFAULT_TERMINAL, fontFamily, PROFILE_IDS, PROFILES, paletteOf, parseTerminal } from "./settings";

describe("terminal settings", () => {
  it("gives the default for a missing or unknown value", () => {
    expect(parseTerminal(undefined)).toEqual(DEFAULT_TERMINAL);
    expect(parseTerminal({ colors: "neon", font: 3, fontSize: 99, scrollback: -1 })).toEqual(DEFAULT_TERMINAL);
  });

  it("keeps valid values", () => {
    const value = { colors: "nord", font: "Fira Code", fontSize: 16, scrollback: 50_000, tabBar: "always", onExit: "keep", newTab: "next" };
    expect(parseTerminal(value)).toEqual(value);
  });

  it("has 16 colors in each profile, and none for the theme", () => {
    for (const profile of Object.values(PROFILES)) expect(profile.palette.ansi).toHaveLength(16);
    expect(paletteOf("theme")).toBeUndefined();
    expect(PROFILE_IDS[0]).toBe("theme");
  });

  it("falls back to the monospace stack after a font", () => {
    expect(fontFamily("Hack")).toMatch(/^"Hack", ui-monospace/);
    expect(fontFamily("default")).toMatch(/^ui-monospace/);
  });
});
