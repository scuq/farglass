import { createSignal, For, Show } from "solid-js";
import { bookmarks as activeBookmarks, browserLabel, CLIENT, describe, TERMINAL } from "../../launcher/bookmarks";
import type { Bookmark, BookmarkFile } from "../../proto";
import { ConfirmDialog, type Outcome, OutcomeLine } from "../settings/common";
import "../settings/settings.css";
import "./bookmarks.css";
import type { AppContext } from "../types";

/** A bookmark in the form: the profile is "" for the default profile. */
interface Draft {
  name: string;
  url: string;
  browser: string;
  profile: string;
  private: boolean;
}

const EMPTY: Draft = { name: "", url: "https://", browser: CLIENT, profile: "", private: false };

function draftOf(bookmark: Bookmark): Draft {
  return { name: bookmark.name, url: bookmark.url, browser: bookmark.browser, profile: bookmark.profile ?? "", private: bookmark.private ?? false };
}

function bookmarkOf(draft: Draft): Bookmark {
  const bookmark: Bookmark = { name: draft.name.trim(), url: draft.url.trim(), browser: draft.browser };
  const browser = draft.browser !== CLIENT && draft.browser !== TERMINAL;
  if (draft.profile && browser) bookmark.profile = draft.profile;
  if (draft.private && browser) bookmark.private = true;
  return bookmark;
}

/**
 * The bookmarks app (F100): URLs with short names that open in a browser of the
 * host, in a profile of it, or in a private window, or in a tab of this browser.
 *
 * A terminal bookmark (F110) opens a terminal window: ssh to a host, or the shell in a folder.
 *
 * The file bookmarks.toml in the agent holds the bookmarks. A change by hand
 * shows when the window gets the focus. A save keeps the comments of the file.
 */
export default function BookmarksApp(props: { context: AppContext }) {
  const client = activeBookmarks();
  const [outcome, setOutcome] = createSignal<Outcome>(null);
  /** The index of the bookmark in the form: -1 for a new one, null with no form. */
  const [editing, setEditing] = createSignal<number | null>(null);
  const [draft, setDraft] = createSignal<Draft>(EMPTY);
  const [removing, setRemoving] = createSignal<number | null>(null);
  const [opening, setOpening] = createSignal<string | null>(null);
  /** In the edit mode, each row shows its Edit and Delete buttons. */
  const [editMode, setEditMode] = createSignal(false);

  if (!client) {
    return <p class="bookmarks-empty">The bookmarks are not available: the desktop has no connection to the host.</p>;
  }
  client.refresh();
  props.context.onFocus(() => client.refresh());

  const state = () => client.state();
  const file = (): BookmarkFile => state()?.file ?? { launcher: false, bookmarks: [] };
  const problemOf = (index: number) => state()?.problems.find((problem) => problem.index === index)?.message;
  // An updater: two changes in one tick both stay.
  const change = (patch: Partial<Draft>) => setDraft((current) => ({ ...current, ...patch }));

  const save = (next: BookmarkFile, done: string) => {
    setOutcome(null);
    return client.save(next).then(
      () => {
        setOutcome({ kind: "ok", text: done });
        return true;
      },
      (message: string) => {
        setOutcome({ kind: "error", text: message });
        return false;
      },
    );
  };

  const open = (bookmark: Bookmark) => {
    if (opening()) return;
    setOutcome(null);
    setOpening(bookmark.name);
    client.open(bookmark).then(
      () => {
        setOpening(null);
        setOutcome({
          kind: "ok",
          text:
            bookmark.browser === CLIENT
              ? `${bookmark.name} opened in a new tab of this browser.`
              : bookmark.browser === TERMINAL
                ? `${bookmark.name} opened in a terminal window.`
                : `${bookmark.name} opened in ${browserLabel(state(), bookmark.browser)}.`,
        });
      },
      (message: string) => {
        setOpening(null);
        setOutcome({ kind: "error", text: message });
      },
    );
  };

  const startEdit = (index: number) => {
    const bookmark = file().bookmarks[index];
    setDraft(bookmark ? draftOf(bookmark) : { ...EMPTY });
    setEditing(index);
    setOutcome(null);
  };

  const submit = (event: SubmitEvent) => {
    event.preventDefault();
    const index = editing();
    if (index === null) return;
    const bookmark = bookmarkOf(draft());
    const list = [...file().bookmarks];
    if (index < 0) list.push(bookmark);
    else list[index] = bookmark;
    void save({ ...file(), bookmarks: list }, `${bookmark.name} is saved.`).then((saved) => {
      if (saved) setEditing(null);
    });
  };

  const remove = (index: number) => {
    const name = file().bookmarks[index]?.name ?? "";
    setRemoving(null);
    void save({ ...file(), bookmarks: file().bookmarks.filter((_, each) => each !== index) }, `${name} is deleted.`);
  };

  /** The browsers of the form: the installed ones, the client browser, and the value of the bookmark if it is not installed. */
  const browserChoices = () => {
    const installed = (state()?.browsers ?? []).map((browser) => ({ id: browser.id, label: browser.name }));
    const choices = [...installed, { id: CLIENT, label: "This computer (a new tab of this browser)" }, { id: TERMINAL, label: "Terminal on the host (ssh or a folder)" }];
    const current = draft().browser;
    if (!choices.some((choice) => choice.id === current)) choices.push({ id: current, label: `${current} (not installed)` });
    return choices;
  };
  const profileChoices = () => {
    const profiles = [...(state()?.browsers.find((browser) => browser.id === draft().browser)?.profiles ?? [])];
    const current = draft().profile;
    if (current && !profiles.includes(current)) profiles.push(current);
    return profiles;
  };

  return (
    <section class="bookmarks">
      <header class="bookmarks-bar">
        <label class="settings-choice">
          <input type="checkbox" checked={file().launcher} disabled={!state() || state()?.error !== undefined} onChange={(event) => void save({ ...file(), launcher: event.currentTarget.checked }, event.currentTarget.checked ? "The launcher shows the bookmarks." : "The launcher does not show the bookmarks.")} />
          Show in launcher search
        </label>
        <span class="bookmarks-spacer" />
        <button type="button" onClick={() => client.refresh()}>
          Reload
        </button>
        <button type="button" disabled={!state() || state()?.error !== undefined} aria-pressed={editMode() ? "true" : "false"} onClick={() => setEditMode(!editMode())}>
          {editMode() ? "Done" : "Edit"}
        </button>
        <button type="button" disabled={!state() || state()?.error !== undefined} onClick={() => startEdit(-1)}>
          Add
        </button>
      </header>
      <Show when={state()?.error}>
        {(error) => (
          <p class="bookmarks-error" role="alert">
            The file is not valid, thus farglass does not change it. Correct the file and select Reload. {error()}
          </p>
        )}
      </Show>
      <Show when={editing() !== null}>
        <form class="bookmarks-form" onSubmit={submit} aria-label={editing() === -1 ? "New bookmark" : "Edit bookmark"}>
          <label for="bookmark-name">Short name</label>
          <input id="bookmark-name" required value={draft().name} autocomplete="off" spellcheck={false} ref={(el) => queueMicrotask(() => el.focus())} onInput={(event) => change({ name: event.currentTarget.value })} />
          <label for="bookmark-url">{draft().browser === TERMINAL ? "ssh URL or folder" : "URL"}</label>
          <input
            id="bookmark-url"
            type={draft().browser === TERMINAL ? "text" : "url"}
            required
            pattern={draft().browser === TERMINAL ? "(ssh://.+|/.*|~|~/.*|file:///.*)" : "https?://.+"}
            title={draft().browser === TERMINAL ? "ssh://user@host:port, or a folder: /path, ~/path, or file:///path" : undefined}
            placeholder={draft().browser === TERMINAL ? "ssh://admin@192.0.2.1:22 or ~/src/project" : undefined}
            value={draft().url}
            autocomplete="off"
            spellcheck={false}
            onInput={(event) => change({ url: event.currentTarget.value })}
          />
          <label for="bookmark-browser">Browser</label>
          <select
            id="bookmark-browser"
            value={draft().browser}
            onChange={(event) => {
              const browser = event.currentTarget.value;
              // An http URL is no terminal start, and a terminal start is no http URL. The updater
              // reads the draft of this tick: a change of the URL in the same tick stays.
              setDraft((current) => {
                const http = /^https?:/.test(current.url);
                const fits = browser === TERMINAL ? !http : http;
                return { ...current, browser, profile: "", url: fits ? current.url : browser === TERMINAL ? "ssh://" : "https://" };
              });
            }}
          >
            <For each={browserChoices()}>{(choice) => <option value={choice.id} selected={choice.id === draft().browser}>{choice.label}</option>}</For>
          </select>
          <Show when={draft().browser !== CLIENT && draft().browser !== TERMINAL}>
            <label for="bookmark-profile">Profile</label>
            <select id="bookmark-profile" value={draft().profile} onChange={(event) => change({ profile: event.currentTarget.value })}>
              <option value="" selected={draft().profile === ""}>
                Default profile
              </option>
              <For each={profileChoices()}>{(profile) => <option value={profile} selected={profile === draft().profile}>{profile}</option>}</For>
            </select>
            <span />
            <label class="settings-choice">
              <input type="checkbox" checked={draft().private} onChange={(event) => change({ private: event.currentTarget.checked })} />
              Private window
            </label>
          </Show>
          <Show when={draft().browser === CLIENT}>
            <span />
            <p class="bookmarks-note">The URL opens on this computer, not on the host.</p>
          </Show>
          <Show when={draft().browser === TERMINAL}>
            <span />
            <p class="bookmarks-note">A new terminal window opens on the host. An ssh URL starts ssh to the host. A folder starts your shell in that folder.</p>
          </Show>
          <div class="settings-buttons bookmarks-form-buttons">
            <button type="button" onClick={() => setEditing(null)}>
              Cancel
            </button>
            <button type="submit">Save</button>
          </div>
        </form>
      </Show>
      <Show
        when={state()}
        fallback={
          <Show when={client.closed()} fallback={<p class="bookmarks-empty" role="status">Loading</p>}>
            {(message) => (
              <p class="bookmarks-error" role="alert">
                {message()}
              </p>
            )}
          </Show>
        }
      >
        <ul class="bookmarks-list" aria-label="Bookmarks" aria-busy={opening() ? "true" : undefined}>
          <For each={file().bookmarks} fallback={<li class="bookmarks-empty">No bookmarks yet. Select Add, or edit the file {state()?.path}.</li>}>
            {(bookmark, index) => (
              <li class="bookmarks-row" data-problem={problemOf(index()) ? "true" : undefined}>
                <button type="button" class="bookmarks-open" disabled={problemOf(index()) !== undefined} onClick={() => open(bookmark)} title={bookmark.url}>
                  <span class="bookmarks-name">{bookmark.name}</span>
                  <span class="bookmarks-line">{describe(state(), bookmark)}</span>
                  <Show when={bookmark.browser === CLIENT}>
                    <span class="bookmarks-line">Opens on this computer, not on the host</span>
                  </Show>
                  <Show when={problemOf(index())}>{(problem) => <span class="bookmarks-problem">{problem()}</span>}</Show>
                </button>
                <Show when={editMode()}>
                  <button type="button" disabled={state()?.error !== undefined} onClick={() => startEdit(index())} aria-label={`Edit ${bookmark.name}`}>
                    Edit
                  </button>
                  <button type="button" disabled={state()?.error !== undefined} onClick={() => setRemoving(index())} aria-label={`Delete ${bookmark.name}`}>
                    Delete
                  </button>
                </Show>
              </li>
            )}
          </For>
        </ul>
        <p class="bookmarks-path">
          File: <code>{state()?.path}</code>
        </p>
      </Show>
      <OutcomeLine outcome={outcome()} />
      <Show when={removing() !== null}>
        <ConfirmDialog title="Delete the bookmark?" action="Delete" onconfirm={() => remove(removing() ?? -1)} oncancel={() => setRemoving(null)}>
          <p>
            farglass removes <strong>{file().bookmarks[removing() ?? -1]?.name}</strong> from the file. Make a copy of the file if you want to keep it.
          </p>
        </ConfirmDialog>
      </Show>
    </section>
  );
}
