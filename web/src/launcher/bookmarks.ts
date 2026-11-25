/**
 * The bookmarks of the user (F100): the file bookmarks.toml in the agent.
 *
 * The agent reads and writes the file, finds the browsers of the host and their
 * profiles, and starts the browser of a bookmark. A bookmark of the client
 * browser opens here, in a new tab, because only a click can open a tab. A terminal
 * bookmark opens a terminal window of the shell (F110).
 */

import { createSignal } from "solid-js";
import { openInClient } from "../links";
import type { Bookmark, BookmarkBrowser, BookmarkFile, BookmarkProblem, BookmarksEvent, BookmarksRequest, Channel, Connection, PtyStart } from "../proto";

/** The browser value of a bookmark that opens in a tab of the client browser. */
export const CLIENT = "client";
/** The browser value of a bookmark that opens in a terminal window of the shell (F110): an ssh URL or a folder. */
export const TERMINAL = "terminal";

/** Returns the start of the terminal of a terminal bookmark. The agent checks it when the terminal starts. */
export function terminalStart(url: string): PtyStart {
  return url.startsWith("ssh:") ? { type: "ssh", url } : { type: "folder", path: url };
}

/** Opens a terminal window with a start, for a terminal bookmark. `name` is the first title of the window. */
export type TerminalOpener = (start: PtyStart, name: string) => void;

/** The state of the file in the agent. */
export interface BookmarksState {
  readonly path: string;
  readonly file: BookmarkFile;
  readonly browsers: readonly BookmarkBrowser[];
  readonly problems: readonly BookmarkProblem[];
  /** Why the file is not valid TOML. The agent does not save over it. */
  readonly error?: string | undefined;
}

type Waiter = { resolve: () => void; reject: (message: string) => void };

export class Bookmarks {
  readonly #channel: Channel;
  readonly #state = createSignal<BookmarksState | null>(null);
  /** Why the channel stopped, for example an agent of an older version that does not know it. */
  readonly #closed = createSignal<string | null>(null);
  #saves: Waiter[] = [];
  readonly #opens = new Map<string, Waiter[]>();
  readonly #openTerminal: TerminalOpener | undefined;

  constructor(connection: Connection, openTerminal?: TerminalOpener) {
    this.#openTerminal = openTerminal;
    this.#channel = connection.openChannel(
      { kind: "bookmarks" },
      {
        oncontrol: (message) => this.#event(message as BookmarksEvent),
        onclose: (close) => {
          const message =
            close.reason === "not-supported"
              ? "The agent of this session is older than the shell and has no bookmarks. Restart farglass-session, or log out and log in again."
              : `The bookmarks are not available: ${close.message ?? close.reason}.`;
          this.#closed[1](message);
          this.#rejectAll(message);
        },
      },
    );
  }

  /** Why the bookmarks are not available, or null while the channel is open. */
  closed(): string | null {
    return this.#closed[0]();
  }

  #rejectAll(message: string): void {
    for (const waiter of [...this.#saves, ...[...this.#opens.values()].flat()]) waiter.reject(message);
    this.#saves = [];
    this.#opens.clear();
  }

  close(): void {
    this.#channel.close();
    this.#rejectAll("The connection closed.");
  }

  /** The file, or null until the agent sent it. */
  state(): BookmarksState | null {
    return this.#state[0]();
  }

  /** The bookmarks for the launcher: none unless the option of the file is on. */
  launcherBookmarks(): readonly Bookmark[] {
    const file = this.state()?.file;
    return file?.launcher ? file.bookmarks : [];
  }

  /** Reads the file again, for example after a change by hand. */
  refresh(): void {
    this.#send({ type: "load" });
  }

  /** Writes the file. Resolves after the save. Rejects with a message for the user. */
  save(file: BookmarkFile): Promise<void> {
    return new Promise((resolve, reject) => {
      this.#saves.push({ resolve, reject });
      this.#send({ type: "save", file });
    });
  }

  /**
   * Opens `bookmark`. Call it in the handler of a user action: a bookmark of the
   * client browser opens a tab at once. Any other bookmark opens on the host.
   */
  open(bookmark: Bookmark): Promise<void> {
    if (bookmark.browser === TERMINAL) {
      if (!this.#openTerminal) return Promise.reject("The desktop cannot open a terminal window.");
      this.#openTerminal(terminalStart(bookmark.url), bookmark.name);
      return Promise.resolve();
    }
    if (bookmark.browser === CLIENT) {
      return openInClient(bookmark.url) ? Promise.resolve() : Promise.reject(`${bookmark.url} is not an http or https URL.`);
    }
    return new Promise((resolve, reject) => {
      const waiting = this.#opens.get(bookmark.name) ?? [];
      waiting.push({ resolve, reject });
      this.#opens.set(bookmark.name, waiting);
      this.#send({ type: "open", name: bookmark.name });
    });
  }

  #send(request: BookmarksRequest): void {
    this.#channel.control(request);
  }

  #event(event: BookmarksEvent): void {
    switch (event.type) {
      case "loaded":
        this.#state[1]({ path: event.path, file: event.file, browsers: event.browsers, problems: event.problems, error: event.error });
        this.#saves.shift()?.resolve();
        break;
      case "opened":
        this.#opens.get(event.name)?.shift()?.resolve();
        break;
      case "failed":
        if (event.name === undefined) this.#saves.shift()?.reject(event.message);
        else this.#opens.get(event.name)?.shift()?.reject(event.message);
        break;
    }
  }
}

/** Returns the label of the browser of `bookmark`: the name of the desktop entry, or the key. */
export function browserLabel(state: BookmarksState | null, browser: string): string {
  if (browser === CLIENT) return "This computer";
  if (browser === TERMINAL) return "Terminal";
  return state?.browsers.find((each) => each.id === browser)?.name ?? browser;
}

/** Returns a short line about `bookmark` for a list: the browser, the profile, private, and the host of the URL. */
export function describe(state: BookmarksState | null, bookmark: Bookmark): string {
  if (bookmark.browser === TERMINAL) {
    return [browserLabel(state, bookmark.browser), bookmark.url.startsWith("ssh:") ? `ssh ${bookmark.url.replace(/^ssh:\/\//, "").replace(/\/$/, "")}` : bookmark.url].join(" · ");
  }
  let host = bookmark.url;
  try {
    host = new URL(bookmark.url).host;
  } catch {
    // The URL is not valid: the line shows it as it is.
  }
  return [browserLabel(state, bookmark.browser), bookmark.profile, bookmark.private ? "private" : undefined, host].filter(Boolean).join(" · ");
}

// A plain variable: the desktop sets it before its panel and its windows render.
let active: Bookmarks | undefined;

/** Returns the bookmarks of the desktop that shows, or undefined with no desktop. */
export function bookmarks(): Bookmarks | undefined {
  return active;
}

/** Starts the bookmarks for `connection`. Returns the function that stops them. */
export function startBookmarks(connection: Connection, openTerminal?: TerminalOpener): () => void {
  const each = new Bookmarks(connection, openTerminal);
  active = each;
  return () => {
    each.close();
    if (active === each) active = undefined;
  };
}
