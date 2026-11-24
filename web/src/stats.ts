/**
 * The use of the host by the apps of each window and by farglass (F045, F046), and by
 * all users (F105). The agent sends a sample every 2 s while the stats channel is open.
 * The client keeps the samples of the last 5 minutes for the graphs of the resource page.
 */

import { createSignal } from "solid-js";
import type { Connection, HostUsage, StatsEvent, Usage } from "./proto";

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
  return `${value >= 100 || unit === 0 ? Math.round(value) : Number(value.toFixed(1))} ${units[unit]}`;
}

/** Returns the details of a use, for a tooltip: "CPU 12 %, memory 340 MB, network 1.2 MB/s". */
export function describeUsage(usage: Usage): string {
  const parts = [`CPU ${Math.round(usage.cpu)} %`, `memory ${formatBytes(usage.memory)}`];
  if (usage.net !== undefined) parts.push(`network ${formatBytes(usage.net)}/s`);
  return parts.join(", ");
}

type Sample = Extract<StatsEvent, { type: "sample" }>;

/** The seconds between two samples of the agent. */
export const SAMPLE_SECONDS = 2;
/** The samples that the client keeps: 5 minutes. */
export const HISTORY_LENGTH = 150;

/** The data rate of the connection of this tab, in bytes each second. */
export interface DataRate {
  readonly received: number;
  readonly sent: number;
}

/** One sample of the history: the use of farglass and of the host, and the data rate of this tab. */
export interface StatsPoint {
  readonly agent: Usage;
  readonly server?: Usage | undefined;
  readonly host?: HostUsage | undefined;
  readonly rate: DataRate;
}

export class StatsClient {
  readonly #sample = createSignal<Sample | null>(null);
  readonly #history = createSignal<readonly StatsPoint[]>([]);
  readonly #channel;
  readonly #connection;
  #last: { sent: number; received: number; at: number };

  constructor(connection: Connection) {
    this.#connection = connection;
    this.#last = { ...connection.bytes, at: performance.now() };
    this.#channel = connection.openChannel({ kind: "stats" }, { oncontrol: (message) => this.#add(message as Sample) });
  }

  /** Keeps a sample, with the data rate of the connection since the sample before. */
  #add(sample: Sample): void {
    const now = { ...this.#connection.bytes, at: performance.now() };
    const seconds = Math.max(0.001, (now.at - this.#last.at) / 1000);
    const rate = { received: (now.received - this.#last.received) / seconds, sent: (now.sent - this.#last.sent) / seconds };
    this.#last = now;
    this.#sample[1](sample);
    this.#history[1]((old) => [...old, { agent: sample.agent, server: sample.server, host: sample.host, rate }].slice(-HISTORY_LENGTH));
  }

  /** The samples of the last 5 minutes, the oldest first. */
  history(): readonly StatsPoint[] {
    return this.#history[0]();
  }

  /** The milliseconds since the last sample, or since the start with no sample. Not a signal. */
  age(): number {
    return performance.now() - this.#last.at;
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
