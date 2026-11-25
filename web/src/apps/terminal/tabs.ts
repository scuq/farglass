/**
 * The tabs of a terminal window (F108). Each tab has its own terminal in the agent.
 * The window keeps the list in its state, thus a reload and a second browser tab show
 * the same tabs. This module has no DOM code.
 *
 * The first tab has the id of the window: a window from before the tabs keeps its shell.
 * A new tab gets `<window id>/<n>`.
 */

export interface TabLayout {
  /** The terminal ids of the tabs, in the order of the tab bar. */
  readonly tabs: readonly string[];
  /** The terminal id of the tab that shows. */
  readonly active: string;
  /** The number of the next new tab. */
  readonly next: number;
}

/** Returns the layout of a new window: one tab with the id of the window. */
export function initialLayout(windowId: string): TabLayout {
  return { tabs: [windowId], active: windowId, next: 2 };
}

/** Reports whether `id` is a terminal of the window `windowId`. */
export function isTabOf(id: string, windowId: string): boolean {
  return id === windowId || id.startsWith(`${windowId}/`);
}

/** Returns the window of a terminal id. */
export function windowOfTerminal(id: string): string {
  const slash = id.indexOf("/");
  return slash < 0 ? id : id.slice(0, slash);
}

/** Returns a valid layout from a value of the state store, or the initial layout. */
export function parseLayout(value: unknown, windowId: string): TabLayout {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  const tabs = Array.isArray(record.tabs) ? [...new Set(record.tabs.filter((id): id is string => typeof id === "string" && isTabOf(id, windowId)))] : [];
  if (tabs.length === 0) return initialLayout(windowId);
  const active = typeof record.active === "string" && tabs.includes(record.active) ? record.active : tabs[0]!;
  const used = Math.max(1, ...tabs.map((id) => Number(id.slice(windowId.length + 1)) || 1));
  const next = typeof record.next === "number" && Number.isInteger(record.next) && record.next > used ? record.next : used + 1;
  return { tabs, active, next };
}

/** Returns the layout with a new tab, which becomes active. `nextTo` puts it after the active tab, else at the end. */
export function addTab(layout: TabLayout, windowId: string, nextTo: boolean): TabLayout {
  const id = `${windowId}/${layout.next}`;
  const tabs = [...layout.tabs];
  tabs.splice(nextTo ? tabs.indexOf(layout.active) + 1 : tabs.length, 0, id);
  return { tabs, active: id, next: layout.next + 1 };
}

/**
 * Returns the layout without the tab `id`. If it was active, the tab on its right becomes
 * active, else the tab on its left. The last tab stays: a window has at least one tab.
 */
export function closeTab(layout: TabLayout, id: string): TabLayout {
  const index = layout.tabs.indexOf(id);
  if (index < 0 || layout.tabs.length === 1) return layout;
  const tabs = layout.tabs.filter((each) => each !== id);
  const active = layout.active === id ? tabs[Math.min(index, tabs.length - 1)]! : layout.active;
  return { ...layout, tabs, active };
}

/** Returns the layout with the tab `id` active. */
export function selectTab(layout: TabLayout, id: string): TabLayout {
  return layout.tabs.includes(id) ? { ...layout, active: id } : layout;
}

/** Returns the layout with the tab `step` places from the active tab active, with wrap-around. */
export function stepTab(layout: TabLayout, step: number): TabLayout {
  const count = layout.tabs.length;
  const index = (((layout.tabs.indexOf(layout.active) + step) % count) + count) % count;
  return { ...layout, active: layout.tabs[index]! };
}
