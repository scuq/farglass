/**
 * Reports keyboard and pointer activity to farglassd for the idle lock.
 * farglassd locks the session after a time with no activity in each tab of the user.
 */

/** The shortest time between two activity messages. farglassd expects one each 30 s at most. */
export const ACTIVITY_INTERVAL_MS = 30_000;

const EVENTS = ["keydown", "pointerdown", "pointermove", "wheel", "touchstart"] as const;

/**
 * Calls `report` on the first input, then at most each {@link ACTIVITY_INTERVAL_MS}.
 * Returns the function that stops the listeners.
 */
export function trackActivity(report: () => void, target: EventTarget = window, now: () => number = () => performance.now()): () => void {
  let last = -Infinity;
  const onInput = () => {
    const time = now();
    if (time - last >= ACTIVITY_INTERVAL_MS) {
      last = time;
      report();
    }
  };
  for (const name of EVENTS) {
    target.addEventListener(name, onInput, { capture: true, passive: true });
  }
  return () => {
    for (const name of EVENTS) {
      target.removeEventListener(name, onInput, { capture: true });
    }
  };
}
