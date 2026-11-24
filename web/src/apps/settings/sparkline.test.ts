import { describe, expect, test } from "vitest";
import { niceMax, sparkAge, sparkIndex, sparkPaths, sparkSpan, sparkX } from "./sparkline";

describe("sparkline", () => {
  test("the newest value is at the right edge, and a short history grows from the right", () => {
    expect(sparkX(2, 3, 5, 100)).toBe(100);
    expect(sparkX(0, 3, 5, 100)).toBe(50);
    expect(sparkX(0, 5, 5, 100)).toBe(0);
  });

  test("the line goes through the values, and the area closes at the bottom", () => {
    const { line, area } = sparkPaths([0, 50, 100], 100, 5, 100, 12);
    expect(line).toBe("M50 11L75 6L100 1");
    expect(area).toBe("M50 11L75 6L100 1L100 12L50 12Z");
  });

  test("fewer than two values give no path, and a value above the scale stays in the box", () => {
    expect(sparkPaths([5], 10, 5, 100, 12)).toEqual({ line: "", area: "" });
    expect(sparkPaths([0, 400], 100, 2, 100, 12).line).toBe("M0 11L100 1");
    expect(sparkPaths([1, 2, 3, 4], 4, 2, 100, 12).line).toBe("M0 3.5L100 1");
  });

  test("the top of the scale is round and not below the floor", () => {
    expect(niceMax([0, 0.2], 5)).toBe(5);
    expect(niceMax([12, 37], 5)).toBe(50);
    expect(niceMax([150_000], 10_000)).toBe(200_000);
    expect(niceMax([], 10)).toBe(10);
    expect(niceMax([100], 5)).toBe(100);
  });

  test("a position gives the value below it, and its age", () => {
    expect(sparkIndex(100, 3, 5, 100)).toBe(2);
    expect(sparkIndex(50, 3, 5, 100)).toBe(0);
    expect(sparkIndex(10, 3, 5, 100)).toBeNull();
    expect(sparkAge(2, 3, 2)).toBe("now");
    expect(sparkAge(0, 3, 2)).toBe("4 s ago");
    expect(sparkAge(0, 61, 2)).toBe("2 min ago");
    expect(sparkAge(0, 66, 2)).toBe("2 min 10 s ago");
    expect([sparkSpan(0), sparkSpan(40), sparkSpan(298)]).toEqual(["0 s", "40 s", "4 min 58 s"]);
  });
});
