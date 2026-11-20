/**
 * The keys of the window manager. F044 adds keybinding schemes. Until then,
 * this table is the one place that names the keys.
 *
 * The client operating system keeps Alt+Tab and the Super key, unless the shell
 * runs in fullscreen with Keyboard Lock (F017). Thus each action that needs
 * them has a second binding that reaches the browser.
 */

/** An action of the window manager. */
export type WmAction =
  | { readonly type: "switch"; readonly backward: boolean }
  | { readonly type: "maximize" }
  | { readonly type: "minimize" }
  | { readonly type: "snap"; readonly side: "left" | "right" }
  | { readonly type: "workspace"; readonly delta: -1 | 1; readonly carry: boolean }
  | { readonly type: "workspace-index"; readonly index: number }
  | { readonly type: "tile" }
  | { readonly type: "overview" }
  | { readonly type: "layouts" };

/** The key that starts the switcher, as `KeyboardEvent.code`. */
const SWITCH_CODES = new Set(["Tab", "Backquote"]);

/** Returns the action of a key press, or `null` if the window manager does not use the key. */
export function actionFor(event: Pick<KeyboardEvent, "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">): WmAction | null {
  const { code, altKey, ctrlKey, metaKey, shiftKey } = event;
  // Alt+Tab, with Keyboard Lock. Alt+` reaches the browser on each client.
  if (altKey && !ctrlKey && !metaKey && SWITCH_CODES.has(code)) {
    return { type: "switch", backward: shiftKey };
  }
  // Alt+Shift+G: the overview, and Alt+Shift+L: the layout picker (F043). Super+Tab and Super+Z with Keyboard Lock.
  if (altKey && shiftKey && !ctrlKey && !metaKey && code === "KeyG") return { type: "overview" };
  if (altKey && shiftKey && !ctrlKey && !metaKey && code === "KeyL") return { type: "layouts" };
  // Ctrl+Alt+Arrow: the workspace on the left or right. With Shift, the window goes along.
  if (ctrlKey && altKey && !metaKey && (code === "ArrowLeft" || code === "ArrowRight")) {
    return { type: "workspace", delta: code === "ArrowLeft" ? -1 : 1, carry: shiftKey };
  }
  // Super with Keyboard Lock.
  if (metaKey && !altKey && !ctrlKey) {
    switch (code) {
      case "ArrowUp":
        return { type: "maximize" };
      case "ArrowDown":
        return { type: "minimize" };
      case "ArrowLeft":
        return { type: "snap", side: "left" };
      case "ArrowRight":
        return { type: "snap", side: "right" };
      case "KeyT":
        return { type: "tile" };
      case "Tab":
        return { type: "overview" };
      case "KeyZ":
        return { type: "layouts" };
    }
    const digit = /^Digit([1-9])$/.exec(code);
    if (digit) {
      return { type: "workspace-index", index: Number(digit[1]) - 1 };
    }
  }
  return null;
}

/**
 * Detects a tap of the Super key: Super goes down and up with no other key between.
 * The tap opens the launcher. It reaches the browser only with Keyboard Lock.
 */
export class SuperTap {
  #down = false;
  #other = false;

  /** Handles a keydown event. */
  down(event: Pick<KeyboardEvent, "key">): void {
    if (isSuper(event.key)) {
      this.#down = true;
      this.#other = false;
    } else if (this.#down) {
      this.#other = true;
    }
  }

  /** Handles a keyup event. Returns `true` if it ends a tap of Super. */
  up(event: Pick<KeyboardEvent, "key">): boolean {
    if (!isSuper(event.key)) {
      return false;
    }
    const tap = this.#down && !this.#other;
    this.#down = false;
    return tap;
  }

  /** Forgets a press, for example when the window loses the focus. */
  reset(): void {
    this.#down = false;
  }
}

function isSuper(key: string): boolean {
  return key === "Meta" || key === "OS" || key === "Super";
}

/** Reports whether a key press opens the launcher with no Keyboard Lock: Alt+F1. */
export function isLauncherKey(event: Pick<KeyboardEvent, "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">): boolean {
  return event.code === "F1" && event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey;
}
