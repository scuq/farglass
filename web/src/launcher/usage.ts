/**
 * The pinned apps of the launcher and the use of each app (F076).
 * The value is in the state store under {@link LAUNCHER_KEY}, thus each tab and each login of the user gets it.
 */

/** The key in the state store. */
export const LAUNCHER_KEY = "launcher";
/** The number of apps in the list of the most used apps. */
export const FREQUENT_LIMIT = 5;
/** The launcher keeps the use of this number of apps at most. */
const USED_LIMIT = 50;
/** After this time with no start, the score of an app is half. */
const HALF_LIFE_MS = 14 * 24 * 60 * 60 * 1000;

/** The use of one app: a score at the time `last`, in milliseconds since the epoch. */
export interface Use {
  readonly score: number;
  readonly last: number;
}

export interface LauncherState {
  /** The ids of the pinned entries, in the order of the pins. */
  readonly pinned: readonly string[];
  readonly used: Readonly<Record<string, Use>>;
}

export const EMPTY_LAUNCHER: LauncherState = { pinned: [], used: {} };

/** Returns a valid state from a value of the state store. Fields that are not valid are dropped. */
export function parseLauncherState(value: unknown): LauncherState {
  const record = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  const pinned = Array.isArray(record.pinned) ? [...new Set(record.pinned.filter((id): id is string => typeof id === "string"))] : [];
  const used: Record<string, Use> = {};
  if (typeof record.used === "object" && record.used !== null) {
    for (const [id, use] of Object.entries(record.used as Record<string, unknown>)) {
      const { score, last } = (use ?? {}) as Partial<Use>;
      if (typeof score === "number" && typeof last === "number" && Number.isFinite(score) && Number.isFinite(last) && score > 0) {
        used[id] = { score, last };
      }
    }
  }
  return { pinned, used };
}

/** Returns the score of `use` at the time `now`: each start counts 1, and a start counts less as it gets older. */
function scoreAt(use: Use, now: number): number {
  return use.score * 0.5 ** (Math.max(0, now - use.last) / HALF_LIFE_MS);
}

/** Returns the state after a start of the entry `id` at the time `now`. */
export function recordUse(state: LauncherState, id: string, now: number): LauncherState {
  const before = state.used[id];
  const used = { ...state.used, [id]: { score: (before ? scoreAt(before, now) : 0) + 1, last: now } };
  const kept = Object.entries(used)
    .sort(([, a], [, b]) => scoreAt(b, now) - scoreAt(a, now))
    .slice(0, USED_LIMIT);
  return { pinned: state.pinned, used: Object.fromEntries(kept) };
}

/** Returns the state with the entry `id` pinned, or not pinned if it was. A new pin goes to the end. */
export function togglePin(state: LauncherState, id: string): LauncherState {
  const pinned = state.pinned.includes(id) ? state.pinned.filter((each) => each !== id) : [...state.pinned, id];
  return { pinned, used: state.used };
}

/**
 * Returns the ids of the most used entries at the time `now`, the highest score first.
 * A pinned entry is not in the list: it has its own place.
 */
export function frequent(state: LauncherState, now: number, limit: number = FREQUENT_LIMIT): string[] {
  return Object.entries(state.used)
    .filter(([id]) => !state.pinned.includes(id))
    .sort(([, a], [, b]) => scoreAt(b, now) - scoreAt(a, now) || b.last - a.last)
    .slice(0, limit)
    .map(([id]) => id);
}
