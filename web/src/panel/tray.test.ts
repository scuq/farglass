import { describe, expect, test } from "vitest";
import type { TrayItem } from "../proto";
import { applyEvent, iconUrl, itemTip, scrollDelta, sortedItems } from "./tray";

const item = (id: number, title: string, more: Partial<TrayItem> = {}): TrayItem => ({ id, name: title.toLowerCase(), title, status: "active", menu: false, menu_only: false, ...more });

describe("tray items", () => {
  test("an event sets all items, adds or replaces one, or removes one", () => {
    let items = applyEvent([], { type: "items", items: [item(1, "Seafile")] });
    items = applyEvent(items, { type: "item", item: item(2, "Bitwarden") });
    expect(items.map((each) => each.id)).toEqual([1, 2]);
    items = applyEvent(items, { type: "item", item: item(1, "Seafile", { status: "needs-attention" }) });
    expect(items.map((each) => each.status)).toEqual(["needs-attention", "active"]);
    items = applyEvent(items, { type: "removed", id: 1 });
    expect(items.map((each) => each.title)).toEqual(["Bitwarden"]);
    // A removal of an item that is not there changes nothing.
    expect(applyEvent(items, { type: "removed", id: 9 })).toEqual(items);
  });

  test("the order is the title, then the id", () => {
    const sorted = sortedItems([item(3, "seafile"), item(2, "Bitwarden"), item(1, "Bitwarden")]);
    expect(sorted.map((each) => each.id)).toEqual([1, 2, 3]);
  });

  test("the icon is a data URL of a PNG or an SVG file only", () => {
    expect(iconUrl(item(1, "A", { icon: "AAAA", icon_mime: "image/png" }))).toBe("data:image/png;base64,AAAA");
    expect(iconUrl(item(1, "A", { icon: "AAAA", icon_mime: "image/svg+xml" }))).toBe("data:image/svg+xml;base64,AAAA");
    expect(iconUrl(item(1, "A", { icon: "AAAA", icon_mime: "text/html" }))).toBeUndefined();
    expect(iconUrl(item(1, "A"))).toBeUndefined();
  });

  test("the tooltip adds the text of the program to the title", () => {
    expect(itemTip(item(1, "Seafile"))).toBe("Seafile");
    expect(itemTip(item(1, "Seafile", { tooltip: "2 files to sync" }))).toBe("Seafile: 2 files to sync");
  });

  test("one step of the wheel up is 120", () => {
    expect(scrollDelta(-100)).toBe(120);
    expect(scrollDelta(100)).toBe(-120);
    expect(scrollDelta(0)).toBe(0);
  });
});
