/**
 * The use of the host by the apps of each window and by farglass (F045, F046).
 * The agent sends a sample every 2 s while the stats channel is open.
 */

import { createSignal } from "solid-js";
import type { Connection, StatsEvent, Usage } from "./proto";

/** The CPU in percent of one core from which an app counts as busy. */
export const BUSY_CPU = 5;
/** The network in bytes each second from which an app counts as busy. */
export const BUSY_NET = 20_000;

/** Reports whether an app is busy. */
export function isBusy(usage: Usage): boolean {
  return usage.cpu >= BUSY_CPU || (usage.net ?? 0) >= BUSY_NET;
}

/** Formats bytes: "340 MB". */
export function formatBytes(bytes: number): string {
  const units = ["B", "kB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit++;
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`;
}

/** Returns the details of a use, for a tooltip: "CPU 12 %, memory 340 MB, network 1.2 MB/s". */
export function describeUsage(usage: Usage): string {
  const parts = [`CPU ${Math.round(usage.cpu)} %`, `memory ${formatBytes(usage.memory)}`];
  if (usage.net !== undefined) parts.push(`network ${formatBytes(usage.net)}/s`);
  return parts.join(", ");
}

type Sample = Extract<StatsEvent, { type: "sample" }>;

export class StatsClient {
  readonly #sample = createSignal<Sample | null>(null);
  readonly #channel;

  constructor(connection: Connection) {
    this.#channel = connection.openChannel({ kind: "stats" }, { oncontrol: (message) => this.#sample[1](message as Sample) });
  }

  close(): void {
    this.#channel.close();
  }

  /** The last sample, or null before the first one. */
  sample(): Sample | null {
    return this.#sample[0]();
  }

  /** The use of the apps of the window `windowId`, or undefined if the agent does not measure it. */
  usageOf(windowId: string): Usage | undefined {
    return this.#sample[0]()?.windows.find((window) => window.window === windowId)?.usage;
  }
}

// A plain variable: the desktop sets it before its panel renders.
let active: StatsClient | undefined;

/** Returns the stats of the desktop that shows. */
export function statsClient(): StatsClient | undefined {
  return active;
}

/** Starts the stats for `connection`. Returns the function that stops them. */
export function startStats(connection: Connection): () => void {
  const client = new StatsClient(connection);
  active = client;
  return () => {
    client.close();
    if (active === client) active = undefined;
  };
}
