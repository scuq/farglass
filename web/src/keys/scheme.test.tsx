import { render } from "@solidjs/web";
import { afterEach, describe, expect, test, vi } from "vitest";
import { encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { Desktop } from "../wm/Desktop";
import { emptyDesktop, openWindow } from "../wm/model";
import { type ActionId, actionOf, type Binding, chordLabel, hintOf, RESERVED, SCHEMES, type SchemeId, parseKeyboardSettings, sameChord, withLayout } from "./scheme";

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

const schemes: SchemeId[] = ["pc", "mac"];
const entries = (scheme: SchemeId) => Object.entries(SCHEMES[scheme]) as [ActionId, readonly Binding[]][];

describe("schemes", () => {
  test("no binding takes a key of a browser, an input method, a screen reader, AltGr, a character, or a common app shortcut", () => {
    for (const scheme of schemes) {
      for (const [action, bindings] of entries(scheme)) {
        for (const binding of bindings) {
          const hit = RESERVED[scheme].find((reserved) => sameChord(reserved.chord, binding.chord));
          expect(hit, `${scheme} ${action} ${chordLabel(binding.chord, scheme)}: ${hit?.reason}`).toBeUndefined();
        }
      }
    }
  });

  test("no two actions share a key in a scheme", () => {
    for (const scheme of schemes) {
      const all = entries(scheme).flatMap(([action, bindings]) => bindings.map((binding) => ({ action, binding })));
      all.forEach((a, i) =>
        all.slice(i + 1).forEach((b) => expect(sameChord(a.binding.chord, b.binding.chord), `${scheme} ${a.action} ${b.action}`).toBe(false)),
      );
    }
  });

  test("an action whose key needs Keyboard Lock has a second key that reaches the browser", () => {
    for (const scheme of schemes) {
      for (const [action, bindings] of entries(scheme)) {
        // Tile has the layout picker, whose first layout is the columns of Tile.
        if (action === "tile") continue;
        expect(bindings.some((binding) => !binding.lock), `${scheme} ${action}`).toBe(true);
      }
    }
  });

  test("labels follow the convention of the scheme, and the hint is the key that needs no lock", () => {
    expect(hintOf("maximize", "pc")).toBe("Alt+Shift+M");
    expect(hintOf("maximize", "mac")).toBe("⌃⇧M");
    expect(hintOf("switch", "pc")).toBe("Alt+`");
    expect(hintOf("switch", "mac")).toBe("⌥Tab");
    expect(hintOf("new-terminal", "mac")).toBe("⌃⇧T");
  });

  test("Cmd+W closes in a window of the shell with Keyboard Lock, and stays with a terminal or an app", () => {
    const cmdW = { code: "KeyW", ctrlKey: false, altKey: false, metaKey: true, shiftKey: false };
    expect(actionOf(cmdW, "mac", true)).toBe("close-window");
    expect(actionOf(cmdW, "mac", false)).toBeNull();
    expect(actionOf({ ...cmdW, metaKey: false, ctrlKey: true, shiftKey: true }, "mac", false)).toBe("close-window");
  });
});

async function mountDesktop() {
  const { server, connection } = await fakeConnection("alice");
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <Desktop connection={connection} user="alice" onlogout={() => {}} />, host);
  const channel = server.take().find((f) => f.type === FrameType.Open && parseJson<{ kind: string }>(f).kind === "state")!.channel;
  server.push(readyFrame(channel));
  const desktop = openWindow(emptyDesktop(), { id: "a", app: "echo", title: "Alpha", w: 300, h: 200 }, { x: 0, y: 0, w: 1024, h: 768 });
  server.push(encodeJsonFrame(channel, FrameType.Control, { type: "value", key: "desktop", value: desktop }));
  server.push(encodeJsonFrame(channel, FrameType.Control, { type: "value", key: "keyboard", value: { scheme: "pc", hints: true } }));
  await new Promise((resolve) => setTimeout(resolve, 0));
  return host;
}
const key = (type: "keydown" | "keyup", init: KeyboardEventInit) => window.dispatchEvent(new KeyboardEvent(type, { bubbles: true, cancelable: true, ...init }));

describe("hints and tour", () => {
  test("holding Alt+Shift shows the hints, and the release hides them", async () => {
    const host = await mountDesktop();
    vi.useFakeTimers();
    key("keydown", { key: "Alt", code: "AltLeft", altKey: true });
    key("keydown", { key: "Shift", code: "ShiftLeft", altKey: true, shiftKey: true });
    vi.advanceTimersByTime(800);
    await vi.runAllTicks();
    vi.useRealTimers();
    await new Promise((resolve) => setTimeout(resolve, 0));
    const hints = host.querySelector('[aria-label="Shortcuts"]');
    expect(hints?.textContent).toContain("Alt+Shift+M");
    expect(hints?.textContent).toContain("Maximize or restore");
    key("keyup", { key: "Shift", code: "ShiftLeft", altKey: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(host.querySelector('[aria-label="Shortcuts"]')).toBeNull();
  });

  test("the tour shows the key of each step and goes on when the user presses it", async () => {
    const host = await mountDesktop();
    window.dispatchEvent(new Event("farglass-tour"));
    await new Promise((resolve) => setTimeout(resolve, 0));
    const tour = () => host.querySelector('[aria-label="Keyboard tour"]')!;
    expect(tour().textContent).toContain("Step 1 of 10");
    expect(tour().querySelector("kbd")?.textContent).toBe("Alt+Shift+Space");
    key("keydown", { key: " ", code: "Space", altKey: true, shiftKey: true });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(tour().textContent).toContain("Done.");
    await new Promise((resolve) => setTimeout(resolve, 950));
    expect(tour().textContent).toContain("Step 2 of 10");
    expect(tour().querySelector("kbd")?.textContent).toBe("Alt+Shift+T");
    [...tour().querySelectorAll("button")].find((b) => b.textContent === "End the tour")!.click();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(host.querySelector('[aria-label="Keyboard tour"]')).toBeNull();
  });
});

test("a selected layout goes to the front of the recent layouts, four at most (F097)", () => {
  let settings = parseKeyboardSettings({ layout: "auto", recent: ["de", 7, "de", "bad layout"] });
  expect(settings.recent).toEqual(["de"]);
  for (const layout of ["at", "ch(fr)", "us", "gb", "at"]) settings = withLayout(settings, layout);
  expect(settings.layout).toBe("at");
  expect(settings.recent).toEqual(["at", "gb", "us", "ch(fr)"]);
  settings = withLayout(settings, "auto");
  expect(settings.layout).toBe("auto");
  expect(settings.recent).toEqual(["at", "gb", "us", "ch(fr)"]);
});
