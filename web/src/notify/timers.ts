/**
 * The times to show of the notifications. A time runs only while the
 * notification is shown and the user does not look at it: hover, focus, and a
 * hidden browser tab pause it.
 */
export class Timers {
  readonly #onexpire: (id: number) => void;
  readonly #items = new Map<number, { remaining: number; started: number | null; timer: ReturnType<typeof setTimeout> | null }>();
  #paused = false;

  constructor(onexpire: (id: number) => void) {
    this.#onexpire = onexpire;
  }

  /** Starts the time of `id` with `ms` again. `null` means no end. */
  set(id: number, ms: number | null): void {
    this.remove(id);
    if (ms === null) {
      return;
    }
    this.#items.set(id, { remaining: ms, started: null, timer: null });
    if (!this.#paused) {
      this.#run(id);
    }
  }

  /** Removes the time of `id`. */
  remove(id: number): void {
    const item = this.#items.get(id);
    if (item?.timer != null) {
      clearTimeout(item.timer);
    }
    this.#items.delete(id);
  }

  /** Reports whether `id` has a running time. */
  has(id: number): boolean {
    return this.#items.has(id);
  }

  /** Stops all times. They continue with the time that remains. */
  pause(): void {
    if (this.#paused) {
      return;
    }
    this.#paused = true;
    const now = performance.now();
    for (const item of this.#items.values()) {
      if (item.timer != null && item.started != null) {
        clearTimeout(item.timer);
        item.remaining = Math.max(0, item.remaining - (now - item.started));
        item.timer = null;
        item.started = null;
      }
    }
  }

  /** Continues all times. */
  resume(): void {
    if (!this.#paused) {
      return;
    }
    this.#paused = false;
    for (const id of this.#items.keys()) {
      this.#run(id);
    }
  }

  /** Removes all times. */
  clear(): void {
    for (const id of [...this.#items.keys()]) {
      this.remove(id);
    }
  }

  #run(id: number): void {
    const item = this.#items.get(id);
    if (!item) {
      return;
    }
    item.started = performance.now();
    item.timer = setTimeout(() => {
      this.#items.delete(id);
      this.#onexpire(id);
    }, item.remaining);
  }
}
