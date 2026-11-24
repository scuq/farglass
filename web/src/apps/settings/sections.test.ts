import { describe, expect, test } from "vitest";
import { PAGES } from "./pages";
import { matchSections, pagesOf, SECTIONS } from "./sections";

describe("sections", () => {
  test("each card has its own id and a page of the list", () => {
    const ids = SECTIONS.map((section) => section.id);
    expect(new Set(ids).size).toBe(ids.length);
    const pages = new Set<string>(PAGES.map((page) => page.id));
    expect(SECTIONS.filter((section) => !pages.has(section.page))).toEqual([]);
    expect(PAGES.filter((page) => !SECTIONS.some((section) => section.page === page.id))).toEqual([]);
  });

  test("an empty query is no filter", () => {
    expect(matchSections("")).toBeNull();
    expect(matchSections("   ")).toBeNull();
  });

  test("each word of the query must match, with no difference of case", () => {
    expect([...matchSections("TimeZone")!]).toEqual(["datetime-host"]);
    expect([...matchSections("clock 24")!]).toEqual(["datetime-clock"]);
    expect(matchSections("clock hostname")!.size).toBe(0);
  });

  test("the label of a page finds each card of the page", () => {
    const matched = matchSections("users and sessions")!;
    expect([...matched].sort()).toEqual(["users-sessions", "users-users"]);
    expect(pagesOf(matched)).toEqual(["users"]);
  });

  test("the pages of a match keep the order of the list", () => {
    expect(pagesOf(new Set(["power-actions", "appearance-theme", "datetime-host"] as const))).toEqual(["appearance", "datetime", "power"]);
    expect(pagesOf(new Set())).toEqual([]);
  });
});
