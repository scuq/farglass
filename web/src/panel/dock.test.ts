import { expect, test } from "vitest";
import type { Win } from "../wm/model";
import { dockItems, windowToShow } from "./dock";

const win = (id: string, app: string): Win => ({ id, app, title: id, rect: { x: 0, y: 0, w: 100, h: 100 }, snap: null, minimized: false, workspace: 0 });
const keyOf = (each: Win) => each.app;

test("pinned apps come first, then the other apps in the order in which they opened", () => {
  const windows = [win("t1", "terminal"), win("c1", "host:calc"), win("s1", "settings"), win("t2", "terminal")];
  const { items, order } = dockItems(windows, ["t2", "s1", "c1", "t1"], ["settings", "host:editor"], [], keyOf);
  expect(items.map((item) => [item.key, item.pinned, item.windows])).toEqual([
    ["settings", true, ["s1"]],
    ["host:editor", true, []],
    ["terminal", false, ["t2", "t1"]],
    ["host:calc", false, ["c1"]],
  ]);
  expect(order).toEqual(["terminal", "host:calc"]);
});

test("an app keeps its place while it has windows, and a new app goes to the end", () => {
  const first = dockItems([win("a", "terminal"), win("b", "settings")], [], [], [], keyOf);
  // The stack order changed: the dock order stays.
  const second = dockItems([win("b", "settings"), win("a", "terminal"), win("c", "echo")], [], [], first.order, keyOf);
  expect(second.order).toEqual(["terminal", "settings", "echo"]);
  const third = dockItems([win("c", "echo"), win("b", "settings")], [], [], second.order, keyOf);
  expect(third.order).toEqual(["settings", "echo"]);
});

test("a click shows the last used window, and the next clicks go through each window", () => {
  const item = { key: "terminal", pinned: false, windows: ["t3", "t1", "t2"] };
  expect(windowToShow(item, null)).toBe("t3");
  expect(windowToShow(item, "x")).toBe("t3");
  expect(windowToShow(item, "t1")).toBe("t2");
  expect(windowToShow(item, "t2")).toBe("t3");
  expect(windowToShow(item, "t3")).toBe("t1");
  expect(windowToShow({ key: "settings", pinned: true, windows: [] }, null)).toBeNull();
});
