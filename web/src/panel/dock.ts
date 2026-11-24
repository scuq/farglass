/**
 * The items of the dock (F098): one item for each app, with its windows.
 * The pinned apps come first, in the order of the pins. The other apps with
 * windows follow, in the order in which their first window opened.
 */

import type { Win } from "../wm/model";

/** One app of the dock. */
export interface DockItem {
  /** The id of the entry of the app in the launcher, for example `terminal` or `host:org.gnome.Calculator`. */
  readonly key: string;
  readonly pinned: boolean;
  /** The ids of the windows of the app, the last used first. */
  readonly windows: readonly string[];
}

/** The most dots under an icon. */
export const MAX_DOTS = 4;

/**
 * Returns the items of the dock and the new order of the apps that are not pinned.
 * `order` is the order of the last call: an app keeps its place while it has windows.
 */
export function dockItems(
  windows: readonly Win[],
  recent: readonly string[],
  pinned: readonly string[],
  order: readonly string[],
  keyOf: (win: Win) => string,
): { items: DockItem[]; order: string[] } {
  const rank = (id: string) => {
    const index = recent.indexOf(id);
    return index === -1 ? recent.length : index;
  };
  const byKey = new Map<string, string[]>();
  for (const win of windows) {
    const key = keyOf(win);
    byKey.set(key, [...(byKey.get(key) ?? []), win.id]);
  }
  for (const ids of byKey.values()) ids.sort((a, b) => rank(a) - rank(b));

  const running = [...byKey.keys()].filter((key) => !pinned.includes(key));
  const nextOrder = [...order.filter((key) => running.includes(key)), ...running.filter((key) => !order.includes(key))];
  const items: DockItem[] = [
    ...pinned.map((key) => ({ key, pinned: true, windows: byKey.get(key) ?? [] })),
    ...nextOrder.map((key) => ({ key, pinned: false, windows: byKey.get(key) ?? [] })),
  ];
  return { items, order: nextOrder };
}

/**
 * Returns the window that a click on `item` shows: the last used window, or,
 * if a window of the app has the focus, the next window of the app. The next
 * window is in a fixed order, so that more clicks show each window.
 * Returns null for an app with no window.
 */
export function windowToShow(item: DockItem, focused: string | null): string | null {
  if (item.windows.length === 0) return null;
  if (focused === null || !item.windows.includes(focused)) return item.windows[0] ?? null;
  const fixed = [...item.windows].sort();
  return fixed[(fixed.indexOf(focused) + 1) % fixed.length] ?? null;
}
