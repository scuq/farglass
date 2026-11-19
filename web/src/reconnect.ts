/** The waits between two attempts to connect again. */

/** The waits in milliseconds: 1 s, 2 s, 4 s, 8 s, and then 15 s for each more attempt. */
const WAITS_MS = [1000, 2000, 4000, 8000, 15_000];

/** Returns the wait before the attempt with the number `attempt`, which starts at 0. */
export function reconnectDelay(attempt: number): number {
  return WAITS_MS[Math.min(attempt, WAITS_MS.length - 1)]!;
}
