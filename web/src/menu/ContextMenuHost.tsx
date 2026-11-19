import { createSignal, For, onCleanup } from "solid-js";
import { keepsNativeMenu, type MenuItem, nextEnabled, placeMenu, providerFor } from "./menu";
import "./menu.css";

/** An open menu: its items and the point where it opens. */
interface OpenMenu {
  readonly items: readonly MenuItem[];
  readonly x: number;
  readonly y: number;
}

/** Reports whether the browser has the Popover API. */
const SUPPORTED = typeof HTMLElement !== "undefined" && "showPopover" in HTMLElement.prototype;

/**
 * Shows the context menus of the shell. Mount it one time, on the desktop.
 *
 * A right click opens the menu of the element under the pointer. Shift+F10 and
 * the ContextMenu key open the menu of the focused element. Text fields and
 * Shift+right click keep the menu of the browser.
 */
export function ContextMenuHost() {
  const [menu, setMenu] = createSignal<OpenMenu | null>(null);
  const [active, setActive] = createSignal(-1);
  let element: HTMLElement | undefined;
  let returnFocus: HTMLElement | null = null;

  const close = (restore = true) => {
    if (!menu()) {
      return;
    }
    setMenu(null);
    setActive(-1);
    if (SUPPORTED && element?.matches(":popover-open")) {
      element.hidePopover();
    }
    if (restore) {
      returnFocus?.focus({ preventScroll: true });
    }
    returnFocus = null;
  };

  const open = (target: Element | null, x: number, y: number): boolean => {
    const provider = providerFor(target);
    if (!provider) {
      return false;
    }
    const items = provider();
    if (items.length === 0) {
      return true;
    }
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setMenu({ items, x, y });
    setActive(-1);
    queueMicrotask(() => {
      if (!element) {
        return;
      }
      if (SUPPORTED && !element.matches(":popover-open")) {
        element.showPopover();
      }
      // Move the menu into the viewport after the browser knows its size.
      const box = element.getBoundingClientRect();
      const place = placeMenu(x, y, box.width, box.height, { width: window.innerWidth, height: window.innerHeight });
      element.style.left = `${place.x}px`;
      element.style.top = `${place.y}px`;
      element.focus({ preventScroll: true });
    });
    return true;
  };

  const onContextMenu = (event: MouseEvent) => {
    const target = event.target instanceof Element ? event.target : null;
    if (event.shiftKey || keepsNativeMenu(target)) {
      return;
    }
    event.preventDefault();
    open(target, event.clientX, event.clientY);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) {
      return;
    }
    const target = document.activeElement;
    if (keepsNativeMenu(target)) {
      return;
    }
    const anchor = (target?.closest(".term-app") ?? target)?.getBoundingClientRect();
    if (open(target, (anchor?.left ?? 0) + 16, (anchor?.top ?? 0) + 16)) {
      // The key opens the menu and goes nowhere else: a terminal must not get Shift+F10.
      event.preventDefault();
      event.stopImmediatePropagation();
      // The keyboard opens the menu with the first item selected.
      queueMicrotask(() => setActive(nextEnabled(menu()?.items ?? [], -1, 1)));
    }
  };

  const run = (item: MenuItem) => {
    if ("separator" in item || item.disabled) {
      return;
    }
    close();
    item.action();
  };

  const onMenuKey = (event: KeyboardEvent) => {
    const items = menu()?.items ?? [];
    switch (event.key) {
      case "ArrowDown":
        setActive(nextEnabled(items, active(), 1));
        break;
      case "ArrowUp":
        setActive(nextEnabled(items, active() < 0 ? items.length : active(), -1));
        break;
      case "Home":
        setActive(nextEnabled(items, -1, 1));
        break;
      case "End":
        setActive(nextEnabled(items, items.length, -1));
        break;
      case "Enter":
      case " ": {
        const item = items[active()];
        if (item) {
          run(item);
        }
        break;
      }
      case "Escape":
      case "Tab":
        close();
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  };

  document.addEventListener("contextmenu", onContextMenu, true);
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("blur", () => close(false));
  window.addEventListener("resize", () => close(false));
  onCleanup(() => {
    document.removeEventListener("contextmenu", onContextMenu, true);
    window.removeEventListener("keydown", onKeyDown, true);
  });

  return (
    <div
      class="context-menu"
      popover="auto"
      role="menu"
      tabindex={-1}
      aria-label="Context menu"
      hidden={!SUPPORTED && !menu()}
      ref={(el) => {
        element = el;
        // The browser closes the popover on a click outside and on Escape.
        el.addEventListener("toggle", (event) => {
          if ((event as ToggleEvent).newState === "closed") {
            close(false);
          }
        });
      }}
      onKeyDown={onMenuKey}
    >
      <For each={menu()?.items ?? []}>
        {(item, index) =>
          "separator" in item ? (
            <div class="context-menu-separator" role="separator" />
          ) : (
            <div
              class="context-menu-item"
              role="menuitem"
              aria-disabled={item.disabled ? "true" : undefined}
              data-active={String(index() === active())}
              onPointerMove={() => !item.disabled && setActive(index())}
              onClick={() => run(item)}
            >
              <span>{item.label}</span>
              {item.shortcut ? <kbd>{item.shortcut}</kbd> : null}
            </div>
          )
        }
      </For>
    </div>
  );
}
