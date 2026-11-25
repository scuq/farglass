import { describe, expect, it } from "vitest";
import { byWindow } from "../../stats";
import { activeTab, addTab, allPanes, closePane, closeTab, focusPane, geometry, initialLayout, type PaneNode, paneInDirection, parseLayout, resizeSplit, selectTab, splitPane, stepTab, type TabLayout } from "./tabs";

const one = (id: string) => ({ id, root: { pane: id }, focus: id });
const ids = (layout: TabLayout) => layout.tabs.map((tab) => tab.id);

describe("terminal tabs", () => {
  it("starts with one tab that has the id of the window", () => {
    expect(parseLayout(null, "w1")).toEqual(initialLayout("w1"));
    expect(initialLayout("w1")).toEqual({ tabs: [one("w1")], active: "w1", next: 2 });
  });

  it("adds a tab at the end or after the active tab, and makes it active", () => {
    const two = addTab(initialLayout("w1"), "w1", false);
    expect(ids(two)).toEqual(["w1", "w1/2"]);
    expect(two.active).toBe("w1/2");
    const three = addTab(selectTab(two, "w1"), "w1", true);
    expect(ids(three)).toEqual(["w1", "w1/3", "w1/2"]);
    expect(three.active).toBe("w1/3");
  });

  it("closes a tab, activates its neighbor, and keeps the last tab", () => {
    const layout = { tabs: [one("w1"), one("w1/2"), one("w1/3")], active: "w1/2", next: 4 };
    expect(closeTab(layout, "w1/2")).toEqual({ tabs: [one("w1"), one("w1/3")], active: "w1/3", next: 4 });
    expect(closeTab({ ...layout, active: "w1/3" }, "w1/3").active).toBe("w1/2");
    expect(closeTab(initialLayout("w1"), "w1")).toEqual(initialLayout("w1"));
  });

  it("steps through the tabs with wrap-around", () => {
    const layout = { tabs: [one("w1"), one("w1/2"), one("w1/3")], active: "w1", next: 4 };
    expect(stepTab(layout, 1).active).toBe("w1/2");
    expect(stepTab(layout, -1).active).toBe("w1/3");
  });

  it("reads the list of terminal ids of the first version, and drops terminals of other windows", () => {
    const layout = parseLayout({ tabs: ["w1/5", "w2", 7, "w1/5", "w1"], active: "gone", next: 1 }, "w1");
    expect(layout).toEqual({ tabs: [one("w1/5"), one("w1")], active: "w1/5", next: 6 });
  });

  it("counts the terminals of the tabs for their window", () => {
    const merged = byWindow([
      { window: "w1", usage: { cpu: 1, memory: 10 } },
      { window: "w1/2", usage: { cpu: 2, memory: 20, net: 5 } },
      { window: "surface-3", usage: { cpu: 4, memory: 40 } },
    ]);
    expect(merged).toEqual([
      { window: "w1", usage: { cpu: 3, memory: 30, net: 5 } },
      { window: "surface-3", usage: { cpu: 4, memory: 40 } },
    ]);
  });
});

describe("terminal splits", () => {
  // w1 | (w1/2 over w1/3)
  const split = () => splitPane(splitPane(initialLayout("w1"), "w1", "w1", "row"), "w1", "w1/2", "column");

  it("splits a pane, and the new pane gets the focus", () => {
    const layout = split();
    expect(allPanes(layout)).toEqual(["w1", "w1/2", "w1/3"]);
    expect(activeTab(layout).focus).toBe("w1/3");
    expect(layout.next).toBe(4);
    const { panes, dividers } = geometry(activeTab(layout).root);
    expect(panes.get("w1")).toEqual({ x: 0, y: 0, w: 0.5, h: 1 });
    expect(panes.get("w1/3")).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
    expect(dividers.map((each) => each.dir)).toEqual(["row", "column"]);
  });

  it("finds the pane in a direction", () => {
    const root = activeTab(split()).root;
    expect(paneInDirection(root, "w1", "right")).toBe("w1/2");
    expect(paneInDirection(root, "w1/3", "up")).toBe("w1/2");
    expect(paneInDirection(root, "w1/3", "left")).toBe("w1");
    expect(paneInDirection(root, "w1", "left")).toBeUndefined();
  });

  it("closes a pane: the neighbor gets its space and the focus", () => {
    const layout = closePane(split(), "w1/3");
    expect(activeTab(layout).root).toEqual({ split: "w1/2", dir: "row", ratio: 0.5, a: { pane: "w1" }, b: { pane: "w1/2" } });
    expect(activeTab(layout).focus).toBe("w1/2");
    const last = closePane(closePane(layout, "w1"), "w1/2");
    expect(allPanes(last)).toEqual(["w1/2"]);
  });

  it("closes the tab with its last pane", () => {
    const two = addTab(initialLayout("w1"), "w1", false);
    expect(ids(closePane(two, "w1/2"))).toEqual(["w1"]);
  });

  it("keeps a ratio from 0.1 to 0.9, and reads a stored split tree", () => {
    const layout = resizeSplit(split(), "w1/2", 0.02);
    expect((activeTab(layout).root as Extract<PaneNode, { split: string }>).ratio).toBe(0.1);
    expect(parseLayout(JSON.parse(JSON.stringify(layout)), "w1")).toEqual(layout);
  });

  it("gives a pane the focus and makes its tab active", () => {
    const layout = addTab(split(), "w1", false);
    const back = focusPane(layout, "w1");
    expect(back.active).toBe("w1");
    expect(activeTab(back).focus).toBe("w1");
  });
});
