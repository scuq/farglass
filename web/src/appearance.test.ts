import { afterEach, expect, test, vi } from "vitest";
import { applyAppearance, DEFAULT_APPEARANCE, isDarkTheme, isLcarsTheme, parseAppearance } from "./appearance";

afterEach(() => {
  const root = document.documentElement;
  delete root.dataset.theme;
  delete root.dataset.accent;
  root.style.fontSize = "";
});

test("a stored value that is not valid gives the default for each field", () => {
  expect(parseAppearance(null)).toEqual(DEFAULT_APPEARANCE);
  expect(parseAppearance({ theme: "dark", accent: "pink", textSize: 300 })).toEqual({ ...DEFAULT_APPEARANCE, theme: "dark" });
});

test("the appearance goes to the root element, and the system theme removes the attribute", () => {
  const root = document.documentElement;
  let changes = 0;
  document.addEventListener("farglass-theme", () => (changes += 1));
  applyAppearance({ theme: "dark", accent: "green", textSize: 125, motion: "system", panel: "bar" });
  expect([root.dataset.theme, root.dataset.accent, root.style.fontSize]).toEqual(["dark", "green", "125%"]);
  expect(isDarkTheme()).toBe(true);
  applyAppearance({ ...DEFAULT_APPEARANCE, theme: "system" });
  expect([root.dataset.theme, root.style.fontSize]).toEqual([undefined, ""]);
  expect(changes).toBe(2);
});

test("the motion follows the client, a choice overrides it, and a change of the client applies with no reload", async () => {
  const root = document.documentElement;
  let reduce = false;
  const listeners: (() => void)[] = [];
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("reduced-motion") ? reduce : false,
    addEventListener: (_: string, listener: () => void) => listeners.push(listener),
  }));
  // A new copy of the module: it listens to the client one time, and the other tests listened already.
  vi.resetModules();
  const fresh = await import("./appearance");
  try {
    fresh.applyAppearance(DEFAULT_APPEARANCE);
    expect(root.dataset.motion).toBe("full");
    reduce = true;
    for (const listener of listeners) listener();
    expect(root.dataset.motion).toBe("reduced");
    expect(fresh.isReducedMotion()).toBe(true);
    fresh.applyAppearance({ ...DEFAULT_APPEARANCE, motion: "full" });
    expect(root.dataset.motion).toBe("full");
    fresh.applyAppearance({ ...DEFAULT_APPEARANCE, motion: "reduced" });
    reduce = false;
    for (const listener of listeners) listener();
    expect(root.dataset.motion).toBe("reduced");
  } finally {
    vi.unstubAllGlobals();
  }
});

test("the LCARS theme is the default, and it is a dark theme", () => {
  const root = document.documentElement;
  expect(DEFAULT_APPEARANCE.theme).toBe("lcars");
  applyAppearance(DEFAULT_APPEARANCE);
  expect([root.dataset.theme, root.dataset.accent]).toEqual(["lcars", "orange"]);
  expect(isLcarsTheme()).toBe(true);
  expect(isDarkTheme()).toBe(true);
});

test("the dock layout of the panel is a choice, and the bar is the default (F098)", () => {
  expect(parseAppearance({}).panel).toBe("bar");
  expect(parseAppearance({ panel: "dock" }).panel).toBe("dock");
  expect(parseAppearance({ panel: "bottom" }).panel).toBe("bar");
  applyAppearance({ ...DEFAULT_APPEARANCE, panel: "dock" });
  expect(document.documentElement.dataset.panel).toBe("dock");
  applyAppearance(DEFAULT_APPEARANCE);
  expect(document.documentElement.dataset.panel).toBe("bar");
});
