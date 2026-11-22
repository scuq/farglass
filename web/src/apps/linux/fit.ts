/**
 * The frame of a Linux app window follows the size of the app (F085).
 *
 * After a quiet time with no new frame, the shell compares the size of the app with the size of
 * the frame. An app can answer a size with a different one, or change its size itself, for
 * example an X11 app that snaps to its text cells or keeps its smallest size.
 */

/** A size in logical pixels. */
export interface Size {
  readonly width: number;
  readonly height: number;
}

/** What the frame does: take the size of the app, keep its size, or wait for the answer of the app. */
export type Fit = "fit" | "keep" | "wait";

/** Two sizes are the same if they differ by 1 pixel or less: the shell rounds the size of the frame. */
export function sameSize(a: Size, b: Size): boolean {
  return Math.abs(a.width - b.width) <= 1 && Math.abs(a.height - b.height) <= 1;
}

/**
 * Returns what the frame does with the size `app` of the app and the size `frame` of its content.
 * `requested` is the last size that the shell asked for and the app did not answer yet,
 * `waited` the time in milliseconds since that request, and `answerMs` the longest wait.
 */
export function fit(app: Size, frame: Size, requested: Size | null, waited: number, answerMs: number): Fit {
  const open = requested !== null && !sameSize(app, requested);
  if (open && waited < answerMs) return "wait";
  return sameSize(app, frame) ? "keep" : "fit";
}
