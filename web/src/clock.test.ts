import { expect, test } from "vitest";
import { crockfordStart, crockfordTimestamp, DEFAULT_CLOCK, formatClock, parseClockSettings } from "./clock";

test("the examples of the specification give their codes", () => {
  const examples: [string, string][] = [
    ["2020-01-01T00:00:00Z", "01100"],
    ["2026-06-29T08:05:00Z", "66X82"],
    ["2026-06-29T10:05:00+02:00", "66X82"],
    ["2026-01-01T00:30:00+01:00", "5CZQF"],
    ["2031-10-10T23:59:59Z", "BAAQX"],
    ["2052-01-01T00:00:00Z", "01100+1"],
    ["2100-03-15T12:34:56Z", "G3FCH+2"],
    ["3043-12-31T23:59:59Z", "ZCZQX+Z"],
  ];
  for (const [input, code] of examples) expect(crockfordTimestamp(new Date(input)), input).toBe(code);
});

test("a year out of the range gives no code", () => {
  expect(crockfordTimestamp(new Date("2019-12-31T23:59:59Z"))).toBeNull();
  expect(crockfordTimestamp(new Date("3044-01-01T00:00:00Z"))).toBeNull();
  expect(crockfordTimestamp(new Date(Number.NaN))).toBeNull();
});

test("the tooltip time is the UTC start of the 2-minute interval", () => {
  expect(crockfordStart(new Date("2026-06-29T10:05:31+02:00"))).toBe("2026-06-29 08:04");
});

test("the hour format gives 24 hours or 12 hours", () => {
  const date = new Date(2026, 5, 29, 14, 5);
  expect(formatClock(date, "24", "en-US")).toBe("14:05");
  expect(formatClock(date, "12", "de-DE")).toMatch(/^2:05\s?PM$/);
  expect(formatClock(date, "12", "en-US")).toMatch(/^2:05\sPM$/);
});

test("stored settings that are not valid give the default", () => {
  expect(parseClockSettings(null)).toEqual(DEFAULT_CLOCK);
  expect(parseClockSettings({ hours: "36", crockford: "yes" })).toEqual(DEFAULT_CLOCK);
  expect(parseClockSettings({ hours: "12", crockford: true })).toEqual({ hours: "12", crockford: true });
});
