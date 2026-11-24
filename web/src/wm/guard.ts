/**
 * Guards the tab of the desktop against a close by accident (F096).
 *
 * In a usual tab, the browser keeps Ctrl+W, and a page cannot stop it. The
 * browser asks before it unloads the page, so Ctrl+W, a tab close, or a reload
 * does not end the work at once. In fullscreen with Keyboard Lock (Chromium),
 * Ctrl+W, Ctrl+T, and Ctrl+N reach the shell and the browser does not act on them.
 */

import { createSignal } from "solid-js";

/** The part of `navigator.keyboard` that the guard uses. Only Chromium has it. */
interface KeyboardLock {
  lock(codes?: string[]): Promise<void>;
  unlock(): void;
}

function keyboardLock(): KeyboardLock | null {
  const keyboard = (navigator as Navigator & { keyboard?: Partial<KeyboardLock> }).keyboard;
  return keyboard && typeof keyboard.lock === "function" && typeof keyboard.unlock === "function" ? (keyboard as KeyboardLock) : null;
}

/** Reports whether the browser has Keyboard Lock. */
export function keyboardLockSupported(): boolean {
  return keyboardLock() !== null;
}

const [fullscreen, setFullscreen] = createSignal(false);

/** Reports whether the shell is in fullscreen. */
export const lockedFullscreen = fullscreen;

/** The keys of the browser that a terminal, a Linux app, or the shell uses with Ctrl, or with Cmd on a Mac. */
const BROWSER_KEYS = new Set(["KeyW", "KeyT", "KeyN"]);

/** Makes the browser ask before it unloads the page. Returns the function that stops the guard. */
export function guardUnload(target: EventTarget = window): () => void {
  const onBeforeUnload = (event: Event) => {
    event.preventDefault();
    (event as BeforeUnloadEvent).returnValue = "";
  };
  target.addEventListener("beforeunload", onBeforeUnload);
  return () => target.removeEventListener("beforeunload", onBeforeUnload);
}

/** Puts the shell in fullscreen and locks the keyboard. With no Keyboard Lock, the shell is in fullscreen only. */
export async function enterLockedFullscreen(): Promise<void> {
  if (!document.fullscreenElement) {
    await document.documentElement.requestFullscreen();
  }
  setFullscreen(true);
  await keyboardLock()
    ?.lock()
    .catch(() => {});
}

/** Unlocks the keyboard and leaves fullscreen. */
export async function exitLockedFullscreen(): Promise<void> {
  keyboardLock()?.unlock();
  setFullscreen(false);
  if (document.fullscreenElement) {
    await document.exitFullscreen();
  }
}

/**
 * Follows fullscreen: when the user leaves it, for example with a long press of
 * Esc, the keyboard unlocks. In fullscreen, the browser does not act on Ctrl+W,
 * Ctrl+T, and Ctrl+N. The apps still get these keys, because the guard does not
 * stop the event. Returns the function that stops the guard.
 */
export function startLockGuard(target: EventTarget = window, doc: Document = document): () => void {
  const onChange = () => {
    if (doc.fullscreenElement) {
      setFullscreen(true);
    } else {
      keyboardLock()?.unlock();
      setFullscreen(false);
    }
  };
  const onKeyDown = (event: Event) => {
    const key = event as KeyboardEvent;
    if (fullscreen() && (key.ctrlKey || key.metaKey) && !key.altKey && BROWSER_KEYS.has(key.code)) {
      key.preventDefault();
    }
  };
  doc.addEventListener("fullscreenchange", onChange);
  target.addEventListener("keydown", onKeyDown, true);
  return () => {
    doc.removeEventListener("fullscreenchange", onChange);
    target.removeEventListener("keydown", onKeyDown, true);
  };
}
