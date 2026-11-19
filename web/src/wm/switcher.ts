/**
 * The window switcher (Alt+Tab). It shows the windows of the workspace in the
 * order of their last use. Each Tab selects the next window, and the release
 * of Alt gives the selected window the focus.
 */

/** The state of an open switcher. */
export interface Switcher {
  readonly items: readonly string[];
  readonly index: number;
}

/**
 * Opens the switcher on `items` and selects the next window: the window that
 * had the focus before the current one. With `backward`, it selects the last window.
 * Returns `null` if there is no window.
 */
export function openSwitcher(items: readonly string[], backward = false): Switcher | null {
  if (items.length === 0) {
    return null;
  }
  const start = { items, index: 0 };
  return items.length === 1 ? start : step(start, backward);
}

/** Selects the next window, or the previous one with `backward`. The selection wraps. */
export function step(switcher: Switcher, backward = false): Switcher {
  const count = switcher.items.length;
  return { ...switcher, index: (switcher.index + (backward ? count - 1 : 1)) % count };
}

/** Returns the selected window. */
export function selected(switcher: Switcher): string {
  return switcher.items[switcher.index]!;
}
