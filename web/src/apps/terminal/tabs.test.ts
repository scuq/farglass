import { describe, expect, it } from "vitest";
import { byWindow } from "../../stats";
import { addTab, closeTab, initialLayout, parseLayout, selectTab, stepTab } from "./tabs";

describe("terminal tabs", () => {
  it("starts with one tab that has the id of the window", () => {
    expect(parseLayout(null, "w1")).toEqual({ tabs: ["w1"], active: "w1", next: 2 });
  });

  it("adds a tab at the end or after the active tab, and makes it active", () => {
    const two = addTab(initialLayout("w1"), "w1", false);
    expect(two).toEqual({ tabs: ["w1", "w1/2"], active: "w1/2", next: 3 });
    const three = addTab(selectTab(two, "w1"), "w1", true);
    expect(three.tabs).toEqual(["w1", "w1/3", "w1/2"]);
    expect(three.active).toBe("w1/3");
  });

  it("closes a tab, activates its neighbor, and keeps the last tab", () => {
    const layout = { tabs: ["w1", "w1/2", "w1/3"], active: "w1/2", next: 4 };
    expect(closeTab(layout, "w1/2")).toEqual({ tabs: ["w1", "w1/3"], active: "w1/3", next: 4 });
    expect(closeTab({ ...layout, active: "w1/3" }, "w1/3").active).toBe("w1/2");
    expect(closeTab(initialLayout("w1"), "w1")).toEqual(initialLayout("w1"));
  });

  it("steps through the tabs with wrap-around", () => {
    const layout = { tabs: ["w1", "w1/2", "w1/3"], active: "w1", next: 4 };
    expect(stepTab(layout, 1).active).toBe("w1/2");
    expect(stepTab(layout, -1).active).toBe("w1/3");
  });

  it("drops terminals of other windows and repairs a stored layout", () => {
    const layout = parseLayout({ tabs: ["w1/5", "w2", 7, "w1/5", "w1"], active: "gone", next: 1 }, "w1");
    expect(layout).toEqual({ tabs: ["w1/5", "w1"], active: "w1/5", next: 6 });
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
