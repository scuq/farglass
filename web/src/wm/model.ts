/**
 * The state of the desktop and the actions on it. Each action is a pure
 * function that returns a new state, thus the tests need no DOM.
 */

import { clampRect, type Rect, type SnapZone, snapRect, tileRects } from "./geometry";

/** One window. */
export interface Win {
  readonly id: string;
  /** The app that the window shows. */
  readonly app: string;
  readonly title: string;
  /** The floating position and size. */
  readonly rect: Rect;
  /** The snap zone, or `null` for a floating window. */
  readonly snap: SnapZone | null;
  readonly minimized: boolean;
  /** The workspace, from 0. */
  readonly workspace: number;
  /** The window opened behind with no focus, and the user did not look at it yet (F041). */
  readonly attention?: boolean;
}

/** The state of the desktop. */
export interface Desktop {
  /** The windows in stack order: the last one is on top. */
  readonly windows: readonly Win[];
  /** The window ids in the order of their last use: the first one has the focus. */
  readonly recent: readonly string[];
  /** The workspace that shows, from 0. */
  readonly workspace: number;
  /** The number of workspaces. */
  readonly workspaces: number;
}

/** The default number of workspaces. */
export const WORKSPACES = 4;

/** The step between two new windows in a cascade. */
const CASCADE = 32;

/** Returns an empty desktop. */
export function emptyDesktop(): Desktop {
  return { windows: [], recent: [], workspace: 0, workspaces: WORKSPACES };
}

/** Returns the window with the id `id`. */
export function windowById(desktop: Desktop, id: string): Win | undefined {
  return desktop.windows.find((win) => win.id === id);
}

/** Returns the id of the window that has the focus, or `null`. */
export function focused(desktop: Desktop): string | null {
  const id = desktop.recent[0];
  const win = id === undefined ? undefined : windowById(desktop, id);
  return win && !win.minimized && win.workspace === desktop.workspace ? win.id : null;
}

/** Returns the rectangle where a window shows in `area`. */
export function shownRect(win: Win, area: Rect): Rect {
  return win.snap ? snapRect(win.snap, area) : clampRect(win.rect, area);
}

/** Returns the windows of the current workspace in the order of their last use, also minimized ones. */
export function switchable(desktop: Desktop): string[] {
  return desktop.recent.filter((id) => windowById(desktop, id)?.workspace === desktop.workspace);
}

function update(desktop: Desktop, id: string, change: (win: Win) => Win): Desktop {
  return { ...desktop, windows: desktop.windows.map((win) => (win.id === id ? change(win) : win)) };
}

/**
 * Opens a new window on the current workspace, on top, with the focus. With `focus: false`, the
 * window opens under the window with the focus, is the last in the order of use, and has a mark (F041).
 */
export function openWindow(
  desktop: Desktop,
  init: { id: string; app: string; title: string; w: number; h: number },
  area: Rect,
  options: { focus?: boolean } = {},
): Desktop {
  const onWorkspace = desktop.windows.filter((win) => win.workspace === desktop.workspace && !win.minimized).length;
  const step = (onWorkspace % 8) * CASCADE;
  const rect = clampRect({ x: area.x + CASCADE + step, y: area.y + CASCADE + step, w: init.w, h: init.h }, area);
  const win: Win = {
    id: init.id,
    app: init.app,
    title: init.title,
    rect,
    snap: null,
    minimized: false,
    workspace: desktop.workspace,
  };
  const top = focused(desktop);
  if (options.focus === false && top) {
    const below = desktop.windows.findIndex((other) => other.id === top);
    return {
      ...desktop,
      windows: [...desktop.windows.slice(0, below), { ...win, attention: true }, ...desktop.windows.slice(below)],
      recent: [...desktop.recent, win.id],
    };
  }
  return { ...desktop, windows: [...desktop.windows, win], recent: [win.id, ...desktop.recent] };
}

/** Closes a window. The next window of the workspace gets the focus. */
export function closeWindow(desktop: Desktop, id: string): Desktop {
  return {
    ...desktop,
    windows: desktop.windows.filter((win) => win.id !== id),
    recent: desktop.recent.filter((other) => other !== id),
  };
}

/** Changes the title of a window. */
export function retitleWindow(desktop: Desktop, id: string, title: string): Desktop {
  const win = windowById(desktop, id);
  return !win || win.title === title ? desktop : update(desktop, id, (other) => ({ ...other, title }));
}

/** Gives a window the focus: on top, not minimized, and its workspace shows. */
export function focusWindow(desktop: Desktop, id: string): Desktop {
  const win = windowById(desktop, id);
  if (!win) {
    return desktop;
  }
  const { attention: _seen, ...rest } = win;
  const shown = { ...rest, minimized: false };
  return {
    ...desktop,
    windows: [...desktop.windows.filter((other) => other.id !== id), shown],
    recent: [id, ...desktop.recent.filter((other) => other !== id)],
    workspace: win.workspace,
  };
}

/** Sets the floating position and size of a window. The window is no longer snapped. */
export function placeWindow(desktop: Desktop, id: string, rect: Rect): Desktop {
  return update(desktop, id, (win) => ({ ...win, rect, snap: null }));
}

/** Snaps a window to `zone`. The floating rectangle stays for the restore. */
export function snapWindow(desktop: Desktop, id: string, zone: SnapZone): Desktop {
  return update(desktop, id, (win) => ({ ...win, snap: zone }));
}

/** Maximizes a floating window, or restores a maximized or snapped window. */
export function toggleMaximize(desktop: Desktop, id: string): Desktop {
  return update(desktop, id, (win) => ({ ...win, snap: win.snap === "maximized" ? null : "maximized" }));
}

/** Minimizes a window. The next window of the workspace gets the focus. */
export function minimizeWindow(desktop: Desktop, id: string): Desktop {
  const minimized = update(desktop, id, (win) => ({ ...win, minimized: true }));
  const next = switchable(minimized).find((other) => !windowById(minimized, other)?.minimized);
  // The minimized window goes to the end of the order of use.
  const recent = [...minimized.recent.filter((other) => other !== id), id];
  const result = { ...minimized, recent };
  return next ? focusWindow(result, next) : result;
}

/** Shows the workspace `index`. Its last used window gets the focus. */
export function switchWorkspace(desktop: Desktop, index: number): Desktop {
  const workspace = Math.min(Math.max(index, 0), desktop.workspaces - 1);
  const shown = { ...desktop, workspace };
  const next = switchable(shown).find((id) => !windowById(shown, id)?.minimized);
  return next ? focusWindow(shown, next) : shown;
}

/** Moves a window to the workspace `index`, and the focus with it. */
export function moveToWorkspace(desktop: Desktop, id: string, index: number): Desktop {
  const workspace = Math.min(Math.max(index, 0), desktop.workspaces - 1);
  return focusWindow(
    update(desktop, id, (win) => ({ ...win, workspace })),
    id,
  );
}

/**
 * Arranges the windows of the current workspace that are not minimized in
 * columns. The window with the focus gets the first place.
 */
export function tileWorkspace(desktop: Desktop, area: Rect): Desktop {
  const ids = switchable(desktop).filter((id) => !windowById(desktop, id)?.minimized);
  const rects = tileRects(ids.length, area);
  let result = desktop;
  ids.forEach((id, index) => {
    result = placeWindow(result, id, rects[index]!);
  });
  return result;
}

/**
 * Returns a desktop from a value of the state store, or `null` if the value is
 * not a desktop. A different client can have a different screen, thus the
 * windows show through `shownRect`, which clamps them.
 */
export function parseDesktop(value: unknown): Desktop | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const candidate = value as Partial<Desktop>;
  if (!Array.isArray(candidate.windows) || !Array.isArray(candidate.recent)) {
    return null;
  }
  const windows = candidate.windows.filter(
    (win): win is Win =>
      !!win &&
      typeof win.id === "string" &&
      typeof win.app === "string" &&
      typeof win.title === "string" &&
      !!win.rect &&
      [win.rect.x, win.rect.y, win.rect.w, win.rect.h].every(Number.isFinite),
  );
  const ids = new Set(windows.map((win) => win.id));
  const workspaces = Number.isInteger(candidate.workspaces) ? Math.max(1, candidate.workspaces!) : WORKSPACES;
  return {
    windows,
    recent: candidate.recent.filter((id): id is string => typeof id === "string" && ids.has(id)),
    workspace: Number.isInteger(candidate.workspace) ? Math.min(Math.max(candidate.workspace!, 0), workspaces - 1) : 0,
    workspaces,
  };
}
