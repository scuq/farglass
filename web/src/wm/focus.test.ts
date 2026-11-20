import { afterEach, describe, expect, test, vi } from "vitest";
import { HOLD_MAX_MS, InputWatch, LAUNCH_MS, PAUSE_MS, parseFocusSettings, REQUEST_MS } from "./focus";
import { emptyDesktop, focusWindow, focused, openWindow } from "./model";

afterEach(() => vi.useRealTimers());

const AREA = { x: 0, y: 0, w: 1024, h: 768 };

function setup() {
  vi.useFakeTimers();
  let now = 0;
  const target = new EventTarget();
  const watch = new InputWatch(target, () => now);
  const advance = (ms: number) => {
    now += ms;
    vi.advanceTimersByTime(ms);
  };
  const key = (key: string) => target.dispatchEvent(Object.assign(new Event("keydown"), { key }));
  const press = () => target.dispatchEvent(new Event("pointerdown"));
  const move = () => target.dispatchEvent(new Event("pointermove"));
  return { watch, advance, key, press, move };
}

describe("InputWatch", () => {
  test("typing and moving are busy, and a pause starts after a quiet time", () => {
    const { watch, advance, key, move } = setup();
    const pauses: number[] = [];
    watch.onPause(() => pauses.push(1));
    expect(watch.busy()).toBe(false);
    key("a");
    expect(watch.busy()).toBe(true);
    advance(PAUSE_MS - 100);
    move();
    advance(PAUSE_MS - 100);
    expect(pauses).toEqual([]);
    advance(200);
    expect(watch.busy()).toBe(false);
    expect(pauses).toEqual([1]);
    watch.stop();
  });

  test("a click or Enter asks for a window, typing after it does not, and the request ends", () => {
    const { watch, advance, key, press } = setup();
    expect(watch.requested()).toBe(false);
    key("Enter");
    expect(watch.requested()).toBe(true);
    key("Shift");
    expect(watch.requested()).toBe(true);
    key("x");
    expect(watch.requested()).toBe(false);
    press();
    advance(REQUEST_MS - 1);
    expect(watch.requested()).toBe(true);
    advance(1);
    expect(watch.requested()).toBe(false);
    watch.stop();
  });

  test("a start from the launcher asks for one window of its app for a time", () => {
    const { watch, advance } = setup();
    watch.expectLaunch(["org.gnome.calculator", "gnome-calculator"]);
    expect(watch.launched("vlc")).toBe(false);
    expect(watch.launched("gnome-calculator")).toBe(true);
    expect(watch.launched("gnome-calculator")).toBe(false);
    watch.expectLaunch(["code"]);
    advance(LAUNCH_MS);
    expect(watch.launched("Code")).toBe(false);
    watch.stop();
  });
});

describe("quiet windows", () => {
  test("a window with no focus opens under the focused window, with a mark that goes with the focus", () => {
    let desktop = openWindow(emptyDesktop(), { id: "a", app: "echo", title: "A", w: 300, h: 200 }, AREA);
    desktop = openWindow(desktop, { id: "b", app: "linux", title: "B", w: 300, h: 200 }, AREA, { focus: false });
    expect(focused(desktop)).toBe("a");
    expect(desktop.windows.map((win) => win.id)).toEqual(["b", "a"]);
    expect(desktop.recent).toEqual(["a", "b"]);
    expect(desktop.windows[0]!.attention).toBe(true);
    desktop = focusWindow(desktop, "b");
    expect(focused(desktop)).toBe("b");
    expect(desktop.windows.at(-1)!.attention).toBeUndefined();
  });

  test("with no focused window, a window with no focus asked gets the focus anyway", () => {
    const desktop = openWindow(emptyDesktop(), { id: "b", app: "linux", title: "B", w: 300, h: 200 }, AREA, { focus: false });
    expect(focused(desktop)).toBe("b");
  });

  test("the settings take the default for unknown fields", () => {
    expect(parseFocusSettings({ holdNotifications: false, other: 1 })).toEqual({ holdNotifications: false, quietWindows: true });
    expect(parseFocusSettings(null)).toEqual({ holdNotifications: true, quietWindows: true });
    expect(HOLD_MAX_MS).toBeGreaterThan(PAUSE_MS);
  });
});
