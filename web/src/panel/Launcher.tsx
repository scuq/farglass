import { createSignal, createUniqueId, For, Show } from "solid-js";
import { APPS } from "../apps/registry";
import { hostApps } from "../launcher/hostApps";
import { search } from "./search";

/** One entry of the launcher: an app of the shell, or an app of the host (F031). */
interface Entry {
  readonly id: string;
  readonly title: string;
  readonly keywords?: readonly string[] | undefined;
  readonly description?: string | undefined;
  readonly icon: () => string | undefined;
  /** The id of the desktop entry of an app of the host. */
  readonly host?: string;
  readonly preload?: () => void;
}

/** The icon of an app of the host with no icon of its own. */
const HOST_ICON = APPS.linux?.icon;

/**
 * The launcher: a search field and the apps that match: the apps of the shell
 * and the desktop entries of the host.
 *
 * The arrow keys select an app, and Enter opens it. A click opens it too.
 * An app of the host starts on the compositor of the session. The launcher
 * closes when it started, or shows why it did not.
 */
export function Launcher(props: { onopen: (appId: string) => void; onclose: () => void }) {
  const [query, setQuery] = createSignal("");
  const [active, setActive] = createSignal(0);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [starting, setStarting] = createSignal<string | null>(null);
  const listId = createUniqueId();
  // The desktop entries can change while the shell runs, for example after an install.
  hostApps()?.refresh();

  const entries = (): Entry[] => {
    const own: Entry[] = Object.values(APPS)
      .filter((app) => !app.hidden)
      .map((app) => ({
        id: app.id,
        title: app.title,
        keywords: app.keywords,
        description: app.description,
        icon: () => app.icon,
        preload: () => void app.component.preload(),
      }));
    const apps = hostApps();
    const host: Entry[] = (apps?.apps() ?? []).map((app) => ({
      id: `host:${app.id}`,
      title: app.name,
      keywords: app.keywords,
      description: app.comment,
      icon: () => apps?.icon(app.id) ?? HOST_ICON,
      host: app.id,
    }));
    return [...own, ...host];
  };
  const results = () => search(entries(), query());

  const open = (entry: Entry | undefined) => {
    if (!entry) return;
    if (!entry.host) {
      props.onopen(entry.id);
      return;
    }
    const apps = hostApps();
    if (!apps || starting()) return;
    setFailure(null);
    setStarting(entry.id);
    apps.launch(entry.host).then(
      () => {
        setStarting(null);
        props.onclose();
      },
      (message: string) => {
        setStarting(null);
        setFailure(message);
      },
    );
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
      open(results()[Math.min(active(), count - 1)]);
    }
  };

  const optionId = (entry: Entry) => `${listId}-${entry.id.replace(/[^A-Za-z0-9_-]/g, "_")}`;

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
        aria-activedescendant={results().length > 0 ? optionId(results()[Math.min(active(), results().length - 1)]!) : undefined}
        autocomplete="off"
        spellcheck={false}
        ref={(el) => queueMicrotask(() => el.focus())}
        onInput={(event) => {
          setQuery(event.currentTarget.value);
          setActive(0);
          setFailure(null);
        }}
        onKeyDown={onKeyDown}
      />
      <Show when={failure()}>{(message) => <p class="launcher-failure" role="alert">{message()}</p>}</Show>
      <ul class="launcher-results" id={listId} role="listbox" aria-label="Apps" aria-busy={starting() ? "true" : undefined}>
        <For each={results()} fallback={<li class="launcher-empty">No app matches.</li>}>
          {(entry, index) => (
            <li
              id={optionId(entry)}
              role="option"
              aria-selected={String(index() === active()) as "true" | "false"}
              data-host={entry.host ? "true" : undefined}
              data-starting={starting() === entry.id ? "true" : undefined}
              onPointerMove={() => setActive(index())}
              // The module of the app starts to load before the click.
              onPointerEnter={() => entry.preload?.()}
              onClick={() => open(entry)}
            >
              <img class="launcher-icon" src={entry.icon()} alt="" />
              <span class="launcher-text">
                <span class="launcher-title">{entry.title}</span>
                {entry.description ? <span class="launcher-description">{entry.description}</span> : null}
              </span>
            </li>
          )}
        </For>
      </ul>
    </div>
  );
}
