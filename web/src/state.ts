/**
 * The state store of the session in the agent. The shell keeps its state
 * there and gets it back after a reload or a break of the connection.
 */

import type { Channel, Connection, StateEvent, StateRequest } from "./proto";

/** One state channel. Each key that the shell asks for also gives its changes from other tabs. */
export class StateClient {
  readonly #channel: Channel;
  readonly #watchers = new Map<string, Set<(value: unknown) => void>>();

  constructor(connection: Connection) {
    this.#channel = connection.openChannel(
      { kind: "state" },
      {
        oncontrol: (message) => {
          const event = message as StateEvent;
          if (event.type === "value") {
            for (const watcher of this.#watchers.get(event.key) ?? []) {
              watcher(event.value);
            }
          }
        },
      },
    );
  }

  /**
   * Asks for the value of `key`. `onvalue` gets the current value, or `null`,
   * and then each change that a different tab makes. Returns a function that stops the calls.
   */
  watch(key: string, onvalue: (value: unknown) => void): () => void {
    let watchers = this.#watchers.get(key);
    if (!watchers) {
      watchers = new Set();
      this.#watchers.set(key, watchers);
    }
    watchers.add(onvalue);
    const request: StateRequest = { type: "get", key };
    this.#channel.control(request);
    return () => watchers.delete(onvalue);
  }

  /** Sets the value of `key`. `null` removes the key. Other tabs that watch the key get the value. */
  set(key: string, value: unknown): void {
    const request: StateRequest = { type: "set", key, value };
    this.#channel.control(request);
  }

  /** Closes the channel. */
  close(): void {
    this.#channel.close();
  }
}
