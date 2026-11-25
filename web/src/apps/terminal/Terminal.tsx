import { createSignal, For, Show } from "solid-js";
import type { MenuItem } from "../../menu/menu";
import type { PtyStart } from "../../proto";
import type { AppContext } from "../types";
import { Pane, type PaneHandle } from "./Pane";
import { parseTerminal, TERMINAL_KEY, type TerminalSettings } from "./settings";
import {
  activeTab,
  addTab,
  allPanes,
  type Box,
  closePane,
  closeTab,
  focusPane,
  geometry,
  initialLayout,
  paneInDirection,
  panesOf,
  parseLayout,
  parseStart,
  resizeSplit,
  selectTab,
  splitPane,
  startOf,
  stepTab,
  type Tab,
  type TabLayout,
  tabOfPane,
} from "./tabs";
import "./terminal.css";

const ARROWS: Readonly<Record<string, "left" | "right" | "up" | "down">> = { ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" };

/** Returns the CSS place of a box in the tab area. */
const place = (box: Box) => ({ left: `${box.x * 100}%`, top: `${box.y * 100}%`, width: `${box.w * 100}%`, height: `${box.h * 100}%` });

/**
 * The terminal app: shells on the host, in tabs (F108) and in split panes (F109).
 *
 * Each pane has its own terminal in the agent. The first pane has the id of the window.
 * Thus a reload, a break of the connection, and a second browser tab attach to the same
 * shells, with their scrollback. Closing a pane or a tab ends its shells. Closing the window ends all.
 *
 * All panes of all tabs are children of one element, each at its place. Thus a split,
 * a close, or a change of tab moves a pane but never starts its terminal again.
 */
export default function TerminalApp(props: { context: AppContext }) {
  const context = props.context;
  const windowId = context.windowId;
  const layoutKey = context.state.key("terminal.tabs");
  /** The tabs, or null before the state store answers. */
  const [layout, setLayout] = createSignal<TabLayout | null>(null);
  const [titles, setTitles] = createSignal<Readonly<Record<string, string>>>({});
  // A terminal bookmark opens the window with a start for its first pane (F110). The layout
  // waits for both keys: a new window has no layout, and then the first pane gets the start.
  let stored: unknown;
  let opened: { start?: PtyStart; name?: string } | null | undefined;
  const settle = () => {
    if (stored === undefined || opened === undefined) return;
    if (stored === null && opened?.start) {
      const first = initialLayout(windowId, opened.start);
      if (opened.name) setTitles((old) => ({ ...old, [windowId]: opened!.name! }));
      setLayout(first);
      context.state.set(layoutKey, first);
      context.setTitle(opened.name || "Terminal");
      return;
    }
    setLayout(parseLayout(stored, windowId));
  };
  context.state.watch(context.state.key("start"), (value) => {
    const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : null;
    const start = parseStart(record?.start);
    opened = record && start ? { start, ...(typeof record.name === "string" ? { name: record.name } : {}) } : null;
    settle();
  });
  context.state.watch(layoutKey, (value) => {
    stored = value ?? null;
    settle();
  });
  /** The settings of Settings > Terminal (F107). A change applies at once. */
  const [settings, setSettings] = createSignal<TerminalSettings | undefined>(undefined);
  context.state.watchShared(TERMINAL_KEY, (value) => setSettings(parseTerminal(value)));
  const panes = new Map<string, PaneHandle>();
  let area: HTMLDivElement | undefined;

  const tabs = () => layout()?.tabs ?? [];
  const current = () => {
    const each = layout();
    return each ? activeTab(each) : undefined;
  };
  const titleOf = (pane: string) => titles()[pane] || "Terminal";
  const tabTitle = (tab: Tab) => titleOf(tab.focus);
  const showBar = () => tabs().length > 1 || settings()?.tabBar === "always";
  const shape = () => {
    const tab = current();
    return tab ? geometry(tab.root) : { panes: new Map<string, Box>(), dividers: [] };
  };
  const split = () => (current()?.root && !("pane" in current()!.root) ? true : false);

  /** Shows a new layout. `save` false keeps it out of the state store, for the steps of a drag. */
  const change = (next: TabLayout, save = true) => {
    const before = layout();
    setLayout(next);
    if (save) context.state.set(layoutKey, next);
    const tab = activeTab(next);
    context.setTitle(tabTitle(tab));
    const focused = before ? activeTab(before).focus : undefined;
    if (focused !== tab.focus) queueMicrotask(() => panes.get(tab.focus)?.focus());
  };
  const update = (step: (each: TabLayout) => TabLayout) => {
    const each = layout();
    if (!each) return;
    const next = step(each);
    if (next !== each) change(next);
  };
  /** Ends the shells of the panes that the layout no longer has. */
  const reap = (before: TabLayout, after: TabLayout) => {
    const kept = new Set(allPanes(after));
    for (const pane of allPanes(before)) {
      if (kept.has(pane)) continue;
      panes.get(pane)?.terminate();
      panes.delete(pane);
    }
  };
  const remove = (step: (each: TabLayout) => TabLayout) => {
    const each = layout();
    if (!each) return;
    const next = step(each);
    if (next === each) return;
    reap(each, next);
    change(next);
  };

  const newTab = () => update((each) => addTab(each, windowId, settings()?.newTab === "next"));
  const closeTabOf = (id: string) => remove((each) => closeTab(each, id));
  /** Reports whether the pane can close: the last pane of the last tab stays. */
  const canClose = (pane: string) => tabs().length > 1 || panesOf(tabOfPane(layout()!, pane)?.root ?? { pane }).length > 1;
  const closePaneOf = (pane: string) => {
    if (canClose(pane)) remove((each) => closePane(each, pane));
  };
  const splitOf = (pane: string, dir: "row" | "column") => update((each) => splitPane(each, windowId, pane, dir));

  /**
   * The keys of the tabs and the panes. The browser keeps Ctrl+Shift+T and Ctrl+PageDown,
   * thus the terminal uses Alt and Alt+Shift.
   */
  const shortcut = (event: KeyboardEvent, pane: string) => {
    if (event.ctrlKey || event.metaKey || !event.altKey) return false;
    if (event.shiftKey && event.key === "Enter") {
      newTab();
      return true;
    }
    if (!event.shiftKey && (event.key === "PageDown" || event.key === "PageUp")) {
      update((each) => (each.tabs.length > 1 ? stepTab(each, event.key === "PageDown" ? 1 : -1) : each));
      return true;
    }
    if (event.shiftKey && event.code === "Backslash") {
      splitOf(pane, "row");
      return true;
    }
    if (event.shiftKey && event.code === "Minus") {
      splitOf(pane, "column");
      return true;
    }
    const direction = ARROWS[event.key];
    if (event.shiftKey && direction && split()) {
      const tab = current();
      const target = tab && paneInDirection(tab.root, pane, direction);
      if (target) update((each) => focusPane(each, target));
      return true;
    }
    return false;
  };

  const menuItems = (pane: string): MenuItem[] => [
    { separator: true },
    { label: "Split right", shortcut: "Alt+Shift+\\", action: () => splitOf(pane, "row") },
    { label: "Split down", shortcut: "Alt+Shift+-", action: () => splitOf(pane, "column") },
    { label: "Close pane", disabled: !canClose(pane), action: () => closePaneOf(pane) },
    { separator: true },
    { label: "New tab", shortcut: "Alt+Shift+Enter", action: newTab },
    { label: "Close tab", disabled: tabs().length === 1, action: () => closeTabOf(tabOfPane(layout()!, pane)?.id ?? "") },
  ];

  /** Moves the divider of the split `id` with the pointer. The layout goes to the state store at the end. */
  const drag = (event: PointerEvent, divider: ReturnType<typeof shape>["dividers"][number]) => {
    if (event.button !== 0 || !area) return;
    event.preventDefault();
    // Solid delegates the event: currentTarget is not the divider.
    const handle = (event.target as HTMLElement).closest<HTMLElement>(".term-divider");
    if (!handle) return;
    handle.setPointerCapture(event.pointerId);
    const bounds = area.getBoundingClientRect();
    const { box, dir, split: id } = divider;
    const move = (each: PointerEvent) => {
      const ratio = dir === "row" ? ((each.clientX - bounds.left) / bounds.width - box.x) / box.w : ((each.clientY - bounds.top) / bounds.height - box.y) / box.h;
      const now = layout();
      if (now) change(resizeSplit(now, id, ratio), false);
    };
    const end = () => {
      handle.removeEventListener("pointermove", move);
      const now = layout();
      if (now) context.state.set(layoutKey, now);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end, { once: true });
    handle.addEventListener("pointercancel", end, { once: true });
  };

  context.onFocus(() => {
    const tab = current();
    if (tab) panes.get(tab.focus)?.focus();
  });
  // Closing the window ends each shell. A reload or a break of the connection does not.
  context.onClose(() => {
    for (const pane of panes.values()) pane.terminate();
  });

  return (
    <div class="term-window" data-tabs={showBar() ? "true" : undefined}>
      <Show when={showBar()}>
        <div class="term-tabs" role="tablist" aria-label="Terminal tabs">
          <For each={tabs()} keyed={(tab: Tab) => tab.id}>
            {(tab) => (
              <div
                class="term-tab"
                role="tab"
                tabindex={tab().id === layout()?.active ? 0 : -1}
                aria-selected={tab().id === layout()?.active ? "true" : "false"}
                title={tabTitle(tab())}
                onClick={() => update((each) => selectTab(each, tab().id))}
                onAuxClick={(event) => {
                  if (event.button === 1) closeTabOf(tab().id);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") update((each) => selectTab(each, tab().id));
                }}
              >
                <span class="term-tab-title">{tabTitle(tab())}</span>
                <Show when={tabs().length > 1}>
                  <button
                    type="button"
                    class="term-tab-close"
                    aria-label={`Close ${tabTitle(tab())}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      closeTabOf(tab().id);
                    }}
                  >
                    ×
                  </button>
                </Show>
              </div>
            )}
          </For>
          <button type="button" class="term-tab-new" aria-label="New tab" title="New tab (Alt+Shift+Enter)" onClick={newTab}>
            +
          </button>
        </div>
      </Show>
      <div class="term-panes" ref={(el) => (area = el)} data-split={split() ? "true" : undefined} data-dim={settings()?.dimInactive === false ? undefined : "true"}>
        <For each={layout() ? allPanes(layout()!) : []}>
          {(pane) => {
            const box = () => shape().panes.get(pane);
            const focused = () => current()?.focus === pane;
            return (
              <div
                class="term-cell"
                hidden={!box()}
                data-focused={focused() ? "true" : undefined}
                style={box() ? place(box()!) : undefined}
                onFocusIn={() => update((each) => focusPane(each, pane))}
                onPointerEnter={() => {
                  if (settings()?.focusFollowsMouse && split() && !focused()) update((each) => focusPane(each, pane));
                }}
              >
                <Pane
                  context={context}
                  terminal={pane}
                  start={layout() ? startOf(layout()!, pane) : undefined}
                  visible={() => box() !== undefined}
                  settings={settings}
                  onReady={(handle) => {
                    panes.set(pane, handle);
                    if (focused() && context.isFocused()) handle.focus();
                  }}
                  onTitle={(title) => {
                    setTitles((old) => ({ ...old, [pane]: title }));
                    if (focused()) context.setTitle(titleOf(pane));
                  }}
                  onExit={() => {
                    if (settings()?.onExit === "keep" || !canClose(pane)) return false;
                    closePaneOf(pane);
                    return true;
                  }}
                  onShortcut={(event) => shortcut(event, pane)}
                  menuItems={() => menuItems(pane)}
                />
              </div>
            );
          }}
        </For>
        <For each={shape().dividers} keyed={(divider: ReturnType<typeof shape>["dividers"][number]) => divider.split}>
          {(divider) => {
            const at = () => {
              const { box, dir, ratio } = divider();
              return dir === "row" ? { ...box, x: box.x + box.w * ratio, w: 0 } : { ...box, y: box.y + box.h * ratio, h: 0 };
            };
            return (
              <div
                class="term-divider"
                role="separator"
                aria-orientation={divider().dir === "row" ? "vertical" : "horizontal"}
                aria-valuenow={Math.round(divider().ratio * 100)}
                data-dir={divider().dir}
                // Where two dividers meet, the outer one, which goes on, is on top.
                style={{ ...place(at()), "z-index": String(10 + shape().dividers.length - shape().dividers.findIndex((each) => each.split === divider().split)) }}
                onPointerDown={(event) => drag(event, divider())}
                onDblClick={() => update((each) => resizeSplit(each, divider().split, 0.5))}
              />
            );
          }}
        </For>
      </div>
    </div>
  );
}
