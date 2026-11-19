/**
 * The controller of the desktop: the state, the actions, the keys, and the
 * persistence in the state store. The panel and the window surface share it.
 */

import { type Accessor, createMemo, createSignal, onCleanup } from "solid-js";
import { APPS } from "../apps/registry";
import type { Connection } from "../proto";
import { StateClient } from "../state";
import { clampRect, type Rect, type SnapZone } from "./geometry";
import { actionFor, isLauncherKey, SuperTap } from "./keys";
import {
  closeWindow,
  type Desktop,
  emptyDesktop,
  focused,
  focusWindow,
  minimizeWindow,
  moveToWorkspace,
  openWindow,
  parseDesktop,
  placeWindow,
  retitleWindow,
  snapWindow,
  switchable,
  switchWorkspace,
  tileWorkspace,
  toggleMaximize,
  windowById,
} from "./model";
import { openSwitcher, selected, step, type Switcher } from "./switcher";

/** The key of the desktop in the state store. All tabs of the user share it. */
const DESKTOP_KEY = "desktop";

/** The wait after the last change before the desktop goes to the state store. */
const SAVE_DELAY_MS = 250;

/** The desktop size before the first measurement. */
const DEFAULT_AREA: Rect = { x: 0, y: 0, w: 1280, h: 720 };

/** The state and the actions of the desktop. */
export interface DesktopController {
  readonly connection: Connection;
  /** The state channel of the desktop. The apps share it. */
  readonly state: StateClient;
  /** Records that the window `id` uses the state key `key`. Closing the window removes the key. */
  trackKey(id: string, key: string): void;
  /** Changes the title of a window. */
  setTitle(id: string, title: string): void;
  readonly desktop: Accessor<Desktop>;
  readonly area: Accessor<Rect>;
  readonly preview: Accessor<SnapZone | null>;
  readonly switcher: Accessor<Switcher | null>;
  readonly focusedId: Accessor<string | null>;
  /** Reports whether the launcher is open. */
  readonly launcherOpen: Accessor<boolean>;
  setLauncherOpen(open: boolean): void;
  /** Sets the element of the window surface, for its size and the keyboard focus. */
  attachSurface(element: HTMLElement): void;
  focus(id: string): void;
  /** Gives the window the focus and also the keyboard focus. */
  activate(id: string): void;
  close(id: string): void;
  minimize(id: string): void;
  toggleMaximize(id: string): void;
  place(id: string, rect: Rect, zone: SnapZone | null): void;
  setPreview(zone: SnapZone | null): void;
  openApp(appId: string): void;
  switchWorkspace(index: number): void;
  tile(): void;
}

/** Creates the controller. Call it in a component: the cleanup of the component stops it. */
export function createDesktopController(connection: Connection): DesktopController {
  const [desktop, setDesktop] = createSignal<Desktop>(emptyDesktop());
  const [area, setArea] = createSignal<Rect>(DEFAULT_AREA);
  const [preview, setPreview] = createSignal<SnapZone | null>(null);
  const [switcher, setSwitcher] = createSignal<Switcher | null>(null);
  const [launcherOpen, setLauncherOpen] = createSignal(false);
  const store = new StateClient(connection);
  const superTap = new SuperTap();
  let loaded = false;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let surface: HTMLElement | undefined;
  /** The state keys of each window. */
  const windowKeys = new Map<string, Set<string>>();

  /** Applies an action, and saves the result after a short wait. */
  const apply = (change: (state: Desktop) => Desktop) => {
    const next = change(desktop());
    if (next === desktop()) {
      return;
    }
    setDesktop(next);
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => store.set(DESKTOP_KEY, desktop()), SAVE_DELAY_MS);
  };

  const withApp = (state: Desktop, appId: string): Desktop => {
    const app = APPS[appId];
    if (!app) {
      return state;
    }
    // A second window of the same app gets a number, thus the task buttons differ.
    const titles = new Set(state.windows.map((win) => win.title));
    let title = app.title;
    for (let n = 2; titles.has(title); n++) {
      title = `${app.title} ${n}`;
    }
    return openWindow(state, { id: crypto.randomUUID(), app: app.id, title, ...app.size }, area());
  };

  store.watch(DESKTOP_KEY, (value) => {
    const restored = parseDesktop(value);
    if (restored) {
      setDesktop(restored);
    } else if (!loaded) {
      // A new session: open one window, thus the desktop is not empty.
      apply((state) => withApp(state, "echo"));
    }
    loaded = true;
  });

  const focusedId = createMemo(() => focused(desktop()));

  /** Gives the keyboard focus to the element of the window `id`. */
  const focusElement = (id: string | null) => {
    if (!id || !surface) {
      return;
    }
    const element = surface.querySelector<HTMLElement>(`[data-window="${CSS.escape(id)}"]`);
    if (element && !element.contains(document.activeElement)) {
      element.focus({ preventScroll: true });
    }
  };

  const controller: DesktopController = {
    connection,
    state: store,
    trackKey(id, key) {
      let keys = windowKeys.get(id);
      if (!keys) {
        keys = new Set();
        windowKeys.set(id, keys);
      }
      keys.add(key);
    },
    setTitle: (id, title) => apply((state) => retitleWindow(state, id, title)),
    desktop,
    area,
    preview,
    switcher,
    focusedId,
    launcherOpen,
    setLauncherOpen,
    attachSurface(element) {
      surface = element;
      resize.observe(element);
    },
    focus(id) {
      if (focusedId() !== id || windowById(desktop(), id)?.minimized) {
        apply((state) => focusWindow(state, id));
      }
    },
    activate(id) {
      apply((state) => focusWindow(state, id));
      queueMicrotask(() => focusElement(id));
    },
    close(id) {
      apply((state) => closeWindow(state, id));
      // A closed window does not come back, thus its state goes. A reconnect does not close windows.
      for (const key of windowKeys.get(id) ?? []) {
        store.set(key, null);
      }
      windowKeys.delete(id);
    },
    minimize: (id) => apply((state) => minimizeWindow(state, id)),
    // A keyboard activation of the button gives no pointer event, thus the action also focuses.
    toggleMaximize: (id) => apply((state) => focusWindow(toggleMaximize(state, id), id)),
    place: (id, rect, zone) =>
      apply((state) =>
        zone ? snapWindow(placeWindow(state, id, rect), id, zone) : placeWindow(state, id, clampRect(rect, area())),
      ),
    setPreview,
    openApp(appId) {
      apply((state) => withApp(state, appId));
      queueMicrotask(() => focusElement(focusedId()));
    },
    switchWorkspace: (index) => apply((state) => switchWorkspace(state, index)),
    tile: () => apply((state) => tileWorkspace(state, area())),
  };

  const onKeyDown = (event: KeyboardEvent) => {
    superTap.down(event);
    if (isLauncherKey(event)) {
      event.preventDefault();
      setLauncherOpen(!launcherOpen());
      return;
    }
    const action = actionFor(event);
    if (!action) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const id = focusedId();
    switch (action.type) {
      case "switch": {
        const open = switcher();
        setSwitcher(open ? step(open, action.backward) : openSwitcher(switchable(desktop()), action.backward));
        break;
      }
      case "maximize":
        if (id) apply((state) => toggleMaximize(state, id));
        break;
      case "minimize":
        if (id) apply((state) => minimizeWindow(state, id));
        break;
      case "snap":
        if (id) apply((state) => snapWindow(state, id, action.side));
        break;
      case "workspace": {
        const index = desktop().workspace + action.delta;
        apply((state) => (action.carry && id ? moveToWorkspace(state, id, index) : switchWorkspace(state, index)));
        break;
      }
      case "workspace-index":
        apply((state) => switchWorkspace(state, action.index));
        break;
      case "tile":
        apply((state) => tileWorkspace(state, area()));
        break;
    }
    queueMicrotask(() => focusElement(focusedId()));
  };

  const onKeyUp = (event: KeyboardEvent) => {
    if (superTap.up(event)) {
      // A tap of Super opens or closes the launcher. It reaches the browser only with Keyboard Lock.
      event.preventDefault();
      setLauncherOpen(!launcherOpen());
      return;
    }
    // The release of Alt gives the selected window the focus.
    const open = switcher();
    if (open && event.key === "Alt") {
      setSwitcher(null);
      controller.activate(selected(open));
    }
  };

  const onBlur = () => {
    setSwitcher(null);
    superTap.reset();
  };

  const measure = (width: number, height: number) => {
    if (width > 0 && height > 0) {
      setArea({ x: 0, y: 0, w: Math.round(width), h: Math.round(height) });
    }
  };
  // A browser with no ResizeObserver, for example a test DOM, uses the size of the window.
  const resize =
    typeof ResizeObserver === "undefined"
      ? { observe: () => measure(window.innerWidth, window.innerHeight), disconnect: () => {} }
      : new ResizeObserver(([entry]) => {
          if (entry) {
            measure(entry.contentRect.width, entry.contentRect.height);
          }
        });

  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("keyup", onKeyUp, true);
  window.addEventListener("blur", onBlur);
  onCleanup(() => {
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("keyup", onKeyUp, true);
    window.removeEventListener("blur", onBlur);
    resize.disconnect();
    clearTimeout(saveTimer);
    store.close();
  });

  return controller;
}
