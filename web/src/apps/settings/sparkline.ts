/** The geometry of the graphs of the resource page (F105). This module has no DOM code. */

/** The points of a history of 5 minutes, with one point every 2 s. */
export const HISTORY_SLOTS = 150;

/** The paths of one graph in a box of `width` and `height`. Empty strings with fewer than two values. */
export interface SparkPaths {
  /** The line through the values. */
  readonly line: string;
  /** The area between the line and the bottom of the box. */
  readonly area: string;
}

const round = (value: number) => Math.round(value * 100) / 100;

/** Returns the x position of the value `index` of `count` values. The newest value is at the right edge. */
export function sparkX(index: number, count: number, slots: number, width: number): number {
  return round(((slots - count + index) / (slots - 1)) * width);
}

/**
 * Returns the paths of `values`, the oldest first, on a scale from 0 to `max`.
 * The box has `slots` positions, and the newest value is at the right edge: a short
 * history grows from the right. A value above `max` stays in the box.
 */
export function sparkPaths(values: readonly number[], max: number, slots: number, width: number, height: number): SparkPaths {
  const shown = values.slice(-slots);
  if (shown.length < 2 || max <= 0) {
    return { line: "", area: "" };
  }
  // The line keeps 1 unit from the top and the bottom, thus its stroke stays in the box.
  const y = (value: number) => round(height - 1 - (Math.min(Math.max(value, 0), max) / max) * (height - 2));
  const points = shown.map((value, index) => `${sparkX(index, shown.length, slots, width)} ${y(value)}`);
  const line = `M${points.join("L")}`;
  const first = sparkX(0, shown.length, slots, width);
  return { line, area: `${line}L${width} ${height}L${first} ${height}Z` };
}

/** Returns a round top of a scale for `values`: 1, 2, or 5 times a power of ten, and `floor` at least. */
export function niceMax(values: readonly number[], floor: number): number {
  const peak = Math.max(floor, ...values);
  const power = 10 ** Math.floor(Math.log10(peak));
  const step = [1, 2, 5, 10].find((each) => each * power >= peak) ?? 10;
  return step * power;
}

/** Returns the index of the value at the position `x` of a box of `width`, or null left of the first value. */
export function sparkIndex(x: number, count: number, slots: number, width: number): number | null {
  if (count === 0 || width <= 0) return null;
  const index = Math.round((x / width) * (slots - 1)) - (slots - count);
  return index < 0 ? null : Math.min(index, count - 1);
}

/** Returns a time of `seconds` as text: "40 s", "5 min", or "2 min 10 s". */
export function sparkSpan(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return [minutes > 0 ? `${minutes} min` : "", rest > 0 || minutes === 0 ? `${rest} s` : ""].filter(Boolean).join(" ");
}

/** Returns the age of the value `index` of `count` values as text, with `seconds` between two values. */
export function sparkAge(index: number, count: number, seconds: number): string {
  const age = (count - 1 - index) * seconds;
  return age <= 0 ? "now" : `${sparkSpan(age)} ago`;
}
