import { For, Show, untrack } from "solid-js";
import { APPS } from "../apps";
import type { DesktopController } from "./controller";
import { snapRect } from "./geometry";
import { type Win, windowById } from "./model";
import { WindowFrame } from "./WindowFrame";

/** The window surface: the windows of the desktop, the snap preview, and the window switcher. */
export function Surface(props: { controller: DesktopController }) {
  const c = props.controller;
  const order = (id: string) => c.desktop().windows.findIndex((win) => win.id === id);
  const title = (id: string) => windowById(c.desktop(), id)?.title ?? "";
  const actions = {
    focus: c.focus,
    close: c.close,
    minimize: c.minimize,
    toggleMaximize: c.toggleMaximize,
    place: c.place,
    preview: c.setPreview,
  };

  return (
    <div class="wm-surface" ref={(element) => c.attachSurface(element)}>
      <For each={c.desktop().windows} keyed={(win: Win) => win.id}>
        {(win) => {
          // The app renders one time for each window. A move, a snap, or a focus
          // changes `win`, but must not start the app again.
          const appId = untrack(() => win().app);
          const windowId = untrack(() => win().id);
          const content =
            untrack(() => APPS[appId]?.render({ connection: c.connection, windowId })) ?? (
              <p>The app {appId} is not available.</p>
            );
          return (
            <WindowFrame
              win={win}
              area={c.area}
              z={() => order(win().id) + 1}
              focused={() => c.focusedId() === win().id}
              hidden={() => win().minimized || win().workspace !== c.desktop().workspace}
              actions={actions}
            >
              {content}
            </WindowFrame>
          );
        }}
      </For>
      <Show when={c.preview()}>
        {(zone) => {
          const rect = () => snapRect(zone(), c.area());
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
      <Show when={c.switcher()}>
        {(open) => (
          <div class="wm-switcher" role="listbox" aria-label="Windows">
            <For each={open().items}>
              {(id, index) => (
                <div
                  class="wm-switcher-item"
                  role="option"
                  aria-selected={String(index() === open().index) as "true" | "false"}
                >
                  {title(id)}
                </div>
              )}
            </For>
          </div>
        )}
      </Show>
    </div>
  );
}
