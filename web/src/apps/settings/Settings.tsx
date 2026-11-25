import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { DbusClient } from "../../dbus";
import type { AppContext } from "../types";
import { SectionFilterContext } from "./common";
import { AppearancePage } from "./pages/Appearance";
import { DateTimePage } from "./pages/DateTime";
import { FocusPage } from "./pages/Focus";
import { KeyboardPage } from "./pages/Keyboard";
import { NetworkPage } from "./pages/Network";
import { PowerPage } from "./pages/Power";
import { PasskeysPage } from "./pages/Passkeys";
import { ResourcesPage } from "./pages/Resources";
import { SystemPage } from "./pages/System";
import { TerminalPage } from "./pages/Terminal";
import { UsersPage } from "./pages/Users";
import { matchSections, pagesOf } from "./sections";
import "./settings.css";

import { PAGES, type PageId } from "./pages";

export { PAGES };

/**
 * The settings app. Each page uses one dbus channel to the system bus. A change
 * that needs an administrator goes through polkit, which asks in the shell.
 */
export default function Settings(props: { context: AppContext }) {
  const [page, setPage] = createSignal<PageId>("appearance");
  const [filter, setFilter] = createSignal("");
  const pageKey = props.context.state.key("page");
  props.context.state.watch(pageKey, (value) => {
    if (PAGES.some((each) => each.id === value)) {
      setPage(value as PageId);
    }
  });
  const choose = (id: PageId) => {
    setFilter("");
    setPage(id);
    props.context.state.set(pageKey, id);
  };
  const dbus = new DbusClient(props.context.connection, "system");
  onCleanup(() => dbus.close());

  /** The cards that match the filter, or null with no filter. */
  const matched = createMemo(() => matchSections(filter()));
  /** The pages that show: the open page, or with a filter each page that has a matching card. */
  const shown = createMemo(() => {
    const cards = matched();
    const ids = cards ? pagesOf(cards) : [page()];
    return PAGES.filter((each) => ids.includes(each.id));
  });

  /** The arrow keys move in the list of pages, as in a list box. */
  const onKey = (event: KeyboardEvent) => {
    const index = PAGES.findIndex((each) => each.id === page());
    const forward = event.key === "ArrowDown" || event.key === "ArrowRight";
    const back = event.key === "ArrowUp" || event.key === "ArrowLeft";
    const target = PAGES[forward ? index + 1 : back ? index - 1 : -1];
    if (target) {
      event.preventDefault();
      choose(target.id);
      (event.currentTarget as HTMLElement).querySelector<HTMLElement>(`[data-page="${target.id}"]`)?.focus();
    }
  };

  let filterInput: HTMLInputElement | undefined;
  /** Ctrl+F, or Cmd+F on a Mac, goes to the filter. */
  const onFind = (event: KeyboardEvent) => {
    if ((event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === "f") {
      event.preventDefault();
      filterInput?.focus();
      filterInput?.select();
    }
  };
  /** Escape clears a filter. With no filter, the key goes on to the shell. */
  const onFilterKey = (event: KeyboardEvent) => {
    if (event.key === "Escape" && filter() !== "") {
      event.preventDefault();
      event.stopPropagation();
      setFilter("");
    }
  };

  const view = (id: PageId) => {
    switch (id) {
      case "appearance":
        return <AppearancePage context={props.context} />;
      case "terminal":
        return <TerminalPage context={props.context} />;
      case "keyboard":
        return <KeyboardPage context={props.context} />;
      case "focus":
        return <FocusPage context={props.context} />;
      case "datetime":
        return <DateTimePage dbus={dbus} context={props.context} />;
      case "system":
        return <SystemPage dbus={dbus} />;
      case "network":
        return <NetworkPage dbus={dbus} />;
      case "passkeys":
        return <PasskeysPage context={props.context} />;
      case "users":
        return <UsersPage dbus={dbus} user={props.context.user} />;
      case "resources":
        return <ResourcesPage context={props.context} />;
      case "power":
        return <PowerPage dbus={dbus} />;
    }
  };

  return (
    <div class="settings" onKeyDown={onFind}>
      <aside class="settings-side">
        <div class="settings-filter">
          <input
            type="search"
            placeholder="Filter settings"
            aria-label="Filter settings"
            autocomplete="off"
            spellcheck={false}
            value={filter()}
            onInput={(event) => setFilter(event.currentTarget.value)}
            onKeyDown={onFilterKey}
            ref={(el) => {
              filterInput = el;
            }}
          />
        </div>
        <nav class="settings-nav" aria-label="Settings pages" onKeyDown={onKey}>
          <For each={PAGES}>
            {(each) => (
              <button
                type="button"
                data-page={each.id}
                aria-current={!matched() && page() === each.id ? "page" : undefined}
                tabindex={page() === each.id ? 0 : -1}
                onClick={() => choose(each.id)}
              >
                {each.label}
              </button>
            )}
          </For>
        </nav>
      </aside>
      <main class="settings-main">
        <SectionFilterContext value={matched}>
          <For each={shown()}>{(each) => view(each.id)}</For>
        </SectionFilterContext>
        <Show when={shown().length === 0}>
          <p class="settings-empty" role="status">
            No settings match
          </p>
        </Show>
      </main>
    </div>
  );
}
