import { createSignal, For, Show } from "solid-js";
import type { AppContext } from "../types";
import { Pane, type PaneHandle } from "./Pane";
import { parseTerminal, TERMINAL_KEY, type TerminalSettings } from "./settings";
import { addTab, closeTab, parseLayout, selectTab, stepTab, type TabLayout } from "./tabs";
import "./terminal.css";

/**
 * The terminal app: shells on the host, one in each tab of the window (F108).
 *
 * Each tab has its own terminal in the agent. The first tab has the id of the window.
 * Thus a reload, a break of the connection, and a second browser tab attach to the same
 * shells, with their scrollback. Closing a tab ends its shell. Closing the window ends all.
 */
export default function TerminalApp(props: { context: AppContext }) {
  const context = props.context;
  const windowId = context.windowId;
  const layoutKey = context.state.key("terminal.tabs");
  /** The tabs, or null before the state store answers. */
  const [layout, setLayout] = createSignal<TabLayout | null>(null);
  context.state.watch(layoutKey, (value) => setLayout(parseLayout(value, windowId)));
  /** The settings of Settings > Terminal (F107). A change applies at once. */
  const [settings, setSettings] = createSignal<TerminalSettings | undefined>(undefined);
  context.state.watchShared(TERMINAL_KEY, (value) => setSettings(parseTerminal(value)));
  const [titles, setTitles] = createSignal<Readonly<Record<string, string>>>({});
  const panes = new Map<string, PaneHandle>();

  const tabs = () => layout()?.tabs ?? [];
  const active = () => layout()?.active ?? windowId;
  const titleOf = (id: string) => titles()[id] || "Terminal";
  const showBar = () => tabs().length > 1 || settings()?.tabBar === "always";

  const change = (next: TabLayout) => {
    const before = layout();
    setLayout(next);
    context.state.set(layoutKey, next);
    context.setTitle(titleOf(next.active));
    if (before?.active !== next.active) queueMicrotask(() => panes.get(next.active)?.focus());
  };

  const newTab = () => {
    const current = layout();
    if (current) change(addTab(current, windowId, settings()?.newTab === "next"));
  };
  const close = (id: string) => {
    const current = layout();
    if (!current || current.tabs.length === 1) return;
    panes.get(id)?.terminate();
    panes.delete(id);
    change(closeTab(current, id));
  };
  const select = (id: string) => {
    const current = layout();
    if (current) change(selectTab(current, id));
  };
  const step = (by: number) => {
    const current = layout();
    if (current && current.tabs.length > 1) change(stepTab(current, by));
  };

  /** The keys of the tabs. The browser keeps Ctrl+Shift+T and Ctrl+PageDown, thus the tabs use Alt. */
  const shortcut = (event: KeyboardEvent) => {
    const plain = !event.ctrlKey && !event.metaKey;
    if (plain && event.altKey && event.shiftKey && event.key === "Enter") {
      newTab();
      return true;
    }
    if (plain && event.altKey && !event.shiftKey && (event.key === "PageDown" || event.key === "PageUp")) {
      step(event.key === "PageDown" ? 1 : -1);
      return true;
    }
    return false;
  };

  const menuItems = (id: string) => [
    { separator: true as const },
    { label: "New tab", shortcut: "Alt+Shift+Enter", action: newTab },
    { label: "Close tab", disabled: tabs().length === 1, action: () => close(id) },
  ];

  context.onFocus(() => panes.get(active())?.focus());
  // Closing the window ends each shell. A reload or a break of the connection does not.
  context.onClose(() => {
    for (const pane of panes.values()) pane.terminate();
  });

  return (
    <div class="term-window" data-tabs={showBar() ? "true" : undefined}>
      <Show when={showBar()}>
        <div class="term-tabs" role="tablist" aria-label="Terminal tabs">
          <For each={tabs()}>
            {(id) => (
              <div
                class="term-tab"
                role="tab"
                tabindex={id === active() ? 0 : -1}
                aria-selected={id === active() ? "true" : "false"}
                title={titleOf(id)}
                onClick={() => select(id)}
                onAuxClick={(event) => {
                  if (event.button === 1) close(id);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") select(id);
                }}
              >
                <span class="term-tab-title">{titleOf(id)}</span>
                <Show when={tabs().length > 1}>
                  <button
                    type="button"
                    class="term-tab-close"
                    aria-label={`Close ${titleOf(id)}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      close(id);
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
      <div class="term-panes">
        <For each={tabs()}>
          {(id) => (
            <Pane
              context={context}
              terminal={id}
              visible={() => id === active()}
              settings={settings}
              onReady={(handle) => {
                panes.set(id, handle);
                if (id === active() && context.isFocused()) handle.focus();
              }}
              onTitle={(title) => {
                setTitles((old) => ({ ...old, [id]: title }));
                if (id === active()) context.setTitle(titleOf(id));
              }}
              onExit={() => {
                if (settings()?.onExit === "keep" || tabs().length === 1) return false;
                close(id);
                return true;
              }}
              onShortcut={shortcut}
              menuItems={() => menuItems(id)}
            />
          )}
        </For>
      </div>
    </div>
  );
}
