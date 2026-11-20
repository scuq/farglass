/**
 * Focus protection (F041): the desktop does not interrupt the user.
 *
 * The input watch knows when the user types or moves the pointer, and when a
 * pause starts. The notifications wait for a pause. A new window of a Linux
 * app gets the focus only if the user asked for it: a start from the launcher,
 * a click or Enter just before it, or a dialog of the window with the focus.
 * Each rule has a switch in the settings, in the state store under {@link FOCUS_KEY}.
 */

/** The key of the settings in the state store. */
export const FOCUS_KEY = "focus-protection";

export interface FocusSettings {
  /** Notifications wait while the user types or moves the pointer. */
  readonly holdNotifications: boolean;
  /** A window that the user did not ask for opens behind, with a mark in the panel. */
  readonly quietWindows: boolean;
}

export const DEFAULT_FOCUS: FocusSettings = { holdNotifications: true, quietWindows: true };

/** Returns valid settings from a value of the state store. Unknown fields get the default. */
export function parseFocusSettings(value: unknown): FocusSettings {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  const flag = (name: keyof FocusSettings) => (typeof record[name] === "boolean" ? (record[name] as boolean) : DEFAULT_FOCUS[name]);
  return { holdNotifications: flag("holdNotifications"), quietWindows: flag("quietWindows") };
}

/** The time with no input that is a pause. */
export const PAUSE_MS = 1500;

/** The longest wait of a notification: it shows also if the user does not pause. */
export const HOLD_MAX_MS = 30_000;

/** The time after a click or Enter in which a new window counts as asked for. */
export const REQUEST_MS = 10_000;

/** The time after a start from the launcher in which a window of that app counts as asked for. */
export const LAUNCH_MS = 30_000;

/** The keys that do not count as typing. */
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "AltGraph"]);

/** The input of the user: when they are busy, when a pause starts, and when they asked for a window. */
export class InputWatch {
  readonly #now: () => number;
  readonly #target: EventTarget;
  #lastInput = -Infinity;
  #lastRequest = -Infinity;
  #timer: ReturnType<typeof setTimeout> | undefined;
  readonly #onPause = new Set<() => void>();
  readonly #launches: { names: readonly string[]; at: number }[] = [];

  readonly #onKey = (event: Event) => {
    const key = (event as KeyboardEvent).key;
    if (MODIFIERS.has(key)) return;
    // Enter starts a command, for example in a terminal. Each other key is typing, and the user did not ask for a window.
    this.#lastRequest = key === "Enter" ? this.#now() : -Infinity;
    this.#input();
  };
  readonly #onPress = () => {
    this.#lastRequest = this.#now();
    this.#input();
  };
  readonly #onMove = () => this.#input();

  constructor(target: EventTarget = window, now: () => number = () => performance.now()) {
    this.#target = target;
    this.#now = now;
    target.addEventListener("keydown", this.#onKey, { capture: true, passive: true });
    target.addEventListener("pointerdown", this.#onPress, { capture: true, passive: true });
    target.addEventListener("pointermove", this.#onMove, { capture: true, passive: true });
    target.addEventListener("wheel", this.#onMove, { capture: true, passive: true });
  }

  stop(): void {
    this.#target.removeEventListener("keydown", this.#onKey, { capture: true });
    this.#target.removeEventListener("pointerdown", this.#onPress, { capture: true });
    this.#target.removeEventListener("pointermove", this.#onMove, { capture: true });
    this.#target.removeEventListener("wheel", this.#onMove, { capture: true });
    clearTimeout(this.#timer);
    this.#onPause.clear();
  }

  /** Reports whether the user typed or moved the pointer in the last {@link PAUSE_MS}. */
  busy(): boolean {
    return this.#now() - this.#lastInput < PAUSE_MS;
  }

  /** Calls `handler` at the start of each pause. Returns the function that stops the calls. */
  onPause(handler: () => void): () => void {
    this.#onPause.add(handler);
    return () => this.#onPause.delete(handler);
  }

  /** Reports whether the last input was a click or Enter, in the last {@link REQUEST_MS}, with no typing after it. */
  requested(): boolean {
    return this.#now() - this.#lastRequest < REQUEST_MS;
  }

  /** Records a start from the launcher: a window with one of `names` as its app id is asked for. */
  expectLaunch(names: readonly string[]): void {
    this.#launches.push({ names, at: this.#now() });
  }

  /** Reports whether a start from the launcher asked for a window of the app `appId`, and forgets that start. */
  launched(appId: string | null | undefined): boolean {
    const now = this.#now();
    const name = appId?.toLowerCase();
    const index = this.#launches.findIndex((launch) => now - launch.at < LAUNCH_MS && name !== undefined && launch.names.includes(name));
    for (let i = this.#launches.length - 1; i >= 0; i--) {
      if (i === index || now - this.#launches[i]!.at >= LAUNCH_MS) this.#launches.splice(i, 1);
    }
    return index >= 0;
  }

  #input(): void {
    this.#lastInput = this.#now();
    if (this.#timer === undefined) this.#schedule(PAUSE_MS);
  }

  /** One timer for each pause: it checks the last input when it fires, thus a move does not set a new timer. */
  #schedule(ms: number): void {
    this.#timer = setTimeout(() => {
      const rest = PAUSE_MS - (this.#now() - this.#lastInput);
      if (rest > 0) {
        this.#schedule(rest);
        return;
      }
      this.#timer = undefined;
      for (const handler of [...this.#onPause]) handler();
    }, ms);
  }
}
