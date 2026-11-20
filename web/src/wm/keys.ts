/**
 * The keys of the window manager. The keybinding schemes (F044) name the keys:
 * this module turns the action of a key press into an action of the window manager.
 */

import { type ActionId, actionOf, type SchemeId } from "../keys/scheme";

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
  | { readonly type: "layouts" }
  | { readonly type: "launcher" }
  | { readonly type: "new-terminal" }
  | { readonly type: "new-window" }
  | { readonly type: "close-window" };

/** Returns the action of the window manager for an action of a scheme. */
export function wmAction(id: ActionId): WmAction {
  switch (id) {
    case "switch":
    case "switch-back":
      return { type: "switch", backward: id === "switch-back" };
    case "snap-left":
    case "snap-right":
      return { type: "snap", side: id === "snap-left" ? "left" : "right" };
    case "workspace-left":
    case "workspace-right":
    case "carry-left":
    case "carry-right":
      return { type: "workspace", delta: id.endsWith("left") ? -1 : 1, carry: id.startsWith("carry") };
    case "workspace-1":
    case "workspace-2":
    case "workspace-3":
    case "workspace-4":
      return { type: "workspace-index", index: Number(id.slice(-1)) - 1 };
    default:
      return { type: id };
  }
}

type KeyEvent = Pick<KeyboardEvent, "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">;

/**
 * Returns the action of a key press in `scheme`, or `null` if the window manager does not use
 * the key. `shellFocused` reports whether a window of the shell has the focus.
 */
export function actionFor(event: KeyEvent, scheme: SchemeId = "pc", shellFocused = true): WmAction | null {
  const id = actionOf(event, scheme, shellFocused);
  return id ? wmAction(id) : null;
}

/** Returns the scheme action of a key press, for the tour. */
export function schemeActionFor(event: KeyEvent, scheme: SchemeId, shellFocused: boolean): ActionId | null {
  return actionOf(event, scheme, shellFocused);
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

/** Reports whether a key press opens the launcher with no Keyboard Lock: Alt+F1, or Ctrl+Shift+Space on a Mac. */
export function isLauncherKey(event: KeyEvent, scheme: SchemeId = "pc"): boolean {
  return actionOf(event, scheme, true) === "launcher";
}
