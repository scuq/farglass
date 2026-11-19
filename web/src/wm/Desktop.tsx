import { createMemo, createSignal, For, onCleanup, Show, untrack } from "solid-js";
import { type AppContext, APPS } from "../apps";
import type { Connection } from "../proto";
import { StateClient } from "../state";
import { clampRect, type Rect, type SnapZone, snapRect } from "./geometry";
import { actionFor } from "./keys";
import {
  closeWindow,
  type Desktop as DesktopState,
  emptyDesktop,
  focused,
  focusWindow,
  minimizeWindow,
  moveToWorkspace,
  openWindow,
  parseDesktop,
  placeWindow,
  snapWindow,
  switchable,
  switchWorkspace,
  tileWorkspace,
  toggleMaximize,
  type Win,
  windowById,
} from "./model";
import { openSwitcher, selected, step, type Switcher } from "./switcher";
import { WindowFrame } from "./WindowFrame";
import "./wm.css";

/** The key of the desktop in the state store. All tabs of the user share it. */
const DESKTOP_KEY = "desktop";

/** The wait after the last change before the desktop goes to the state store. */
const SAVE_DELAY_MS = 250;

/** The desktop size before the first measurement. */
const DEFAULT_AREA: Rect = { x: 0, y: 0, w: 1280, h: 720 };

/**
 * The desktop: the windows, the workspace bar, and the switcher.
 *
 * The state of the desktop is in the state store of the agent. A reload gives it
 * back, and each tab of the user shows the same windows.
 */
export function Desktop(props: { connection: Connection }) {
  const [desktop, setDesktop] = createSignal<DesktopState>(emptyDesktop());
  const [area, setArea] = createSignal<Rect>(DEFAULT_AREA);
  const [preview, setPreview] = createSignal<SnapZone | null>(null);
  const [switcher, setSwitcher] = createSignal<Switcher | null>(null);
  const [loaded, setLoaded] = createSignal(false);
  const context: Omit<AppContext, "windowId"> = { connection: props.connection };
  const store = new StateClient(props.connection);
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let surface: HTMLElement | undefined;

  /** Applies an action, and saves the result after a short wait. */
  const apply = (change: (state: DesktopState) => DesktopState) => {
    const next = change(desktop());
    if (next === desktop()) {
      return;
    }
    setDesktop(next);
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => store.set(DESKTOP_KEY, desktop()), SAVE_DELAY_MS);
  };

  store.watch(DESKTOP_KEY, (value) => {
    const restored = parseDesktop(value);
    if (restored) {
      setDesktop(restored);
    } else if (!loaded()) {
      // A new session: open one window, thus the desktop is not empty.
      apply((state) => openApp(state, "echo"));
    }
    setLoaded(true);
  });

  const openApp = (state: DesktopState, appId: string) => {
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

  const focusedId = createMemo(() => focused(desktop()));

  // Give the keyboard focus to the window with the focus.
  const focusElement = (id: string | null) => {
    if (!id || !surface) {
      return;
    }
    const element = surface.querySelector<HTMLElement>(`[data-window="${CSS.escape(id)}"]`);
    if (element && !element.contains(document.activeElement)) {
      element.focus({ preventScroll: true });
    }
  };

  const actions = {
    focus: (id: string) => {
      if (focusedId() !== id || windowById(desktop(), id)?.minimized) {
        apply((state) => focusWindow(state, id));
      }
    },
    close: (id: string) => apply((state) => closeWindow(state, id)),
    minimize: (id: string) => apply((state) => minimizeWindow(state, id)),
    // A keyboard activation of the button gives no pointer event, thus the action also focuses.
    toggleMaximize: (id: string) => apply((state) => focusWindow(toggleMaximize(state, id), id)),
    place: (id: string, rect: Rect, zone: SnapZone | null) =>
      apply((state) => (zone ? snapWindow(placeWindow(state, id, rect), id, zone) : placeWindow(state, id, clampRect(rect, area())))),
    preview: setPreview,
  };

  const onKeyDown = (event: KeyboardEvent) => {
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

  // The release of Alt gives the selected window the focus.
  const onKeyUp = (event: KeyboardEvent) => {
    const open = switcher();
    if (open && event.key === "Alt") {
      setSwitcher(null);
      const id = selected(open);
      apply((state) => focusWindow(state, id));
      queueMicrotask(() => focusElement(id));
    }
  };
  const onBlur = () => setSwitcher(null);

  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("keyup", onKeyUp, true);
  window.addEventListener("blur", onBlur);
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
  onCleanup(() => {
    window.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("keyup", onKeyUp, true);
    window.removeEventListener("blur", onBlur);
    resize.disconnect();
    clearTimeout(saveTimer);
    store.close();
  });

  const order = (id: string) => desktop().windows.findIndex((win) => win.id === id);
  const title = (id: string) => windowById(desktop(), id)?.title ?? "";

  return (
    <div class="wm">
      <div
        class="wm-surface"
        ref={(element) => {
          surface = element;
          resize.observe(element);
        }}
      >
        <For each={desktop().windows} keyed={(win: Win) => win.id}>
          {(win) => {
            // The app renders one time for each window. A move, a snap, or a focus
            // changes `win`, but must not start the app again.
            const appId = untrack(() => win().app);
            const windowId = untrack(() => win().id);
            const content =
              untrack(() => APPS[appId]?.render({ ...context, windowId })) ?? <p>The app {appId} is not available.</p>;
            return (
              <WindowFrame
                win={win}
                area={area}
                z={() => order(win().id) + 1}
                focused={() => focusedId() === win().id}
                hidden={() => win().minimized || win().workspace !== desktop().workspace}
                actions={actions}
              >
                {content}
              </WindowFrame>
            );
          }}
        </For>
        <Show when={preview()}>
          {(zone) => {
            const rect = () => snapRect(zone(), area());
            return (
              <div
                class="wm-snap-preview"
                style={{
                  transform: `translate3d(${rect().x}px, ${rect().y}px, 0)`,
                  width: `${rect().w}px`,
                  height: `${rect().h}px`,
                }}
              />
            );
          }}
        </Show>
        <Show when={switcher()}>
          {(open) => (
            <div class="wm-switcher" role="listbox" aria-label="Windows">
              <For each={open().items}>
                {(id, index) => (
                  <div class="wm-switcher-item" role="option" aria-selected={String(index() === open().index) as "true" | "false"}>
                    {title(id)}
                  </div>
                )}
              </For>
            </div>
          )}
        </Show>
      </div>

      <nav class="wm-bar" aria-label="Workspaces and windows">
        <div class="wm-workspaces">
          <For each={Array.from({ length: desktop().workspaces }, (_, i) => i)}>
            {(index) => (
              <button
                type="button"
                aria-pressed={String(desktop().workspace === index) as "true" | "false"}
                aria-label={`Workspace ${index + 1}`}
                onClick={() => apply((state) => switchWorkspace(state, index))}
              >
                {index + 1}
              </button>
            )}
          </For>
        </div>
        <div class="wm-tasks">
          <For
            each={desktop()
              .windows.filter((win) => win.workspace === desktop().workspace)
              // The stack order changes with each focus. The title order stays.
              .toSorted((a, b) => a.title.localeCompare(b.title, undefined, { numeric: true }))}
            keyed={(win: Win) => win.id}
          >
            {(win) => (
              <button
                type="button"
                class="wm-task"
                aria-pressed={String(focusedId() === win().id) as "true" | "false"}
                data-minimized={String(win().minimized)}
                onClick={() => {
                  if (focusedId() === win().id) {
                    actions.minimize(win().id);
                  } else {
                    actions.focus(win().id);
                    queueMicrotask(() => focusElement(win().id));
                  }
                }}
              >
                {win().title}
              </button>
            )}
          </For>
        </div>
        <div class="wm-launch">
          <For each={Object.values(APPS)}>
            {(app) => (
              <button type="button" onClick={() => apply((state) => openApp(state, app.id))}>
                New {app.title}
              </button>
            )}
          </For>
          <button type="button" onClick={() => apply((state) => tileWorkspace(state, area()))}>
            Tile
          </button>
        </div>
      </nav>
    </div>
  );
}
