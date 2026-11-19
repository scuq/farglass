import { afterEach, expect, test } from "vitest";
import { applyAppearance, DEFAULT_APPEARANCE, isDarkTheme, parseAppearance } from "./appearance";

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
  applyAppearance({ theme: "dark", accent: "green", textSize: 125 });
  expect([root.dataset.theme, root.dataset.accent, root.style.fontSize]).toEqual(["dark", "green", "125%"]);
  expect(isDarkTheme()).toBe(true);
  applyAppearance(DEFAULT_APPEARANCE);
  expect([root.dataset.theme, root.style.fontSize]).toEqual([undefined, ""]);
  expect(changes).toBe(2);
});
