/**
 * The system tray (F102): the status notifier items of the programs of the host.
 *
 * A program that stays in a tray, for example a password manager or a sync
 * client, registers an item in the agent. The agent sends the items on the
 * tray channel, and a click on an item goes back to the program.
 *
 * The menu of an item (F103) is in the program. The shell asks the agent for
 * it at each right click and shows it as a context menu.
 */

import { createSignal } from "solid-js";
import { type MenuItem, openMenuAt } from "../menu/menu";
import type { Channel, Connection, TrayEvent, TrayItem, TrayMenuItem, TrayRequest } from "../proto";

/** The time in milliseconds that the shell waits for the menu of an item. A later menu does not open. */
export const MENU_WAIT_MS = 3000;

/** Returns the items after `event`. */
export function applyEvent(items: readonly TrayItem[], event: TrayEvent): readonly TrayItem[] {
  switch (event.type) {
    case "items":
      return event.items;
    case "item":
      return items.some((each) => each.id === event.item.id) ? items.map((each) => (each.id === event.item.id ? event.item : each)) : [...items, event.item];
    case "removed":
      return items.filter((each) => each.id !== event.id);
    case "menu":
      return items;
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

/**
 * Returns the menu of a tray item as items of a shell menu. `click` gets the id of the selected entry.
 * A separator at the start, at the end, or after a separator is not in the result.
 */
export function toMenuItems(tree: readonly TrayMenuItem[], click: (entry: number) => void): MenuItem[] {
  const items: MenuItem[] = [];
  for (const entry of tree) {
    if (entry.separator) {
      const last = items.at(-1);
      if (last && !("separator" in last)) items.push({ separator: true });
      continue;
    }
    const submenu = toMenuItems(entry.children, click);
    items.push({
      label: entry.label,
      action: () => click(entry.id),
      disabled: !entry.enabled,
      checked: entry.toggle ? entry.checked : undefined,
      radio: entry.toggle === "radio" ? true : undefined,
      icon: entry.icon ? `data:image/png;base64,${entry.icon}` : undefined,
      submenu: submenu.length > 0 ? submenu : undefined,
    });
  }
  const last = items.at(-1);
  if (last && "separator" in last) items.pop();
  return items;
}

/** A menu that the shell waits for: the item, where the menu opens, and when the shell asked. */
interface Asked {
  readonly id: number;
  readonly x: number;
  readonly y: number;
  readonly source: Element | null;
  readonly at: number;
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
        oncontrol: (message) => this.#event(message as TrayEvent),
        // An agent of an older version does not know the channel: the panel shows no tray.
        onclose: () => this.#set([]),
      },
    );
  }

  // The menu that this tab asked for. Each tab gets each menu, and a tab that did not ask ignores it.
  #asked: Asked | undefined;

  #event(event: TrayEvent): void {
    if (event.type !== "menu") {
      this.#set(applyEvent(this.#list, event));
      return;
    }
    const asked = this.#asked;
    this.#asked = undefined;
    if (!asked || asked.id !== event.id || Date.now() - asked.at > MENU_WAIT_MS) return;
    const items = toMenuItems(event.items, (entry) => this.#send({ type: "menu-click", id: event.id, item: entry }));
    openMenuAt(items, asked.x, asked.y, asked.source);
  }

  /** Remembers that the menu of `item` opens below `button` when the agent sends it. */
  #expect(item: TrayItem, button: Element): void {
    const box = button.getBoundingClientRect();
    this.#asked = { id: item.id, x: box.left, y: box.bottom + 2, source: button, at: Date.now() };
  }

  /**
   * A click on `button`, the button of `item`, with the pointer at (`x`, `y`).
   * An item that is only a menu opens its menu. Any other item does its main action.
   * The agent answers with the menu if the program has no main action.
   */
  click(item: TrayItem, button: Element, x: number, y: number): void {
    if (item.menu_only) {
      this.openMenu(item, button);
      return;
    }
    if (item.menu) this.#expect(item, button);
    this.activate(item.id, x, y);
  }

  /** Opens the menu of `item` below `button`. An item with no menu does its second action. */
  openMenu(item: TrayItem, button: Element): void {
    if (!item.menu) {
      const box = button.getBoundingClientRect();
      this.secondaryActivate(item.id, box.left, box.bottom);
      return;
    }
    this.#expect(item, button);
    this.#send({ type: "menu", id: item.id });
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
