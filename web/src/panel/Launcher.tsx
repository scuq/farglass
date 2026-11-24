import { createMemo, createSignal, createUniqueId, For, Show } from "solid-js";
import { APPS } from "../apps/registry";
import { PAGES } from "../apps/settings/pages";
import { bookmarks, describe } from "../launcher/bookmarks";
import { calculate } from "../launcher/calc";
import calcIcon from "../launcher/calc.svg";
import { hostApps, isShellIcon } from "../launcher/hostApps";
import { EMPTY_LAUNCHER, frequent, type LauncherState, recordUse, togglePin } from "../launcher/usage";
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
  /** The page of the settings that the query found (F042). The entry of the settings opens at this page. */
  readonly setting?: string | undefined;
  /** The value of a calculation or a conversion, to copy (F042). */
  readonly calc?: string;
  /** The name of a bookmark (F100). */
  readonly bookmark?: string;
  readonly preload?: () => void;
}

/** A row of the list: the label of a section, or an entry with its position in the selection order. */
type Row = { readonly label: string } | { readonly entry: Entry; readonly index: number };

const PAGE_ENTRIES = PAGES.map((page) => ({ id: page.id, title: page.label, keywords: page.keywords }));
/** The settings entry finds each page: the labels and the keywords of the pages are its keywords. */
const PAGE_KEYWORDS = PAGES.flatMap((page) => [page.label, ...page.keywords]);

/** The icon of an app of the host with no icon of its own. */
const HOST_ICON = APPS.linux?.icon;
/** The icon of a bookmark. */
const BOOKMARK_ICON = APPS.bookmarks?.icon;

/**
 * The launcher: one search field for the apps of the shell, the desktop entries
 * of the host, the pages of the settings, and calculations (F042).
 *
 * With no query, the pinned apps come first, then the most used apps, then
 * all other apps (F076). Alt+P or the button of an entry pins it or removes
 * the pin. The settings are one entry: a query that finds a page of the
 * settings opens the settings at that page.
 *
 * The arrow keys select a result, and Enter opens it. A click opens it too.
 * An app of the host starts on the compositor of the session. The launcher
 * closes when it started, or shows why it did not. A calculation or a unit
 * conversion is the first result while you type, and Enter copies its value.
 */
export function Launcher(props: {
  onopen: (appId: string) => void;
  onclose: () => void;
  onlaunch?: (names: readonly string[]) => void;
  onsetting?: (page: string) => void;
  /** The pins and the use of the apps, and the function that saves a change (F076). */
  usage?: LauncherState;
  onusage?: (next: LauncherState) => void;
}) {
  const [query, setQuery] = createSignal("");
  const [active, setActive] = createSignal(0);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [starting, setStarting] = createSignal<string | null>(null);
  const listId = createUniqueId();
  // The desktop entries can change while the shell runs, for example after an install.
  hostApps()?.refresh();
  // The bookmarks file can change by hand.
  bookmarks()?.refresh();

  const usage = () => props.usage ?? EMPTY_LAUNCHER;

  /** The page of the settings that the query finds, if the query does not find the settings app by its name. */
  const settingsPage = () => {
    const text = query().trim();
    const app = APPS.settings;
    if (!text || !app || search([{ id: app.id, title: app.title }], text).length > 0) return undefined;
    return search(PAGE_ENTRIES, text)[0];
  };

  const entries = (): Entry[] => {
    const page = settingsPage();
    const own: Entry[] = Object.values(APPS)
      .filter((app) => !app.hidden)
      .map((app) => ({
        id: app.id,
        title: app.title,
        keywords: app.id === "settings" ? [...(app.keywords ?? []), ...PAGE_KEYWORDS] : app.keywords,
        description: app.id === "settings" && page ? `Opens the page ${page.title}` : app.description,
        icon: () => app.icon,
        setting: app.id === "settings" ? page?.id : undefined,
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
    // Only with the option of the file: the default is off (F100).
    const marks = bookmarks();
    const marked: Entry[] = (marks?.launcherBookmarks() ?? []).map((bookmark) => {
      let host = bookmark.url;
      try {
        host = new URL(bookmark.url).hostname;
      } catch {
        // The agent marks the bookmark as not valid; the launcher shows it as it is.
      }
      return {
        id: `bookmark:${bookmark.name}`,
        title: bookmark.name,
        keywords: [host, "bookmark"],
        description: describe(marks?.state() ?? null, bookmark),
        icon: () => BOOKMARK_ICON,
        bookmark: bookmark.name,
      };
    });
    return [...own, ...host, ...marked];
  };
  /** The rows of the list. A query gives one list. No query gives the sections, if an app is pinned or was used. */
  const rows = createMemo((): Row[] => {
    const found = search(entries(), query());
    const sections: { label?: string; entries: Entry[] }[] = [];
    const result = calculate(query());
    if (query().trim()) {
      const calc: Entry[] = result ? [{ id: "calc", title: result.text, description: "Enter copies the result", icon: () => calcIcon, calc: result.value }] : [];
      sections.push({ entries: [...calc, ...found] });
    } else {
      const byId = new Map(found.map((entry) => [entry.id, entry]));
      const pick = (ids: readonly string[]) => ids.flatMap((id) => byId.get(id) ?? []);
      const pinned = pick(usage().pinned);
      const often = pick(frequent(usage(), Date.now()));
      if (pinned.length === 0 && often.length === 0) {
        sections.push({ entries: found });
      } else {
        const top = new Set([...pinned, ...often]);
        sections.push({ label: "Pinned", entries: pinned }, { label: "Most used", entries: often }, { label: "All apps", entries: found.filter((entry) => !top.has(entry)) });
      }
    }
    let index = 0;
    return sections
      .filter((section) => section.entries.length > 0)
      .flatMap((section): Row[] => [...(section.label ? [{ label: section.label }] : []), ...section.entries.map((entry) => ({ entry, index: index++ }))]);
  });
  const results = (): Entry[] => rows().flatMap((row) => ("entry" in row ? [row.entry] : []));
  const selected = () => results()[Math.min(active(), results().length - 1)];

  const isPinned = (entry: Entry) => usage().pinned.includes(entry.id);
  const used = (entry: Entry) => props.onusage?.(recordUse(usage(), entry.id, Date.now()));
  /** Pins the entry, or removes its pin. The selection stays on the entry at its new place. */
  const pin = (entry: Entry | undefined) => {
    if (!entry || entry.calc !== undefined) return;
    props.onusage?.(togglePin(usage(), entry.id));
    queueMicrotask(() => {
      const index = results().findIndex((each) => each.id === entry.id);
      if (index >= 0) setActive(index);
    });
  };

  const open = (entry: Entry | undefined) => {
    if (!entry) return;
    if (entry.calc !== undefined) {
      const copied = navigator.clipboard?.writeText(entry.calc) ?? Promise.reject(new Error("no clipboard"));
      copied.then(
        () => props.onclose(),
        () => setFailure(`The browser did not let the launcher copy ${entry.calc}.`),
      );
      return;
    }
    if (entry.bookmark !== undefined) {
      const marks = bookmarks();
      const bookmark = marks?.launcherBookmarks().find((each) => each.name === entry.bookmark);
      if (!marks || !bookmark || starting()) return;
      setFailure(null);
      setStarting(entry.id);
      // In the handler of the key or the click: a bookmark of this browser opens a tab at once.
      marks.open(bookmark).then(
        () => {
          setStarting(null);
          used(entry);
          props.onclose();
        },
        (message: string) => {
          setStarting(null);
          setFailure(message);
        },
      );
      return;
    }
    if (entry.setting) {
      used(entry);
      props.onsetting?.(entry.setting);
      return;
    }
    if (!entry.host) {
      used(entry);
      props.onopen(entry.id);
      return;
    }
    const apps = hostApps();
    if (!apps || starting()) return;
    setFailure(null);
    setStarting(entry.id);
    // The window of the app gets the focus: the user asked for it (F041).
    props.onlaunch?.(apps.apps().find((app) => app.id === entry.host)?.names ?? []);
    apps.launch(entry.host).then(
      () => {
        setStarting(null);
        used(entry);
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
      open(selected());
    } else if (event.altKey && event.code === "KeyP") {
      // The code, not the key: Alt+P gives a different character on a Mac.
      event.preventDefault();
      pin(selected());
    }
  };

  const optionId = (entry: Entry) => `${listId}-${entry.id.replace(/[^A-Za-z0-9_-]/g, "_")}`;

  return (
    <div class="launcher">
      <input
        class="launcher-search"
        type="search"
        placeholder="Search apps and settings, or calculate"
        aria-label="Search apps and settings, or calculate"
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-activedescendant={selected() ? optionId(selected()!) : undefined}
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
        <For each={rows()} fallback={<li class="launcher-empty">No app matches.</li>}>
          {(row) => {
            if ("label" in row) {
              return (
                <li class="launcher-section" role="presentation">
                  {row.label}
                </li>
              );
            }
            const { entry, index } = row;
            return (
              <li
                id={optionId(entry)}
                role="option"
                aria-selected={String(index === active()) as "true" | "false"}
                data-host={entry.host ? "true" : undefined}
                data-starting={starting() === entry.id ? "true" : undefined}
                onPointerMove={() => setActive(index)}
                // The module of the app starts to load before the click.
                onPointerEnter={() => entry.preload?.()}
                onClick={() => open(entry)}
              >
                <img class="launcher-icon" src={entry.icon()} alt="" data-shell-icon={isShellIcon(entry.icon()) ? "true" : undefined} />
                <span class="launcher-text">
                  <span class="launcher-title">{entry.title}</span>
                  {entry.description ? <span class="launcher-description">{entry.description}</span> : null}
                </span>
                {entry.calc === undefined ? (
                  <button
                    type="button"
                    class="launcher-pin"
                    tabindex={-1}
                    aria-label={`${isPinned(entry) ? "Remove the pin of" : "Pin"} ${entry.title}`}
                    // The keyboard focus stays in the search field.
                    onPointerDown={(event) => event.preventDefault()}
                    onClick={(event) => {
                      event.stopPropagation();
                      pin(entry);
                    }}
                  >
                    {isPinned(entry) ? "Unpin" : "Pin"}
                  </button>
                ) : null}
              </li>
            );
          }}
        </For>
      </ul>
      <p class="launcher-hint">Alt+P pins the selected app, or removes its pin.</p>
    </div>
  );
}
