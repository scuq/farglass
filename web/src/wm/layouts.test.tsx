import { render } from "@solidjs/web";
import { afterEach, describe, expect, test } from "vitest";
import { encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { Desktop } from "./Desktop";
import { LAYOUTS, layoutRects, type Rect } from "./geometry";
import { emptyDesktop, moveToWorkspace, openWindow, overviewLetters, switchWorkspace } from "./model";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

const AREA = { x: 0, y: 0, w: 1200, h: 800 };
const overlap = (a: Rect, b: Rect) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
const inside = (a: Rect) => a.x >= AREA.x && a.y >= AREA.y && a.x + a.w <= AREA.x + AREA.w && a.y + a.h <= AREA.y + AREA.h;

describe("layouts", () => {
  test("each layout gives one rectangle for each window, inside the area, with no overlap", () => {
    for (const layout of LAYOUTS) {
      for (let count = 1; count <= 7; count++) {
        const rects = layoutRects(layout, count, AREA);
        expect(rects, `${layout} ${count}`).toHaveLength(count);
        rects.forEach((rect, i) => {
          expect(inside(rect), `${layout} ${count} ${i}`).toBe(true);
          expect(rect.w > 0 && rect.h > 0).toBe(true);
          rects.slice(i + 1).forEach((other) => expect(overlap(rect, other), `${layout} ${count}`).toBe(false));
        });
      }
    }
  });

  test("center, wide, and spotlight give the window with the focus the largest place", () => {
    for (const layout of ["center", "wide", "spotlight"] as const) {
      const rects = layoutRects(layout, 4, AREA);
      const area = (rect: Rect) => rect.w * rect.h;
      expect(rects.slice(1).every((rect) => area(rect) < area(rects[0]!)), layout).toBe(true);
    }
    expect(layoutRects("grid", 4, AREA).map((r) => [r.w, r.h])).toEqual([
      [588, 388],
      [588, 388],
      [588, 388],
      [588, 388],
    ]);
  });

  test("the overview gives letters by workspace, the top window first", () => {
    let desktop = emptyDesktop();
    for (const id of ["a", "b", "c"]) desktop = openWindow(desktop, { id, app: "echo", title: id, w: 300, h: 200 }, AREA);
    desktop = moveToWorkspace(desktop, "a", 1);
    expect(overviewLetters(desktop)).toEqual([
      { id: "c", letter: "a" },
      { id: "b", letter: "s" },
      { id: "a", letter: "d" },
    ]);
  });
});

async function mount() {
  const { server, connection } = await fakeConnection("alice");
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <Desktop connection={connection} user="alice" onlogout={() => {}} />, host);
  const channel = server.take().find((f) => f.type === FrameType.Open && parseJson<{ kind: string }>(f).kind === "state")!.channel;
  server.push(readyFrame(channel));
  let desktop = emptyDesktop();
  for (const [id, title] of [
    ["a", "Alpha"],
    ["b", "Beta"],
    ["c", "Gamma"],
  ] as const) {
    desktop = openWindow(desktop, { id, app: "echo", title, w: 300, h: 200 }, { x: 0, y: 0, w: 1024, h: 768 });
  }
  desktop = switchWorkspace(moveToWorkspace(desktop, "a", 2), 0);
  server.push(encodeJsonFrame(channel, FrameType.Control, { type: "value", key: "desktop", value: desktop }));
  await new Promise((resolve) => setTimeout(resolve, 0));
  return host;
}
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const press = (target: Element, key: string) => target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));

describe("layout picker and overview", () => {
  test("Tile opens the picker with a preview for each layout, and Enter applies the selected layout", async () => {
    const host = await mount();
    [...host.querySelectorAll<HTMLButtonElement>(".panel button")].find((b) => b.textContent?.trim() === "Tile")!.click();
    await settle();
    const dialog = host.querySelector('[role="dialog"][aria-label="Layouts"]')!;
    expect([...dialog.querySelectorAll(".wm-layout")].map((card) => card.getAttribute("data-layout"))).toEqual([...LAYOUTS]);
    // The previews show the two windows of the workspace, the focused one first.
    expect([...dialog.querySelectorAll('[data-layout="spotlight"] .wm-layout-title')].map((t) => t.textContent)).toEqual(["Gamma", "Beta"]);
    for (let i = 0; i < 4; i++) {
      press(dialog, "ArrowRight");
      await settle();
    }
    expect(dialog.querySelector('[aria-selected="true"]')?.getAttribute("data-layout")).toBe("spotlight");
    press(dialog, "Enter");
    await settle();
    expect(host.querySelector('[aria-label="Layouts"]')).toBeNull();
    const width = (id: string) => host.querySelector<HTMLElement>(`[data-window="${id}"]`)!.style.width;
    expect(Number.parseInt(width("c"))).toBeGreaterThan(Number.parseInt(width("b")) * 2);
  });

  test("the overview shows each workspace and a letter for each window, and the letter gives a window on a different workspace the focus", async () => {
    const host = await mount();
    window.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyG", key: "G", altKey: true, shiftKey: true, bubbles: true, cancelable: true }));
    await settle();
    const overview = host.querySelector('[role="dialog"][aria-label="Overview"]')!;
    expect(overview.querySelectorAll(".wm-overview-workspace")).toHaveLength(4);
    expect([...overview.querySelectorAll(".wm-overview-window")].map((w) => w.getAttribute("aria-label")).sort()).toEqual(["Alpha, key D", "Beta, key S", "Gamma, key A"]);
    expect(overview.querySelector(".wm-overview-workspace")!.getAttribute("style")).toMatch(/transform: translate\(.*\) scale\(/);
    press(overview, "d");
    await settle();
    expect(host.querySelector('[aria-label="Overview"]')).toBeNull();
    expect(host.querySelector('.panel-workspaces [aria-pressed="true"]')?.textContent).toBe("3");
    expect(host.querySelector('[data-window="a"]')!.className).toContain("wm-focused");
  });
});
