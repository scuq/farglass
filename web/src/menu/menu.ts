/**
 * The context menus of the shell: the items, and the providers of the elements.
 *
 * An element gets a provider with `setContextMenu`. A right click, Shift+F10,
 * or the ContextMenu key asks the provider of the nearest element. A provider
 * that gives no items blocks the menus of the elements around it.
 */

/** One entry of a menu. */
export type MenuItem =
  | {
      readonly label: string;
      readonly action: () => void;
      readonly disabled?: boolean;
      /** The keys of the same action, as a hint. */
      readonly shortcut?: string;
    }
  | { readonly separator: true };

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

/** Reports whether the browser menu stays for `target`: text fields, for spelling and the native paste. */
export function keepsNativeMenu(target: Element | null): boolean {
  const field = target?.closest("input, textarea, [contenteditable=''], [contenteditable='true']");
  // The input element of xterm.js is a textarea, but the terminal has its own menu.
  return !!field && !field.classList.contains("xterm-helper-textarea");
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
