/**
 * The tabs and the split panes of a terminal window (F108, F109). Each pane has its own
 * terminal in the agent. The window keeps the layout in its state, thus a reload and a
 * second browser tab show the same tabs and panes. This module has no DOM code.
 *
 * The first pane has the id of the window: a window from before the tabs keeps its shell.
 * A new pane gets `<window id>/<n>`.
 */

import type { PtyStart } from "../../proto";

/** A pane, or two parts of a tab side by side ("row") or one above the other ("column"). */
export type PaneNode =
  /** A pane. `start` is what its terminal starts, for a terminal bookmark (F110): ssh or a folder. */
  | { readonly pane: string; readonly start?: PtyStart }
  | {
      /** The id of the split: the id of the pane that the split made. */
      readonly split: string;
      readonly dir: "row" | "column";
      /** The part of the first child, from 0 to 1. */
      readonly ratio: number;
      readonly a: PaneNode;
      readonly b: PaneNode;
    };

export interface Tab {
  /** The id of the tab: the id of its first pane. It stays when that pane closes. */
  readonly id: string;
  readonly root: PaneNode;
  /** The pane of the tab that has the focus. */
  readonly focus: string;
}

export interface TabLayout {
  /** The tabs, in the order of the tab bar. */
  readonly tabs: readonly Tab[];
  /** The id of the tab that shows. */
  readonly active: string;
  /** The number of the next new pane. */
  readonly next: number;
}

/** The smallest part of a split. */
export const MIN_RATIO = 0.1;

const clampRatio = (ratio: number) => Math.min(1 - MIN_RATIO, Math.max(MIN_RATIO, ratio));

/** Returns the layout of a new window: one tab with one pane with the id of the window, and its start. */
export function initialLayout(windowId: string, start?: PtyStart): TabLayout {
  const root: PaneNode = start ? { pane: windowId, start } : { pane: windowId };
  return { tabs: [{ id: windowId, root, focus: windowId }], active: windowId, next: 2 };
}

/** Returns a valid start of a pane, or undefined. */
export function parseStart(value: unknown): PtyStart | undefined {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  if (record.type === "ssh" && typeof record.url === "string") return { type: "ssh", url: record.url };
  if (record.type === "folder" && typeof record.path === "string") return { type: "folder", path: record.path };
  return undefined;
}

/** Returns the start of the pane `pane`, or undefined for the login shell in the home folder. */
export function startOf(layout: TabLayout, pane: string): PtyStart | undefined {
  const find = (node: PaneNode): PtyStart | undefined => ("pane" in node ? (node.pane === pane ? node.start : undefined) : (find(node.a) ?? find(node.b)));
  for (const tab of layout.tabs) {
    const start = find(tab.root);
    if (start) return start;
  }
  return undefined;
}

/** Reports whether `id` is a terminal of the window `windowId`. */
export function isTabOf(id: string, windowId: string): boolean {
  return id === windowId || id.startsWith(`${windowId}/`);
}

/** Returns the panes of a node, from the left or the top. */
export function panesOf(node: PaneNode): string[] {
  return "pane" in node ? [node.pane] : [...panesOf(node.a), ...panesOf(node.b)];
}

/** Returns the panes of all tabs. */
export function allPanes(layout: TabLayout): string[] {
  return layout.tabs.flatMap((tab) => panesOf(tab.root));
}

/** Returns the tab that has the pane `pane`. */
export function tabOfPane(layout: TabLayout, pane: string): Tab | undefined {
  return layout.tabs.find((tab) => panesOf(tab.root).includes(pane));
}

/** Returns the tab that shows. */
export function activeTab(layout: TabLayout): Tab {
  return layout.tabs.find((tab) => tab.id === layout.active) ?? layout.tabs[0]!;
}

/** Returns a valid node, or null. `seen` collects the panes, and a pane counts once. */
function parseNode(value: unknown, windowId: string, seen: Set<string>, depth = 0): PaneNode | null {
  if (typeof value === "string") value = { pane: value };
  if (typeof value !== "object" || value === null || depth > 16) return null;
  const record = value as Record<string, unknown>;
  if (typeof record.pane === "string") {
    if (!isTabOf(record.pane, windowId) || seen.has(record.pane)) return null;
    seen.add(record.pane);
    const start = parseStart(record.start);
    return start ? { pane: record.pane, start } : { pane: record.pane };
  }
  const a = parseNode(record.a, windowId, seen, depth + 1);
  const b = parseNode(record.b, windowId, seen, depth + 1);
  if (!a || !b) return a ?? b;
  const dir = record.dir === "column" ? "column" : "row";
  const ratio = typeof record.ratio === "number" && Number.isFinite(record.ratio) ? clampRatio(record.ratio) : 0.5;
  const split = typeof record.split === "string" ? record.split : panesOf(b)[0]!;
  return { split, dir, ratio, a, b };
}

/**
 * Returns a valid layout from a value of the state store, or the initial layout.
 * A list of terminal ids, as F108 stored it, gives one tab for each.
 */
export function parseLayout(value: unknown, windowId: string): TabLayout {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  const seen = new Set<string>();
  const tabs: Tab[] = [];
  for (const each of Array.isArray(record.tabs) ? record.tabs : []) {
    const tab: Record<string, unknown> = typeof each === "object" && each !== null ? (each as Record<string, unknown>) : { root: each };
    const root = parseNode(tab.root, windowId, seen);
    if (!root) continue;
    const panes = panesOf(root);
    const id = typeof tab.id === "string" && !tabs.some((other) => other.id === tab.id) ? tab.id : panes[0]!;
    const focus = typeof tab.focus === "string" && panes.includes(tab.focus) ? tab.focus : panes[0]!;
    tabs.push({ id, root, focus });
  }
  if (tabs.length === 0) return initialLayout(windowId);
  const active = typeof record.active === "string" ? (tabs.find((tab) => tab.id === record.active || panesOf(tab.root).includes(record.active as string))?.id ?? tabs[0]!.id) : tabs[0]!.id;
  const used = Math.max(1, ...[...seen].map((id) => Number(id.slice(windowId.length + 1)) || 1));
  const next = typeof record.next === "number" && Number.isInteger(record.next) && record.next > used ? record.next : used + 1;
  return { tabs, active, next };
}

const withTab = (layout: TabLayout, id: string, change: (tab: Tab) => Tab): TabLayout => ({ ...layout, tabs: layout.tabs.map((tab) => (tab.id === id ? change(tab) : tab)) });

/** Returns the layout with a new tab, which becomes active. `nextTo` puts it after the active tab, else at the end. */
export function addTab(layout: TabLayout, windowId: string, nextTo: boolean): TabLayout {
  const id = `${windowId}/${layout.next}`;
  const tabs = [...layout.tabs];
  const index = nextTo ? tabs.findIndex((tab) => tab.id === layout.active) + 1 : tabs.length;
  tabs.splice(index, 0, { id, root: { pane: id }, focus: id });
  return { tabs, active: id, next: layout.next + 1 };
}

/**
 * Returns the layout without the tab `id`. If it was active, the tab on its right becomes
 * active, else the tab on its left. The last tab stays: a window has at least one tab.
 */
export function closeTab(layout: TabLayout, id: string): TabLayout {
  const index = layout.tabs.findIndex((tab) => tab.id === id);
  if (index < 0 || layout.tabs.length === 1) return layout;
  const tabs = layout.tabs.filter((tab) => tab.id !== id);
  const active = layout.active === id ? tabs[Math.min(index, tabs.length - 1)]!.id : layout.active;
  return { ...layout, tabs, active };
}

/** Returns the layout with the tab `id` active. */
export function selectTab(layout: TabLayout, id: string): TabLayout {
  return layout.tabs.some((tab) => tab.id === id) ? { ...layout, active: id } : layout;
}

/** Returns the layout with the tab `step` places from the active tab active, with wrap-around. */
export function stepTab(layout: TabLayout, step: number): TabLayout {
  const count = layout.tabs.length;
  const index = (((layout.tabs.findIndex((tab) => tab.id === layout.active) + step) % count) + count) % count;
  return { ...layout, active: layout.tabs[index]!.id };
}

function replaceNode(node: PaneNode, pane: string, by: (leaf: PaneNode) => PaneNode): PaneNode {
  if ("pane" in node) return node.pane === pane ? by(node) : node;
  return { ...node, a: replaceNode(node.a, pane, by), b: replaceNode(node.b, pane, by) };
}

/** Returns the layout with the pane `pane` split in two. The new pane is right of it or below it, and has the focus. */
export function splitPane(layout: TabLayout, windowId: string, pane: string, dir: "row" | "column"): TabLayout {
  const tab = tabOfPane(layout, pane);
  if (!tab) return layout;
  const id = `${windowId}/${layout.next}`;
  const root = replaceNode(tab.root, pane, (leaf) => ({ split: id, dir, ratio: 0.5, a: leaf, b: { pane: id } }));
  return { ...withTab(layout, tab.id, () => ({ ...tab, root, focus: id })), active: tab.id, next: layout.next + 1 };
}

function removePane(node: PaneNode, pane: string): PaneNode | null {
  if ("pane" in node) return node.pane === pane ? null : node;
  const a = removePane(node.a, pane);
  const b = removePane(node.b, pane);
  if (!a || !b) return a ?? b;
  return { ...node, a, b };
}

/**
 * Returns the layout without the pane `pane`. Its neighbor gets its space and, if the
 * pane had the focus, the focus. The last pane of a tab closes the tab.
 */
export function closePane(layout: TabLayout, pane: string): TabLayout {
  const tab = tabOfPane(layout, pane);
  if (!tab) return layout;
  const root = removePane(tab.root, pane);
  if (!root) return closeTab(layout, tab.id);
  const panes = panesOf(tab.root);
  const left = panesOf(root);
  const index = panes.indexOf(pane);
  const neighbor = left[Math.max(0, Math.min(index, left.length) - 1)] ?? left[0]!;
  const focus = tab.focus === pane ? neighbor : tab.focus;
  return withTab(layout, tab.id, () => ({ ...tab, root, focus }));
}

/** Returns the layout with the focus on the pane `pane`, and its tab active. */
export function focusPane(layout: TabLayout, pane: string): TabLayout {
  const tab = tabOfPane(layout, pane);
  if (!tab || (tab.focus === pane && layout.active === tab.id)) return layout;
  return { ...withTab(layout, tab.id, () => ({ ...tab, focus: pane })), active: tab.id };
}

function setRatio(node: PaneNode, split: string, ratio: number): PaneNode {
  if ("pane" in node) return node;
  if (node.split === split) return { ...node, ratio: clampRatio(ratio) };
  return { ...node, a: setRatio(node.a, split, ratio), b: setRatio(node.b, split, ratio) };
}

/** Returns the layout with a new ratio for the split `split` of the active tab. */
export function resizeSplit(layout: TabLayout, split: string, ratio: number): TabLayout {
  return withTab(layout, layout.active, (tab) => ({ ...tab, root: setRatio(tab.root, split, ratio) }));
}

/** A rectangle in parts of the tab area, from 0 to 1. */
export interface Box {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

/** The place of each pane and each divider of a tab. */
export interface Geometry {
  readonly panes: ReadonlyMap<string, Box>;
  /** A divider is on the border between the two parts of the split, which fills `box`. */
  readonly dividers: readonly { readonly split: string; readonly dir: "row" | "column"; readonly ratio: number; readonly box: Box }[];
}

/** Returns the place of each pane and each divider of a node in `box`. */
export function geometry(node: PaneNode, box: Box = { x: 0, y: 0, w: 1, h: 1 }): Geometry {
  const panes = new Map<string, Box>();
  const dividers: Geometry["dividers"][number][] = [];
  const walk = (each: PaneNode, area: Box) => {
    if ("pane" in each) {
      panes.set(each.pane, area);
      return;
    }
    dividers.push({ split: each.split, dir: each.dir, ratio: each.ratio, box: area });
    if (each.dir === "row") {
      const w = area.w * each.ratio;
      walk(each.a, { ...area, w });
      walk(each.b, { ...area, x: area.x + w, w: area.w - w });
    } else {
      const h = area.h * each.ratio;
      walk(each.a, { ...area, h });
      walk(each.b, { ...area, y: area.y + h, h: area.h - h });
    }
  };
  walk(node, box);
  return { panes, dividers };
}

/**
 * Returns the pane next to `pane` in the direction, or undefined. Of the panes on that side
 * that overlap it, the nearest wins.
 */
export function paneInDirection(node: PaneNode, pane: string, direction: "left" | "right" | "up" | "down"): string | undefined {
  const { panes } = geometry(node);
  const from = panes.get(pane);
  if (!from) return undefined;
  const eps = 1e-6;
  let best: { id: string; distance: number } | undefined;
  for (const [id, box] of panes) {
    if (id === pane) continue;
    const horizontal = direction === "left" || direction === "right";
    const overlaps = horizontal ? box.y < from.y + from.h - eps && box.y + box.h > from.y + eps : box.x < from.x + from.w - eps && box.x + box.w > from.x + eps;
    if (!overlaps) continue;
    const distance =
      direction === "left" ? from.x - (box.x + box.w) : direction === "right" ? box.x - (from.x + from.w) : direction === "up" ? from.y - (box.y + box.h) : box.y - (from.y + from.h);
    if (distance < -eps) continue;
    if (!best || distance < best.distance) best = { id, distance };
  }
  return best?.id;
}
