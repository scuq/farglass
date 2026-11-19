import { createSignal, createUniqueId, For } from "solid-js";
import { APPS } from "../apps";
import { search } from "./search";

/**
 * The launcher: a search field and the apps that match.
 *
 * The arrow keys select an app, and Enter opens it. A click opens it too.
 */
export function Launcher(props: { onopen: (appId: string) => void }) {
  const [query, setQuery] = createSignal("");
  const [active, setActive] = createSignal(0);
  const listId = createUniqueId();
  const results = () => search(Object.values(APPS), query());

  const open = (appId: string | undefined) => {
    if (appId) {
      props.onopen(appId);
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    const count = results().length;
    if (event.key === "ArrowDown" && count > 0) {
      event.preventDefault();
      setActive((active() + 1) % count);
    } else if (event.key === "ArrowUp" && count > 0) {
      event.preventDefault();
      setActive((active() - 1 + count) % count);
    } else if (event.key === "Enter") {
      event.preventDefault();
      open(results()[Math.min(active(), count - 1)]?.id);
    }
  };

  return (
    <div class="launcher">
      <input
        class="launcher-search"
        type="search"
        placeholder="Search apps"
        aria-label="Search apps"
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-activedescendant={results().length > 0 ? `${listId}-${results()[Math.min(active(), results().length - 1)]!.id}` : undefined}
        autocomplete="off"
        spellcheck={false}
        ref={(el) => queueMicrotask(() => el.focus())}
        onInput={(event) => {
          setQuery(event.currentTarget.value);
          setActive(0);
        }}
        onKeyDown={onKeyDown}
      />
      <ul class="launcher-results" id={listId} role="listbox" aria-label="Apps">
        <For each={results()} fallback={<li class="launcher-empty">No app matches.</li>}>
          {(app, index) => (
            <li
              id={`${listId}-${app.id}`}
              role="option"
              aria-selected={String(index() === active()) as "true" | "false"}
              onPointerMove={() => setActive(index())}
              onClick={() => open(app.id)}
            >
              <span class="launcher-title">{app.title}</span>
              {app.description ? <span class="launcher-description">{app.description}</span> : null}
            </li>
          )}
        </For>
      </ul>
    </div>
  );
}
