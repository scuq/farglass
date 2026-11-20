import { createSignal, For, Match, onCleanup, Switch } from "solid-js";
import { DbusClient } from "../../dbus";
import type { AppContext } from "../types";
import { AppearancePage } from "./pages/Appearance";
import { DateTimePage } from "./pages/DateTime";
import { FocusPage } from "./pages/Focus";
import { NetworkPage } from "./pages/Network";
import { PowerPage } from "./pages/Power";
import { SystemPage } from "./pages/System";
import { UsersPage } from "./pages/Users";
import "./settings.css";

import { PAGES, type PageId } from "./pages";

export { PAGES };

/**
 * The settings app. Each page uses one dbus channel to the system bus. A change
 * that needs an administrator goes through polkit, which asks in the shell.
 */
export default function Settings(props: { context: AppContext }) {
  const [page, setPage] = createSignal<PageId>("appearance");
  const pageKey = props.context.state.key("page");
  props.context.state.watch(pageKey, (value) => {
    if (PAGES.some((each) => each.id === value)) {
      setPage(value as PageId);
    }
  });
  const choose = (id: PageId) => {
    setPage(id);
    props.context.state.set(pageKey, id);
  };
  const dbus = new DbusClient(props.context.connection, "system");
  onCleanup(() => dbus.close());

  /** Up and Down move in the list of pages, as in a list box. */
  const onKey = (event: KeyboardEvent) => {
    const index = PAGES.findIndex((each) => each.id === page());
    const next = event.key === "ArrowDown" ? index + 1 : event.key === "ArrowUp" ? index - 1 : -1;
    const target = PAGES[next];
    if (target) {
      event.preventDefault();
      choose(target.id);
      (event.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-page="${target.id}"]`)?.focus();
    }
  };

  return (
    <div class="settings">
      <nav class="settings-nav" aria-label="Settings pages" onKeyDown={onKey}>
        <For each={PAGES}>
          {(each) => (
            <button
              type="button"
              data-page={each.id}
              aria-current={page() === each.id ? "page" : undefined}
              tabindex={page() === each.id ? 0 : -1}
              onClick={() => choose(each.id)}
            >
              {each.label}
            </button>
          )}
        </For>
      </nav>
      <main class="settings-main">
        <Switch>
          <Match when={page() === "appearance"}>
            <AppearancePage context={props.context} />
          </Match>
          <Match when={page() === "focus"}>
            <FocusPage context={props.context} />
          </Match>
          <Match when={page() === "datetime"}>
            <DateTimePage dbus={dbus} />
          </Match>
          <Match when={page() === "system"}>
            <SystemPage dbus={dbus} />
          </Match>
          <Match when={page() === "network"}>
            <NetworkPage dbus={dbus} />
          </Match>
          <Match when={page() === "users"}>
            <UsersPage dbus={dbus} user={props.context.user} />
          </Match>
          <Match when={page() === "power"}>
            <PowerPage dbus={dbus} />
          </Match>
        </Switch>
      </main>
    </div>
  );
}
