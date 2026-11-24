import { createSignal, For, onCleanup, Show } from "solid-js";
import { itemsAt, type MenuItem, nextEnabled, placeMenu, providerFor, setMenuOpener, submenuOf, textField, textFieldMenu } from "./menu";
import "./menu.css";

/** An open menu: its items and the point where it opens. */
interface OpenMenu {
  readonly items: readonly MenuItem[];
  readonly x: number;
  readonly y: number;
}

/** Reports whether the browser has the Popover API. */
const SUPPORTED = typeof HTMLElement !== "undefined" && "showPopover" in HTMLElement.prototype;

/** Reports whether an item of the level has a state or an icon: the level then has a column for them. */
const hasLead = (items: readonly MenuItem[]) => items.some((item) => !("separator" in item) && (item.checked !== undefined || item.icon !== undefined));

/**
 * Shows the context menus of the shell. Mount it one time, at the root of the shell.
 *
 * A right click opens the menu of the element under the pointer. Shift+F10 and
 * the ContextMenu key open the menu of the focused element. A text field gets
 * the text menu of the shell. As in a native desktop, the menu of the browser never shows.
 *
 * An entry can have a submenu. The pointer on the entry, the Right arrow, or
 * Enter opens it, and the Left arrow closes it.
 */
export function ContextMenuHost() {
  const [menu, setMenu] = createSignal<OpenMenu | null>(null);
  // The open submenus: the index of the open entry at each depth.
  const [path, setPath] = createSignal<readonly number[]>([]);
  // The selected entry in the deepest open menu.
  const [active, setActive] = createSignal(-1);
  let element: HTMLElement | undefined;
  let returnFocus: HTMLElement | null = null;

  const close = (restore = true) => {
    if (!menu()) {
      return;
    }
    setMenu(null);
    setPath([]);
    setActive(-1);
    if (SUPPORTED && element?.matches(":popover-open")) {
      element.hidePopover();
    }
    if (restore) {
      returnFocus?.focus({ preventScroll: true });
    }
    returnFocus = null;
  };

  const show = (items: readonly MenuItem[], target: Element | null, x: number, y: number) => {
    returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setMenu({ items, x, y });
    setPath([]);
    setActive(-1);
    queueMicrotask(() => {
      if (!element) {
        return;
      }
      if (SUPPORTED && !element.matches(":popover-open")) {
        // With the clicked element as its source, the menu is a child of a popover
        // around that element, for example the launcher: a click in the menu does not close it.
        try {
          element.showPopover({ source: target } as ShowPopoverOptions);
        } catch {
          element.showPopover();
        }
      }
      // Move the menu into the viewport after the browser knows its size.
      const box = element.getBoundingClientRect();
      const place = placeMenu(x, y, box.width, box.height, { width: window.innerWidth, height: window.innerHeight });
      element.style.left = `${place.x}px`;
      element.style.top = `${place.y}px`;
      element.focus({ preventScroll: true });
    });
  };

  const open = (target: Element | null, x: number, y: number): boolean => {
    const field = textField(target);
    const provider = field ? () => textFieldMenu(field) : providerFor(target);
    if (!provider) {
      return false;
    }
    const items = provider();
    if (items.length === 0) {
      return true;
    }
    show(items, target, x, y);
    return true;
  };

  const onContextMenu = (event: MouseEvent) => {
    const target = event.target instanceof Element ? event.target : null;
    event.preventDefault();
    open(target, event.clientX, event.clientY);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) {
      return;
    }
    const target = document.activeElement;
    const anchor = (target?.closest(".term-app") ?? target)?.getBoundingClientRect();
    if (open(target, (anchor?.left ?? 0) + 16, (anchor?.top ?? 0) + 16)) {
      // The key opens the menu and goes nowhere else: a terminal must not get Shift+F10.
      event.preventDefault();
      event.stopImmediatePropagation();
      // The keyboard opens the menu with the first item selected.
      queueMicrotask(() => setActive(nextEnabled(menu()?.items ?? [], -1, 1)));
    }
  };

  /** The items of the menu at `depth`: 0 is the menu, 1 is its open submenu. */
  const level = (depth: number) => itemsAt(menu()?.items ?? [], path().slice(0, depth));

  /** Opens the submenu of the entry `index` at `depth`. The keyboard selects its first entry. */
  const openSubmenu = (depth: number, index: number, select: boolean) => {
    const next = [...path().slice(0, depth), index];
    setPath(next);
    setActive(select ? nextEnabled(itemsAt(menu()?.items ?? [], next), -1, 1) : -1);
  };

  const run = (item: MenuItem, depth: number, index: number, select: boolean) => {
    if ("separator" in item || item.disabled) {
      return;
    }
    if (submenuOf(item)) {
      openSubmenu(depth, index, select);
      return;
    }
    close();
    item.action();
  };

  const onMenuKey = (event: KeyboardEvent) => {
    const depth = path().length;
    const items = level(depth);
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
      case "ArrowRight": {
        const item = items[active()];
        if (item && submenuOf(item) && !("separator" in item) && !item.disabled) {
          openSubmenu(depth, active(), true);
        }
        break;
      }
      case "ArrowLeft":
        if (depth > 0) {
          // The submenu closes, and its entry is selected again.
          setActive(path()[depth - 1]!);
          setPath(path().slice(0, depth - 1));
        }
        break;
      case "Enter":
      case " ": {
        const item = items[active()];
        if (item) {
          run(item, depth, active(), true);
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

  /** Moves a submenu to the left of its entry, or up, if it does not fit in the viewport. */
  const placeSubmenu = (submenu: HTMLElement) => {
    queueMicrotask(() => {
      const box = submenu.getBoundingClientRect();
      if (box.right > window.innerWidth - 4) {
        submenu.dataset.side = "left";
      }
      const below = box.bottom - (window.innerHeight - 4);
      if (below > 0) {
        submenu.style.marginTop = `${-Math.round(below)}px`;
      }
    });
  };

  /** The entries of the menu at `depth`. A submenu is in the element of its entry. */
  const Level = (props: { depth: number }) => {
    const depth = props.depth;
    const selected = (index: number) => (depth < path().length ? path()[depth] === index : active() === index);
    return (
      <For each={level(depth)}>
        {(item, index) => {
          if ("separator" in item) {
            return <div class="context-menu-separator" role="separator" />;
          }
          const submenu = submenuOf(item);
          const isOpen = () => submenu !== undefined && path()[depth] === index();
          return (
            <div
              class="context-menu-item"
              role={item.checked === undefined ? "menuitem" : item.radio ? "menuitemradio" : "menuitemcheckbox"}
              aria-checked={item.checked === undefined ? undefined : (String(item.checked) as "true" | "false")}
              aria-haspopup={submenu ? "menu" : undefined}
              aria-expanded={submenu ? (String(isOpen()) as "true" | "false") : undefined}
              aria-disabled={item.disabled ? "true" : undefined}
              data-active={String(selected(index()))}
              data-lead={hasLead(level(depth)) ? "true" : undefined}
              onPointerMove={(event) => {
                // The entry is in the element of its parent entry: the parent must not get the event.
                event.stopPropagation();
                if (item.disabled) {
                  return;
                }
                if (submenu) {
                  if (!isOpen()) {
                    openSubmenu(depth, index(), false);
                  }
                } else if (path().length > depth || active() !== index()) {
                  setPath(path().slice(0, depth));
                  setActive(index());
                }
              }}
              onClick={(event) => {
                event.stopPropagation();
                run(item, depth, index(), false);
              }}
            >
              <span class="context-menu-label">{item.label}</span>
              {item.shortcut ? <kbd>{item.shortcut}</kbd> : null}
              {/* The mark is after the label in the document and before it on the screen. */}
              {hasLead(level(depth)) ? (
                <span class="context-menu-lead" aria-hidden="true">
                  {item.checked ? (item.radio ? "●" : "✓") : item.checked === undefined && item.icon ? <img src={item.icon} alt="" /> : null}
                </span>
              ) : null}
              {submenu ? (
                <span class="context-menu-arrow" aria-hidden="true">
                  ›
                </span>
              ) : null}
              <Show when={isOpen()}>
                <div class="context-menu context-menu-sub" role="menu" aria-label={item.label} ref={placeSubmenu}>
                  <Level depth={depth + 1} />
                </div>
              </Show>
            </div>
          );
        }}
      </For>
    );
  };

  document.addEventListener("contextmenu", onContextMenu, true);
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("blur", () => close(false));
  window.addEventListener("resize", () => close(false));
  // A menu with items that come later than the click, for example of a tray item.
  const removeOpener = setMenuOpener((items, x, y, source) => show(items, source, x, y));
  onCleanup(() => {
    document.removeEventListener("contextmenu", onContextMenu, true);
    window.removeEventListener("keydown", onKeyDown, true);
    removeOpener();
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
      <Level depth={0} />
    </div>
  );
}
