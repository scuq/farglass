/**
 * The controller of the desktop: the state, the actions, the keys, and the
 * persistence in the state store. The panel and the window surface share it.
 */

import { type Accessor, createMemo, createSignal, onCleanup } from "solid-js";
import { APPS } from "../apps/registry";
import type { Connection } from "../proto";
import { StateClient } from "../state";
import { clampRect, type Rect, type SnapZone } from "./geometry";
import { DEFAULT_FOCUS, FOCUS_KEY, type FocusSettings, InputWatch, parseFocusSettings } from "./focus";
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
  /**
   * Adds a handler that runs when the window `id` closes, or gets the focus.
   * Returns a function that removes the handler.
   */
  addHook(id: string, event: "close" | "focus", handler: () => void): () => void;
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
  /**
   * Opens a window of the app `appId`. With `state`, each name gets its value in the state of the
   * window before the app starts, for example the page of the settings (F042). With `state`, an
   * open window of the app gets the values and the focus instead of a new window.
   */
  openApp(appId: string, state?: Readonly<Record<string, unknown>>): void;
  /**
   * Opens a window with the id `id` for the app `linux`, if no window has the id. With focus
   * protection, it gets the focus only if the user asked for it (F041): `hint` names its app and the window that it belongs to.
   */
  openSurface(id: string, title: string, w: number, h: number, hint?: { appId?: string | null; parent?: string | null }): void;
  /** The input of the user, for focus protection. */
  readonly input: InputWatch;
  /** The settings of focus protection. */
  readonly focusSettings: Accessor<FocusSettings>;
  /** Records a start from the launcher: a window with one of `names` as app id gets the focus. */
  expectLaunch(names: readonly string[]): void;
  /** Returns the ids of the windows of the app `linux`. */
  surfaceWindows(): string[];
  /** Calls `handler` after the desktop came from the state store, at once if it came already. */
  whenLoaded(handler: () => void): void;
  switchWorkspace(index: number): void;
  tile(): void;
  /** Snaps a window to `zone`. */
  snap(id: string, zone: SnapZone): void;
  /** Moves a window to the workspace `index`. */
  moveToWorkspace(id: string, index: number): void;
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
  const input = new InputWatch();
  const [focusSettings, setFocusSettings] = createSignal<FocusSettings>(DEFAULT_FOCUS);
  store.watch(FOCUS_KEY, (value) => setFocusSettings(parseFocusSettings(value)));
  let loaded = false;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  // The desktop after the last change, until it is saved. A reload or a close of the tab saves it at once.
  let unsaved: Desktop | null = null;
  let surface: HTMLElement | undefined;
  /** The state keys of each window. */
  const windowKeys = new Map<string, Set<string>>();
  /** The handlers of each window, by event. */
  const hooks = new Map<string, Set<() => void>>();
  const runHooks = (id: string, event: "close" | "focus") => {
    for (const handler of [...(hooks.get(`${event}:${id}`) ?? [])]) {
      handler();
    }
  };

  /** Applies an action, and saves the result after a short wait. */
  const apply = (change: (state: Desktop) => Desktop) => {
    const next = change(desktop());
    if (next === desktop()) {
      return;
    }
    setDesktop(next);
    clearTimeout(saveTimer);
    unsaved = next;
    saveTimer = setTimeout(save, SAVE_DELAY_MS);
  };
  /** Saves the desktop that waits for its save, if one waits. */
  const save = () => {
    clearTimeout(saveTimer);
    if (unsaved) {
      store.set(DESKTOP_KEY, unsaved);
      unsaved = null;
    }
  };

  const withApp = (state: Desktop, appId: string, id: string = crypto.randomUUID()): Desktop => {
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
    return openWindow(state, { id, app: app.id, title, ...app.size }, area());
  };

  const onLoaded: (() => void)[] = [];
  store.watch(DESKTOP_KEY, (value) => {
    const restored = parseDesktop(value);
    if (restored) {
      setDesktop(restored);
    } else if (!loaded) {
      // A new session: open one window, thus the desktop is not empty.
      apply((state) => withApp(state, "echo"));
    }
    if (!loaded) {
      loaded = true;
      for (const handler of onLoaded.splice(0)) {
        handler();
      }
    }
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
    runHooks(id, "focus");
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
    addHook(id, event, handler) {
      const key = `${event}:${id}`;
      let set = hooks.get(key);
      if (!set) {
        set = new Set();
        hooks.set(key, set);
      }
      set.add(handler);
      return () => set.delete(handler);
    },
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
      // The app gets the close before its window goes, for example to end its terminal.
      runHooks(id, "close");
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
    openApp(appId, state) {
      if (state) {
        // The key of a name in the state of a window, as AppHost makes it.
        const existing = desktop().recent.find((id) => windowById(desktop(), id)?.app === appId);
        const id = existing ?? crypto.randomUUID();
        for (const [name, value] of Object.entries(state)) {
          store.set(`${appId}.${name}.${id}`, value, { local: true });
        }
        if (existing) {
          controller.activate(existing);
          return;
        }
        apply((current) => withApp(current, appId, id));
      } else {
        apply((current) => withApp(current, appId));
      }
      queueMicrotask(() => focusElement(focusedId()));
    },
    openSurface(id, title, w, h, hint = {}) {
      if (desktop().windows.some((win) => win.id === id)) return;
      const asked =
        !focusSettings().quietWindows ||
        input.launched(hint.appId) ||
        input.requested() ||
        (hint.parent != null && hint.parent === focusedId());
      apply((state) => (state.windows.some((win) => win.id === id) ? state : openWindow(state, { id, app: "linux", title, w, h }, area(), { focus: asked })));
    },
    input,
    focusSettings,
    expectLaunch: (names) => input.expectLaunch(names),
    whenLoaded(handler) {
      if (loaded) {
        handler();
      } else {
        onLoaded.push(handler);
      }
    },
    surfaceWindows: () => (unsaved ?? desktop()).windows.filter((win) => win.app === "linux").map((win) => win.id),
    switchWorkspace: (index) => apply((state) => switchWorkspace(state, index)),
    tile: () => apply((state) => tileWorkspace(state, area())),
    snap: (id, zone) => apply((state) => focusWindow(snapWindow(state, id, zone), id)),
    moveToWorkspace: (id, index) => apply((state) => moveToWorkspace(state, id, index)),
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
  window.addEventListener("pagehide", save);
  onCleanup(() => {
    window.removeEventListener("pagehide", save);
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("keyup", onKeyUp, true);
    window.removeEventListener("blur", onBlur);
    resize.disconnect();
    input.stop();
    clearTimeout(saveTimer);
    store.close();
  });

  return controller;
}
