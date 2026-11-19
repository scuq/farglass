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
  | { readonly type: "tile" };

/** The key that starts the switcher, as `KeyboardEvent.code`. */
const SWITCH_CODES = new Set(["Tab", "Backquote"]);

/** Returns the action of a key press, or `null` if the window manager does not use the key. */
export function actionFor(event: Pick<KeyboardEvent, "code" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">): WmAction | null {
  const { code, altKey, ctrlKey, metaKey, shiftKey } = event;
  // Alt+Tab, with Keyboard Lock. Alt+` reaches the browser on each client.
  if (altKey && !ctrlKey && !metaKey && SWITCH_CODES.has(code)) {
    return { type: "switch", backward: shiftKey };
  }
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
    }
    const digit = /^Digit([1-9])$/.exec(code);
    if (digit) {
      return { type: "workspace-index", index: Number(digit[1]) - 1 };
    }
  }
  return null;
}
