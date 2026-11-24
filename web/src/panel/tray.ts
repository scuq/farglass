/**
 * The system tray (F102): the status notifier items of the programs of the host.
 *
 * A program that stays in a tray, for example a password manager or a sync
 * client, registers an item in the agent. The agent sends the items on the
 * tray channel, and a click on an item goes back to the program.
 */

import { createSignal } from "solid-js";
import type { Channel, Connection, TrayEvent, TrayItem, TrayRequest } from "../proto";

/** Returns the items after `event`. */
export function applyEvent(items: readonly TrayItem[], event: TrayEvent): readonly TrayItem[] {
  switch (event.type) {
    case "items":
      return event.items;
    case "item":
      return items.some((each) => each.id === event.item.id) ? items.map((each) => (each.id === event.item.id ? event.item : each)) : [...items, event.item];
    case "removed":
      return items.filter((each) => each.id !== event.id);
    default:
      // A message of a newer agent.
      return items;
  }
}

/** Returns the items in the order of the panel: by title, then by id. The order does not change when an icon changes. */
export function sortedItems(items: readonly TrayItem[]): readonly TrayItem[] {
  return items.toSorted((a, b) => a.title.localeCompare(b.title, undefined, { numeric: true }) || a.id - b.id);
}

/** Returns the icon of `item` as a data URL, or undefined with no icon. */
export function iconUrl(item: TrayItem): string | undefined {
  // Only the image types of the agent: a different type does not become a URL.
  if (!item.icon || (item.icon_mime !== "image/png" && item.icon_mime !== "image/svg+xml")) return undefined;
  return `data:${item.icon_mime};base64,${item.icon}`;
}

/** Returns the text of the tooltip of `item`. */
export function itemTip(item: TrayItem): string {
  return item.tooltip ? `${item.title}: ${item.tooltip}` : item.title;
}

/**
 * Returns the distance for a Scroll request from the `deltaY` or `deltaX` of a wheel event.
 * One step of a wheel is 120, and up is positive.
 */
export function scrollDelta(delta: number): number {
  return Math.round(-delta * 1.2) || 0;
}

export class Tray {
  readonly #channel: Channel;
  // The list is a plain field: a signal gives its old value until the next update, and two events can come in a row.
  #list: readonly TrayItem[] = [];
  readonly #items = createSignal<readonly TrayItem[]>([]);

  constructor(connection: Connection) {
    this.#channel = connection.openChannel(
      { kind: "tray" },
      {
        oncontrol: (message) => this.#set(applyEvent(this.#list, message as TrayEvent)),
        // An agent of an older version does not know the channel: the panel shows no tray.
        onclose: () => this.#set([]),
      },
    );
  }

  #set(items: readonly TrayItem[]): void {
    this.#list = items;
    this.#items[1](items);
  }

  close(): void {
    this.#channel.close();
  }

  /** The items in the order of the panel. */
  items(): readonly TrayItem[] {
    return sortedItems(this.#items[0]());
  }

  /** The main action of the item: most programs show their window. */
  activate(id: number, x: number, y: number): void {
    this.#send({ type: "activate", id, x: Math.round(x), y: Math.round(y) });
  }

  /** The second action of the item. */
  secondaryActivate(id: number, x: number, y: number): void {
    this.#send({ type: "secondary-activate", id, x: Math.round(x), y: Math.round(y) });
  }

  /** The wheel moved on the item. */
  scroll(id: number, delta: number, horizontal: boolean): void {
    if (delta !== 0) this.#send({ type: "scroll", id, delta, horizontal });
  }

  #send(request: TrayRequest): void {
    this.#channel.control(request);
  }
}

// A plain variable: the desktop sets it before its panel renders.
let active: Tray | undefined;

/** Returns the tray of the desktop that shows, or undefined with no desktop. */
export function tray(): Tray | undefined {
  return active;
}

/** Starts the tray for `connection`. Returns the function that stops it. */
export function startTray(connection: Connection): () => void {
  const each = new Tray(connection);
  active = each;
  return () => {
    each.close();
    if (active === each) active = undefined;
  };
}
