/**
 * The client of the netcheck channel (F111): the checks files in the agent and the results of the checks.
 *
 * The agent runs each check on the host, thus a result shows the path of the host, not of this computer.
 */

import { createSignal } from "solid-js";
import type { Channel, ChannelHandlers, NetcheckCheck, NetcheckEvent, NetcheckGroup, NetcheckItem, NetcheckProblem, NetcheckRequest, Open } from "../../proto";

/**
 * The states of a check. This table is the only source of the word, the mark, and the level of each state:
 * the color repeats the level, and the word and the mark carry the meaning without the color.
 */
export const STATUS = {
  "not-checked": { label: "Not checked", mark: "○", level: "unknown" },
  checking: { label: "Checking", mark: "◌", level: "unknown" },
  ok: { label: "Available", mark: "✓", level: "ok" },
  failed: { label: "Failed", mark: "✕", level: "bad" },
  timeout: { label: "No answer", mark: "⧗", level: "bad" },
} as const;

export type CheckStatus = keyof typeof STATUS;

/** A result is old after this time: the page shows its age and dims it. */
export const STALE_MS = 5 * 60_000;

/** The result of one check. */
export interface CheckResult {
  readonly status: CheckStatus;
  /** The time to the answer in ms, for a check that passed. */
  readonly ms?: number | undefined;
  readonly detail: string;
  /** The time of the result, from Date.now(). */
  readonly at: number;
}

/** The checks of the agent. */
export interface NetcheckState {
  readonly path: string;
  readonly adminPath: string;
  readonly user: readonly NetcheckCheck[];
  readonly items: readonly NetcheckItem[];
  readonly problems: readonly NetcheckProblem[];
  readonly proxy?: string | undefined;
  /** Why the file of the user is not valid TOML. The agent does not save over it. */
  readonly error?: string | undefined;
}

type Waiter = { resolve: () => void; reject: (message: string) => void };

/** Opens a channel. Closing the window closes it. */
export type ChannelOpener = (open: Open, handlers?: ChannelHandlers) => Channel;

export class NetcheckClient {
  readonly #channel: Channel;
  readonly #state = createSignal<NetcheckState | null>(null);
  readonly #results = createSignal<Readonly<Record<string, CheckResult>>>({});
  readonly #closed = createSignal<string | null>(null);
  #saves: Waiter[] = [];
  /** Runs all checks after the next load, for example the first one. */
  #runAfterLoad = true;
  readonly #now: () => number;

  constructor(openChannel: ChannelOpener, now: () => number = Date.now) {
    this.#now = now;
    this.#channel = openChannel(
      { kind: "netcheck" },
      {
        oncontrol: (message) => this.#event(message as NetcheckEvent),
        onclose: (close) => {
          const message =
            close.reason === "not-supported"
              ? "The agent of this session is older than the shell and has no connection checks. Restart farglass-session, or log out and log in again."
              : `The connection checks are not available: ${close.message ?? close.reason}.`;
          this.#closed[1](message);
          for (const waiter of this.#saves) waiter.reject(message);
          this.#saves = [];
          // A check that waits gets no result.
          this.#results[1]((results) => Object.fromEntries(Object.entries(results).filter(([, result]) => result.status !== "checking")));
        },
      },
    );
  }

  /** Why the checks are not available, or null while the channel is open. */
  closed(): string | null {
    return this.#closed[0]();
  }

  /** The checks, or null until the agent sent them. */
  state(): NetcheckState | null {
    return this.#state[0]();
  }

  /** The result of each check, by id. A check with no result is not checked. */
  results(): Readonly<Record<string, CheckResult>> {
    return this.#results[0]();
  }

  /** Reads the files again, then runs all checks. */
  reload(): void {
    this.#runAfterLoad = true;
    this.#send({ type: "load" });
  }

  /** Runs the checks `ids`, or all checks. */
  run(ids?: readonly string[]): void {
    this.#run(ids ?? this.state()?.items.map((item) => item.id) ?? [], ids === undefined);
  }

  /** Marks `list` as checking and asks the agent to run it: all checks with `all`. */
  #run(list: readonly string[], all: boolean): void {
    if (list.length === 0 || this.closed()) return;
    const at = this.#now();
    this.#results[1]((results) => {
      const next = { ...results };
      for (const id of list) next[id] = { status: "checking", detail: "", at, ms: undefined };
      return next;
    });
    this.#send(all ? { type: "run" } : { type: "run", ids: [...list] });
  }

  /** Writes the checks of the user, then runs them. Rejects with a message for the user. */
  save(checks: readonly NetcheckCheck[]): Promise<void> {
    return new Promise((resolve, reject) => {
      this.#saves.push({ resolve, reject });
      this.#send({ type: "save", checks: [...checks] });
    });
  }

  #send(request: NetcheckRequest): void {
    this.#channel.control(request);
  }

  #event(event: NetcheckEvent): void {
    switch (event.type) {
      case "loaded": {
        const saved = this.#saves.shift();
        this.#state[1]({ path: event.path, adminPath: event.admin_path, user: event.user, items: event.items, problems: event.problems, proxy: event.proxy, error: event.error });
        // The ids of the checks of a file are positions: after a change of a file, the old results are not valid.
        this.#results[1]((results) => Object.fromEntries(Object.entries(results).filter(([id]) => !id.startsWith("user:") && !id.startsWith("admin:"))));
        if (this.#runAfterLoad) {
          this.#runAfterLoad = false;
          // A signal shows a write only after the update: the list comes from the event.
          this.#run(
            event.items.map((item) => item.id),
            true,
          );
        } else if (saved) {
          this.#run(
            event.items.filter((item) => item.group === "user").map((item) => item.id),
            false,
          );
        }
        saved?.resolve();
        break;
      }
      case "result":
        this.#results[1]((results) => ({ ...results, [event.id]: { status: event.state, ms: event.ms, detail: event.detail, at: this.#now() } }));
        break;
      case "failed":
        this.#saves.shift()?.reject(event.message);
        break;
    }
  }
}

/** The state of a check: its result, or not checked. */
export function statusOf(results: Readonly<Record<string, CheckResult>>, id: string): CheckStatus {
  return results[id]?.status ?? "not-checked";
}

/**
 * The state of a group of checks of which one passing is enough, for example the internet:
 * available if one passed, checking while one runs, and else the state of the first check.
 */
export function anyOf(statuses: readonly CheckStatus[]): CheckStatus {
  if (statuses.length === 0) return "not-checked";
  if (statuses.includes("ok")) return "ok";
  if (statuses.includes("checking")) return "checking";
  if (statuses.includes("not-checked")) return "not-checked";
  return statuses.includes("timeout") && !statuses.includes("failed") ? "timeout" : "failed";
}

/** The summary of the page: the internet, DNS, and the checks of the files. */
export interface Summary {
  readonly internet: CheckStatus;
  readonly dns: CheckStatus;
  /** The checks of the files: how many passed, how many have a result, and how many there are. */
  readonly checks: { readonly ok: number; readonly done: number; readonly total: number };
}

export function summarize(items: readonly NetcheckItem[], results: Readonly<Record<string, CheckResult>>): Summary {
  const of = (group: NetcheckGroup) => items.filter((item) => item.group === group).map((item) => statusOf(results, item.id));
  const files = [...of("admin"), ...of("user")];
  return {
    internet: anyOf(of("internet")),
    dns: anyOf(of("dns")),
    checks: {
      ok: files.filter((status) => status === "ok").length,
      done: files.filter((status) => status === "ok" || status === "failed" || status === "timeout").length,
      total: files.length,
    },
  };
}

/** Returns the age of a result for the user, for example "12 s ago" or "6 min ago". */
export function age(at: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 60) return `${seconds} s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  return `${Math.floor(minutes / 60)} h ago`;
}
