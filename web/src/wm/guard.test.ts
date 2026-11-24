import { afterEach, expect, test, vi } from "vitest";
import { enterLockedFullscreen, exitLockedFullscreen, guardUnload, keyboardLockSupported, lockedFullscreen, startLockGuard } from "./guard";

function stubFullscreen() {
  let element: Element | null = null;
  Object.defineProperty(document, "fullscreenElement", { configurable: true, get: () => element });
  document.documentElement.requestFullscreen = vi.fn(async () => {
    element = document.documentElement;
    document.dispatchEvent(new Event("fullscreenchange"));
  });
  document.exitFullscreen = vi.fn(async () => {
    element = null;
    document.dispatchEvent(new Event("fullscreenchange"));
  });
}

function stubKeyboard() {
  const keyboard = { lock: vi.fn(async () => {}), unlock: vi.fn() };
  Object.defineProperty(navigator, "keyboard", { configurable: true, value: keyboard });
  return keyboard;
}

function ctrlW() {
  return new KeyboardEvent("keydown", { code: "KeyW", ctrlKey: true, cancelable: true });
}

afterEach(() => {
  delete (navigator as { keyboard?: unknown }).keyboard;
});

test("the browser asks before it unloads the page while the guard is active", () => {
  const target = new EventTarget();
  const stop = guardUnload(target);
  const first = new Event("beforeunload", { cancelable: true });
  target.dispatchEvent(first);
  expect(first.defaultPrevented).toBe(true);
  stop();
  const second = new Event("beforeunload", { cancelable: true });
  target.dispatchEvent(second);
  expect(second.defaultPrevented).toBe(false);
});

test("fullscreen locks the keyboard, and leaving fullscreen unlocks it", async () => {
  stubFullscreen();
  const keyboard = stubKeyboard();
  const stop = startLockGuard();
  expect(keyboardLockSupported()).toBe(true);
  await enterLockedFullscreen();
  expect(keyboard.lock).toHaveBeenCalledOnce();
  expect(lockedFullscreen()).toBe(true);
  // A long press of Esc leaves fullscreen without the shell.
  await document.exitFullscreen();
  expect(keyboard.unlock).toHaveBeenCalled();
  expect(lockedFullscreen()).toBe(false);
  stop();
});

test("fullscreen works with no Keyboard Lock", async () => {
  stubFullscreen();
  const stop = startLockGuard();
  expect(keyboardLockSupported()).toBe(false);
  await enterLockedFullscreen();
  expect(lockedFullscreen()).toBe(true);
  await exitLockedFullscreen();
  expect(lockedFullscreen()).toBe(false);
  stop();
});

test("Ctrl+W is prevented only in fullscreen", async () => {
  stubFullscreen();
  stubKeyboard();
  const stop = startLockGuard();
  const before = ctrlW();
  window.dispatchEvent(before);
  expect(before.defaultPrevented).toBe(false);
  await enterLockedFullscreen();
  const inside = ctrlW();
  window.dispatchEvent(inside);
  expect(inside.defaultPrevented).toBe(true);
  const plain = new KeyboardEvent("keydown", { code: "KeyW", cancelable: true });
  window.dispatchEvent(plain);
  expect(plain.defaultPrevented).toBe(false);
  await exitLockedFullscreen();
  stop();
});
