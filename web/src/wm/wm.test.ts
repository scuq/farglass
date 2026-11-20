// @vitest-environment node
import { describe, expect, test } from "vitest";
import { clampRect, MIN_SIZE, type Rect, resizeRect, snapRect, snapZoneAt, tileRects, TITLE_HEIGHT, unsnapRect } from "./geometry";
import { actionFor } from "./keys";
import {
  closeWindow,
  type Desktop,
  emptyDesktop,
  focused,
  focusWindow,
  minimizeWindow,
  moveToWorkspace,
  openWindow,
  parseDesktop,
  placeWindow,
  retitleWindow,
  shownRect,
  snapWindow,
  switchable,
  switchWorkspace,
  tileWorkspace,
  toggleMaximize,
  windowById,
} from "./model";
import { openSwitcher, selected, step } from "./switcher";

const AREA: Rect = { x: 0, y: 0, w: 1600, h: 900 };

function overlap(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

function inside(rect: Rect, area: Rect): boolean {
  return rect.x >= area.x && rect.y >= area.y && rect.x + rect.w <= area.x + area.w && rect.y + rect.h <= area.y + area.h;
}

function desktopWith(count: number): Desktop {
  let desktop = emptyDesktop();
  for (let i = 0; i < count; i++) {
    desktop = openWindow(desktop, { id: `w${i}`, app: "echo", title: `Window ${i}`, w: 600, h: 400 }, AREA);
  }
  return desktop;
}

describe("geometry", () => {
  test("clampRect keeps the title bar on the desktop and the size between the limits", () => {
    expect(clampRect({ x: -5000, y: -50, w: 100, h: 50 }, AREA)).toEqual({ x: -MIN_SIZE.w + 96, y: 0, w: MIN_SIZE.w, h: MIN_SIZE.h });
    const far = clampRect({ x: 5000, y: 5000, w: 3000, h: 3000 }, AREA);
    expect(far).toEqual({ x: AREA.w - 96, y: AREA.h - TITLE_HEIGHT, w: AREA.w, h: AREA.h });
  });

  test.each([
    [0, 450, "left"],
    [1599, 450, "right"],
    [800, 0, "maximized"],
    [0, 10, "top-left"],
    [1599, 890, "bottom-right"],
    [800, 450, null],
  ])("snapZoneAt(%i, %i) is %s", (x, y, zone) => {
    expect(snapZoneAt(x, y, AREA)).toBe(zone);
  });

  test("the snap zones of the two sides fill the desktop with no overlap", () => {
    const left = snapRect("left", { x: 0, y: 0, w: 1601, h: 901 });
    const right = snapRect("right", { x: 0, y: 0, w: 1601, h: 901 });
    expect(left.w + right.w).toBe(1601);
    expect(overlap(left, right)).toBe(false);
    const quarters = (["top-left", "top-right", "bottom-left", "bottom-right"] as const).map((zone) => snapRect(zone, AREA));
    expect(quarters.reduce((sum, rect) => sum + rect.w * rect.h, 0)).toBe(AREA.w * AREA.h);
  });

  test("resizeRect keeps the opposite edge and the smallest size", () => {
    const start = { x: 100, y: 100, w: 500, h: 400 };
    expect(resizeRect(start, "se", 50, 20)).toEqual({ x: 100, y: 100, w: 550, h: 420 });
    expect(resizeRect(start, "nw", 50, 20)).toEqual({ x: 150, y: 120, w: 450, h: 380 });
    const small = resizeRect(start, "w", 1000, 0);
    expect(small.w).toBe(MIN_SIZE.w);
    expect(small.x + small.w).toBe(start.x + start.w);
  });

  test.each([1, 2, 3, 4, 5, 6, 7, 9, 12])("tileRects(%i) fills the desktop with no overlap", (count) => {
    const rects = tileRects(count, AREA);
    expect(rects).toHaveLength(count);
    for (const [i, rect] of rects.entries()) {
      expect(inside(rect, AREA)).toBe(true);
      for (const other of rects.slice(i + 1)) {
        expect(overlap(rect, other)).toBe(false);
      }
    }
  });

  test("unsnapRect keeps the pointer at the same place on the title bar", () => {
    const rect = unsnapRect({ x: 0, y: 0, w: 400, h: 300 }, { x: 0, y: 0, w: 1600, h: 900 }, 800, 10);
    expect(rect.w).toBe(400);
    expect(rect.x).toBe(600);
  });
});

describe("model", () => {
  test("new windows open in a cascade, on top, with the focus", () => {
    const desktop = desktopWith(3);
    expect(desktop.windows.map((win) => win.id)).toEqual(["w0", "w1", "w2"]);
    expect(focused(desktop)).toBe("w2");
    expect(windowById(desktop, "w1")!.rect.x).toBeGreaterThan(windowById(desktop, "w0")!.rect.x);
  });

  test("focus puts the window on top and first in the order of use", () => {
    const desktop = focusWindow(desktopWith(3), "w0");
    expect(desktop.windows.at(-1)!.id).toBe("w0");
    expect(desktop.recent).toEqual(["w0", "w2", "w1"]);
  });

  test("minimize gives the focus to the last used window and focus shows the window again", () => {
    let desktop = minimizeWindow(desktopWith(3), "w2");
    expect(windowById(desktop, "w2")!.minimized).toBe(true);
    expect(focused(desktop)).toBe("w1");
    desktop = focusWindow(desktop, "w2");
    expect(windowById(desktop, "w2")!.minimized).toBe(false);
    expect(focused(desktop)).toBe("w2");
  });

  test("maximize and restore give back the floating rectangle", () => {
    const before = windowById(desktopWith(1), "w0")!;
    let desktop = toggleMaximize(desktopWith(1), "w0");
    expect(shownRect(windowById(desktop, "w0")!, AREA)).toEqual(AREA);
    desktop = toggleMaximize(desktop, "w0");
    expect(shownRect(windowById(desktop, "w0")!, AREA)).toEqual(before.rect);
  });

  test("snap shows the zone and a move makes the window float again", () => {
    let desktop = snapWindow(desktopWith(1), "w0", "left");
    expect(shownRect(windowById(desktop, "w0")!, AREA)).toEqual({ x: 0, y: 0, w: 800, h: 900 });
    desktop = placeWindow(desktop, "w0", { x: 10, y: 10, w: 500, h: 300 });
    expect(windowById(desktop, "w0")!.snap).toBeNull();
  });

  test("workspaces show their own windows", () => {
    let desktop = desktopWith(2);
    desktop = moveToWorkspace(desktop, "w1", 1);
    expect(desktop.workspace).toBe(1);
    expect(switchable(desktop)).toEqual(["w1"]);
    desktop = switchWorkspace(desktop, 0);
    expect(focused(desktop)).toBe("w0");
    desktop = switchWorkspace(desktop, 2);
    expect(focused(desktop)).toBeNull();
    expect(switchWorkspace(desktop, 99).workspace).toBe(desktop.workspaces - 1);
  });

  test("tile places the windows of the workspace with no overlap and skips minimized ones", () => {
    let desktop = minimizeWindow(desktopWith(4), "w3");
    desktop = snapWindow(desktop, "w0", "maximized");
    desktop = tileWorkspace(desktop, AREA);
    const shown = ["w0", "w1", "w2"].map((id) => shownRect(windowById(desktop, id)!, AREA));
    expect(shown.every((rect) => inside(rect, AREA))).toBe(true);
    expect(overlap(shown[0]!, shown[1]!) || overlap(shown[1]!, shown[2]!) || overlap(shown[0]!, shown[2]!)).toBe(false);
    expect(windowById(desktop, "w3")!.minimized).toBe(true);
  });

  test("retitle changes only the title, and the same title changes nothing", () => {
    const desktop = desktopWith(2);
    const renamed = retitleWindow(desktop, "w0", "bash: ~");
    expect(windowById(renamed, "w0")!.title).toBe("bash: ~");
    expect(windowById(renamed, "w1")).toBe(windowById(desktop, "w1"));
    expect(retitleWindow(renamed, "w0", "bash: ~")).toBe(renamed);
    expect(retitleWindow(desktop, "missing", "x")).toBe(desktop);
  });

  test("close removes the window from the order of use", () => {
    const desktop = closeWindow(desktopWith(2), "w1");
    expect(desktop.recent).toEqual(["w0"]);
    expect(focused(desktop)).toBe("w0");
  });

  test("parseDesktop accepts a stored desktop and drops values that are not valid", () => {
    const desktop = desktopWith(2);
    expect(parseDesktop(JSON.parse(JSON.stringify(desktop)))).toEqual(desktop);
    expect(parseDesktop(null)).toBeNull();
    expect(parseDesktop({ windows: "no" })).toBeNull();
    const damaged = parseDesktop({ windows: [{ id: "a" }], recent: ["a", 5], workspace: 9, workspaces: 2 });
    expect(damaged).toEqual({ windows: [], recent: [], workspace: 1, workspaces: 2 });
  });
});

describe("switcher", () => {
  test("Alt+Tab selects the last used window, and Tab steps with a wrap", () => {
    let switcher = openSwitcher(["a", "b", "c"])!;
    expect(selected(switcher)).toBe("b");
    switcher = step(step(switcher));
    expect(selected(switcher)).toBe("a");
    expect(selected(openSwitcher(["a", "b", "c"], true)!)).toBe("c");
    expect(selected(openSwitcher(["a"])!)).toBe("a");
    expect(openSwitcher([])).toBeNull();
  });
});

describe("keys", () => {
  const key = (code: string, mods: Partial<Record<"altKey" | "ctrlKey" | "metaKey" | "shiftKey", boolean>> = {}) =>
    actionFor({ code, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...mods });

  test("each action has a binding that reaches the browser with no Keyboard Lock", () => {
    expect(key("Backquote", { altKey: true })).toEqual({ type: "switch", backward: false });
    expect(key("Tab", { altKey: true, shiftKey: true })).toEqual({ type: "switch", backward: true });
    expect(key("ArrowRight", { ctrlKey: true, altKey: true })).toEqual({ type: "workspace", delta: 1, carry: false });
    expect(key("ArrowLeft", { ctrlKey: true, altKey: true, shiftKey: true })).toEqual({ type: "workspace", delta: -1, carry: true });
    expect(key("Digit3", { metaKey: true })).toEqual({ type: "workspace-index", index: 2 });
  });

  test("keys that apps use stay with the apps", () => {
    expect(key("Tab")).toBeNull();
    // Ctrl+T reaches the shell only with Keyboard Lock, and acts only in a window of the shell.
    expect(actionFor({ code: "KeyT", ctrlKey: true, altKey: false, metaKey: false, shiftKey: false }, "pc", false)).toBeNull();
    expect(key("KeyT", { ctrlKey: true })).toEqual({ type: "new-terminal" });
    expect(key("ArrowLeft", { altKey: true })).toBeNull();
    expect(key("Digit1", { altKey: true })).toBeNull();
  });
});
