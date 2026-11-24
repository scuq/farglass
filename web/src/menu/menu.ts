/**
 * The context menus of the shell: the items, and the providers of the elements.
 *
 * An element gets a provider with `setContextMenu`. A right click, Shift+F10,
 * or the ContextMenu key asks the provider of the nearest element. A provider
 * that gives no items blocks the menus of the elements around it. A text field
 * gets the text menu of the shell. The menu of the browser never shows.
 *
 * `openMenuAt` opens a menu with items that come later than the click, for
 * example the menu of a tray item, which the agent reads from the program.
 */

/** One entry of a menu. */
export type MenuItem =
  | {
      readonly label: string;
      readonly action: () => void;
      readonly disabled?: boolean;
      /** The keys of the same action, as a hint. */
      readonly shortcut?: string | undefined;
      /** The state of an entry that is on or off. An entry with no state has none. */
      readonly checked?: boolean | undefined;
      /** The entry is one of a group, and only one of the group is on. */
      readonly radio?: boolean | undefined;
      /** The URL of an icon. */
      readonly icon?: string | undefined;
      /** The entries of a submenu. The entry then opens the submenu, and `action` does not run. */
      readonly submenu?: readonly MenuItem[] | undefined;
    }
  | { readonly separator: true };

/** Returns the submenu of `item`, or undefined for an entry with no submenu and for a separator. */
export function submenuOf(item: MenuItem | undefined): readonly MenuItem[] | undefined {
  return item && !("separator" in item) && item.submenu && item.submenu.length > 0 ? item.submenu : undefined;
}

/** Returns the items of the submenu at `path`: the index of the open entry at each depth. */
export function itemsAt(items: readonly MenuItem[], path: readonly number[]): readonly MenuItem[] {
  let level = items;
  for (const index of path) {
    level = submenuOf(level[index]) ?? [];
  }
  return level;
}

/** Opens a menu with `items` at a point. `source` is the element that the menu belongs to. */
export type MenuOpener = (items: readonly MenuItem[], x: number, y: number, source: Element | null) => void;

let opener: MenuOpener | undefined;

/** Sets the function that shows a menu. The context menu host of the shell sets it. Returns a function that removes it. */
export function setMenuOpener(next: MenuOpener): () => void {
  opener = next;
  return () => {
    if (opener === next) {
      opener = undefined;
    }
  };
}

/**
 * Opens a menu with `items` at the point (`x`, `y`). Returns `false` if the shell shows no menus, or with no items.
 * `source` is the element that the menu belongs to: the focus goes back to it when the menu closes.
 */
export function openMenuAt(items: readonly MenuItem[], x: number, y: number, source: Element | null = null): boolean {
  if (!opener || items.length === 0) {
    return false;
  }
  opener(items, x, y, source);
  return true;
}

/** Gives the items of the menu of an element at the time of the click. */
export type MenuProvider = () => readonly MenuItem[];

const providers = new WeakMap<Element, MenuProvider>();

/** Sets the provider of the context menu of `element`. Returns a function that removes it. */
export function setContextMenu(element: Element, provider: MenuProvider): () => void {
  providers.set(element, provider);
  return () => {
    if (providers.get(element) === provider) {
      providers.delete(element);
    }
  };
}

/** Returns the provider of the nearest element from `target` up, or `null`. */
export function providerFor(target: Element | null): MenuProvider | null {
  for (let element = target; element; element = element.parentElement) {
    const provider = providers.get(element);
    if (provider) {
      return provider;
    }
  }
  return null;
}

/** A text field that gets the text menu of the shell. */
export type TextField = HTMLInputElement | HTMLTextAreaElement;

/** The types of input elements that hold text. */
const TEXT_TYPES = new Set(["text", "search", "password", "email", "url", "tel", "number", ""]);

/** Returns the text field at `target`, or `null`. The input element of xterm.js is not a text field: the terminal has its own menu. */
export function textField(target: Element | null): TextField | null {
  const field = target?.closest("input, textarea");
  if (!field || field.classList.contains("xterm-helper-textarea")) {
    return null;
  }
  if (field instanceof HTMLInputElement && !TEXT_TYPES.has(field.type)) {
    return null;
  }
  return field as TextField;
}

/** Replaces the selection of `field` with `text`, as typing does, thus the app gets an input event. */
function insert(field: TextField, text: string): void {
  const start = field.selectionStart ?? field.value.length;
  const end = field.selectionEnd ?? start;
  field.setRangeText(text, start, end, "end");
  field.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: text ? "insertFromPaste" : "deleteByCut", data: text }));
}

/**
 * Returns the text menu of a field, as a native app has it: cut, copy, paste, and select all.
 * A password field gives no cut and no copy. A read-only field gives no cut and no paste.
 */
export function textFieldMenu(field: TextField): MenuItem[] {
  const start = field.selectionStart ?? 0;
  const end = field.selectionEnd ?? 0;
  const selected = field.value.slice(start, end);
  const secret = field instanceof HTMLInputElement && field.type === "password";
  const locked = field.readOnly || field.disabled;
  const clipboard = navigator.clipboard;
  return [
    {
      label: "Cut",
      disabled: !selected || secret || locked || !clipboard,
      action: () => {
        void clipboard.writeText(selected);
        field.focus();
        insert(field, "");
      },
    },
    {
      label: "Copy",
      disabled: !selected || secret || !clipboard,
      action: () => void clipboard.writeText(selected),
    },
    {
      label: "Paste",
      disabled: locked || !clipboard,
      action: () =>
        void clipboard.readText().then(
          (text) => {
            field.focus();
            insert(field, text);
          },
          () => {},
        ),
    },
    { separator: true },
    {
      label: "Select all",
      disabled: !field.value,
      action: () => {
        field.focus();
        field.select();
      },
    },
  ];
}

/** Returns the index of the next item that is not a separator and not disabled, or -1. */
export function nextEnabled(items: readonly MenuItem[], from: number, step: 1 | -1): number {
  const count = items.length;
  for (let i = 1; i <= count; i++) {
    const index = (((from + step * i) % count) + count) % count;
    const item = items[index]!;
    if (!("separator" in item) && !item.disabled) {
      return index;
    }
  }
  return -1;
}

/**
 * Returns the position of a menu of `width` x `height` at the point (`x`, `y`),
 * moved so that it stays in the viewport.
 */
export function placeMenu(
  x: number,
  y: number,
  width: number,
  height: number,
  viewport: { width: number; height: number },
  margin = 4,
): { x: number; y: number } {
  const left = x + width + margin > viewport.width ? Math.max(margin, x - width) : x;
  const top = y + height + margin > viewport.height ? Math.max(margin, viewport.height - height - margin) : y;
  return { x: Math.round(left), y: Math.round(top) };
}
