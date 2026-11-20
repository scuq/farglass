/**
 * The gate of OSC 52 clipboard writes (F055).
 *
 * A program in the terminal, for example vim, tmux, or a program on a remote
 * host over ssh, can ask to write the clipboard of the client computer with
 * OSC 52. The write goes through only shortly after a key or a click of the
 * user in that terminal. Otherwise the terminal asks the user first.
 */

/** The time after a key or a click in which a program can write the clipboard. */
export const COPY_WINDOW_MS = 3000;

/** Remembers the last key or click of the user in one terminal. */
export class CopyGate {
  #last = Number.NEGATIVE_INFINITY;

  /** Records a key or a click of the user at `now` (milliseconds). */
  input(now: number = performance.now()): void {
    this.#last = now;
  }

  /** Reports whether a program can write the clipboard at `now` with no question. */
  allows(now: number = performance.now()): boolean {
    return now - this.#last <= COPY_WINDOW_MS;
  }
}
