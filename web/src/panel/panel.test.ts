// @vitest-environment node
import { describe, expect, test } from "vitest";
import { isLauncherKey, SuperTap } from "../wm/keys";
import { search } from "./search";

const apps = [
  { id: "echo", title: "Echo", keywords: ["test", "channel"] },
  { id: "about", title: "About farglass", keywords: ["version", "info"] },
  { id: "terminal", title: "Terminal", keywords: ["shell", "console"] },
  { id: "settings", title: "System settings", keywords: ["preferences"] },
];

describe("search", () => {
  test("an empty query gives all entries in title order", () => {
    expect(search(apps, "  ").map((a) => a.id)).toEqual(["about", "echo", "settings", "terminal"]);
  });

  test("a title that starts with the query comes first, then a word of the title", () => {
    // "System settings" starts with "s". The others match only inside a word or a keyword.
    expect(search(apps, "s").map((a) => a.id)).toEqual(["settings", "about", "echo", "terminal"]);
    expect(search(apps, "set")[0]!.id).toBe("settings");
    expect(search(apps, "farg")[0]!.id).toBe("about");
  });

  test("keywords and ids match, and each word of the query must match", () => {
    expect(search(apps, "SHELL").map((a) => a.id)).toEqual(["terminal"]);
    expect(search(apps, "version about").map((a) => a.id)).toEqual(["about"]);
    expect(search(apps, "echo version")).toEqual([]);
    expect(search(apps, "xyz")).toEqual([]);
  });
});

describe("keys of the launcher", () => {
  test("a tap of Super is Super down and up with no other key", () => {
    const tap = new SuperTap();
    tap.down({ key: "Meta" });
    expect(tap.up({ key: "Meta" })).toBe(true);

    tap.down({ key: "Meta" });
    tap.down({ key: "ArrowUp" });
    expect(tap.up({ key: "Meta" })).toBe(false);

    tap.down({ key: "Meta" });
    tap.reset();
    expect(tap.up({ key: "Meta" })).toBe(false);
    expect(tap.up({ key: "a" })).toBe(false);
  });

  test("Alt+F1 opens the launcher with no Keyboard Lock", () => {
    const base = { altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
    expect(isLauncherKey({ ...base, code: "F1", altKey: true })).toBe(true);
    expect(isLauncherKey({ ...base, code: "F1" })).toBe(false);
    expect(isLauncherKey({ ...base, code: "F1", altKey: true, ctrlKey: true })).toBe(false);
  });

  test("Alt+Shift+Space opens the launcher with no Keyboard Lock", () => {
    const base = { altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
    expect(isLauncherKey({ ...base, code: "Space", altKey: true, shiftKey: true })).toBe(true);
    expect(isLauncherKey({ ...base, code: "Space", shiftKey: true })).toBe(false);
    expect(isLauncherKey({ ...base, code: "Space", altKey: true })).toBe(false);
  });
});
