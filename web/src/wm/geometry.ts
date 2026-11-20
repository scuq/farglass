/** The geometry of windows: rectangles, snap zones, size changes, and the tile layout. */

/** A rectangle in CSS pixels, relative to the desktop. */
export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

/** The smallest window size. */
export const MIN_SIZE = { w: 240, h: 160 } as const;

/** The height of a title bar. A window always keeps its title bar on the desktop. */
export const TITLE_HEIGHT = 32;

/** The part of a window that stays on the desktop when the user moves it to a side. */
const VISIBLE_PART = 96;

/** The distance from an edge, in pixels, at which a drag snaps. */
export const SNAP_EDGE = 12;

/** The space between tiled windows. */
const TILE_GAP = 8;

/** A place where a window snaps. `maximized` is the full desktop. */
export type SnapZone = "maximized" | "left" | "right" | "top-left" | "top-right" | "bottom-left" | "bottom-right";

/** An edge or a corner of a window that the user drags to change its size. */
export type Edge = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

/** Returns `rect` with a size that fits `area` and a position that keeps the title bar reachable. */
export function clampRect(rect: Rect, area: Rect): Rect {
  const w = Math.min(Math.max(rect.w, MIN_SIZE.w), area.w);
  const h = Math.min(Math.max(rect.h, MIN_SIZE.h), area.h);
  const x = Math.min(Math.max(rect.x, area.x - w + VISIBLE_PART), area.x + area.w - VISIBLE_PART);
  const y = Math.min(Math.max(rect.y, area.y), area.y + area.h - TITLE_HEIGHT);
  return { x, y, w, h };
}

/** Returns the snap zone of a pointer at (`px`, `py`), or `null` if the pointer is not at an edge. */
export function snapZoneAt(px: number, py: number, area: Rect, edge = SNAP_EDGE): SnapZone | null {
  const left = px <= area.x + edge;
  const right = px >= area.x + area.w - edge;
  const top = py <= area.y + edge;
  const corner = area.h / 4;
  if (left || right) {
    const side = left ? "left" : "right";
    if (py <= area.y + corner) {
      return `top-${side}`;
    }
    if (py >= area.y + area.h - corner) {
      return `bottom-${side}`;
    }
    return side;
  }
  return top ? "maximized" : null;
}

/** Returns the rectangle of a snap zone in `area`. */
export function snapRect(zone: SnapZone, area: Rect): Rect {
  const halfW = Math.floor(area.w / 2);
  const halfH = Math.floor(area.h / 2);
  const right = area.x + halfW;
  const bottom = area.y + halfH;
  switch (zone) {
    case "maximized":
      return area;
    case "left":
      return { x: area.x, y: area.y, w: halfW, h: area.h };
    case "right":
      return { x: right, y: area.y, w: area.w - halfW, h: area.h };
    case "top-left":
      return { x: area.x, y: area.y, w: halfW, h: halfH };
    case "top-right":
      return { x: right, y: area.y, w: area.w - halfW, h: halfH };
    case "bottom-left":
      return { x: area.x, y: bottom, w: halfW, h: area.h - halfH };
    case "bottom-right":
      return { x: right, y: bottom, w: area.w - halfW, h: area.h - halfH };
  }
}

/**
 * Returns `start` after a drag of `edge` by (`dx`, `dy`). The opposite edge stays,
 * and the size does not go below the smallest window size.
 */
export function resizeRect(start: Rect, edge: Edge, dx: number, dy: number): Rect {
  let { x, y, w, h } = start;
  if (edge.includes("e")) {
    w = Math.max(MIN_SIZE.w, start.w + dx);
  }
  if (edge.includes("s")) {
    h = Math.max(MIN_SIZE.h, start.h + dy);
  }
  if (edge.includes("w")) {
    w = Math.max(MIN_SIZE.w, start.w - dx);
    x = start.x + start.w - w;
  }
  if (edge.includes("n")) {
    h = Math.max(MIN_SIZE.h, start.h - dy);
    y = start.y + start.h - h;
  }
  return { x, y, w, h };
}

/**
 * Returns `count` rectangles that fill `area` in columns, with no overlap.
 * The number of columns is the square root of `count`, rounded up. The first
 * columns get the extra windows.
 */
export function tileRects(count: number, area: Rect, gap = TILE_GAP): Rect[] {
  if (count <= 0) {
    return [];
  }
  const columns = Math.ceil(Math.sqrt(count));
  const base = Math.floor(count / columns);
  const extra = count % columns;
  const columnW = (area.w - gap * (columns + 1)) / columns;
  const rects: Rect[] = [];
  for (let column = 0; column < columns; column++) {
    const rows = base + (column < extra ? 1 : 0);
    const rowH = (area.h - gap * (rows + 1)) / rows;
    for (let row = 0; row < rows; row++) {
      rects.push({
        x: Math.round(area.x + gap + column * (columnW + gap)),
        y: Math.round(area.y + gap + row * (rowH + gap)),
        w: Math.round(columnW),
        h: Math.round(rowH),
      });
    }
  }
  return rects;
}

/** The layouts of the picker (F043). Columns is the layout of the Tile action. */
export const LAYOUTS = ["columns", "grid", "center", "wide", "spotlight"] as const;
export type Layout = (typeof LAYOUTS)[number];

/** The names of the layouts. */
export const LAYOUT_LABELS: Readonly<Record<Layout, string>> = {
  columns: "Columns",
  grid: "Grid",
  center: "Center",
  wide: "Wide",
  spotlight: "Spotlight",
};

/** Splits the rectangle `area` into `count` rows or columns with a gap between them. */
function split(area: Rect, count: number, vertical: boolean, gap: number): Rect[] {
  if (count <= 0) return [];
  const length = ((vertical ? area.h : area.w) - gap * (count - 1)) / count;
  return Array.from({ length: count }, (_, i) =>
    vertical
      ? { x: area.x, y: Math.round(area.y + i * (length + gap)), w: area.w, h: Math.round(length) }
      : { x: Math.round(area.x + i * (length + gap)), y: area.y, w: Math.round(length), h: area.h },
  );
}

/**
 * Returns `count` rectangles of `layout` in `area`. The first one is for the window with the focus.
 *
 * - columns: the columns of the Tile action.
 * - grid: rows and columns of the same size, the last row wider.
 * - center: the first window in the middle, the others in columns on the left and the right.
 * - wide: the first window across the top, the others in a row below.
 * - spotlight: the first window on the left, the others in a column on the right.
 */
export function layoutRects(layout: Layout, count: number, area: Rect, gap = TILE_GAP): Rect[] {
  if (count <= 0) return [];
  const inner = { x: area.x + gap, y: area.y + gap, w: area.w - 2 * gap, h: area.h - 2 * gap };
  if (layout === "columns") return tileRects(count, area, gap);
  if (layout === "grid") {
    const columns = Math.ceil(Math.sqrt(count));
    const rows = Math.ceil(count / columns);
    return split(inner, rows, true, gap).flatMap((row, index) =>
      split(row, index === rows - 1 ? count - columns * (rows - 1) : columns, false, gap),
    );
  }
  if (count === 1) {
    return layout === "center" ? [{ x: Math.round(inner.x + inner.w * 0.15), y: inner.y, w: Math.round(inner.w * 0.7), h: inner.h }] : [inner];
  }
  if (layout === "wide") {
    const mainH = Math.round(inner.h * 0.65);
    const below = { x: inner.x, y: inner.y + mainH + gap, w: inner.w, h: inner.h - mainH - gap };
    return [{ ...inner, h: mainH }, ...split(below, count - 1, false, gap)];
  }
  if (layout === "spotlight") {
    const mainW = Math.round(inner.w * 0.7);
    const side = { x: inner.x + mainW + gap, y: inner.y, w: inner.w - mainW - gap, h: inner.h };
    return [{ ...inner, w: mainW }, ...split(side, count - 1, true, gap)];
  }
  // center: the others alternate between the left and the right column.
  const sideW = Math.round((inner.w - 2 * gap) * 0.2);
  const main = { x: inner.x + sideW + gap, y: inner.y, w: inner.w - 2 * sideW - 2 * gap, h: inner.h };
  const others = count - 1;
  const left = split({ x: inner.x, y: inner.y, w: sideW, h: inner.h }, Math.ceil(others / 2), true, gap);
  const right = split({ x: main.x + main.w + gap, y: inner.y, w: sideW, h: inner.h }, Math.floor(others / 2), true, gap);
  const rest: Rect[] = [];
  for (let i = 0; i < others; i++) rest.push((i % 2 === 0 ? left[i / 2] : right[(i - 1) / 2])!);
  return [main, ...rest];
}

/** Returns the rectangle of a window that the user drags out of a maximized or snapped state. */
export function unsnapRect(restore: Rect, current: Rect, px: number, py: number): Rect {
  // Keep the pointer at the same relative place on the title bar.
  const ratio = current.w > 0 ? (px - current.x) / current.w : 0.5;
  return { x: Math.round(px - restore.w * ratio), y: Math.round(py - TITLE_HEIGHT / 2), w: restore.w, h: restore.h };
}
