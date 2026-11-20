import { render } from "@solidjs/web";
import { afterEach, describe, expect, test } from "vitest";
import { encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { Desktop } from "../wm/Desktop";
import { type Desktop as DesktopState, emptyDesktop, openWindow } from "../wm/model";
import { ContextMenuHost } from "./ContextMenuHost";
import { type MenuItem, nextEnabled, placeMenu, providerFor, setContextMenu, textField, textFieldMenu } from "./menu";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const noop = () => {};

describe("menu helpers", () => {
  test("the nearest provider wins, and an empty provider blocks the outer menu", () => {
    document.body.innerHTML = "<div id=outer><div id=middle><span id=inner></span></div></div>";
    const outer = document.getElementById("outer")!;
    const middle = document.getElementById("middle")!;
    const inner = document.getElementById("inner")!;
    const outerMenu = () => [{ label: "Outer", action: noop }];
    setContextMenu(outer, outerMenu);
    expect(providerFor(inner)).toBe(outerMenu);
    const remove = setContextMenu(middle, () => []);
    expect(providerFor(inner)!()).toEqual([]);
    remove();
    expect(providerFor(inner)).toBe(outerMenu);
    expect(providerFor(document.body)).toBeNull();
  });

  test("text fields get the text menu, and the terminal input and buttons do not", () => {
    document.body.innerHTML = `<input id=a><textarea id=b></textarea><input id=c type=checkbox>
      <textarea id=d class="xterm-helper-textarea"></textarea><p id=e></p><input id=f type=password>`;
    const isField = (id: string) => textField(document.getElementById(id)) !== null;
    expect(["a", "b", "c", "d", "e", "f"].map(isField)).toEqual([true, true, false, false, false, true]);
  });

  test("a password field gives no cut and no copy, a read-only field no cut and no paste", () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: () => Promise.resolve(), readText: () => Promise.resolve("") },
    });
    document.body.innerHTML = "<input id=p type=password value=secret><input id=r readonly value=fixed>";
    const enabled = (id: string) => {
      const field = textField(document.getElementById(id))!;
      field.select();
      return textFieldMenu(field)
        .filter((item): item is Extract<MenuItem, { label: string }> => "label" in item && !item.disabled)
        .map((item) => item.label);
    };
    expect(enabled("p")).toEqual(["Paste", "Select all"]);
    expect(enabled("r")).toEqual(["Copy", "Select all"]);
  });

  test("cut, copy, paste, and select all work on the field and send an input event", async () => {
    let clipboard = "";
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (text: string) => ((clipboard = text), Promise.resolve()), readText: () => Promise.resolve(clipboard) },
    });
    document.body.innerHTML = "<input id=t value='hello world'>";
    const field = textField(document.getElementById("t"))!;
    const inputs: string[] = [];
    field.addEventListener("input", () => inputs.push(field.value));
    const run = (label: string) => {
      const item = textFieldMenu(field).find((entry) => "label" in entry && entry.label === label);
      (item as Extract<MenuItem, { label: string }>).action();
    };
    field.setSelectionRange(0, 5);
    run("Copy");
    expect(clipboard).toBe("hello");
    field.setSelectionRange(5, 11);
    run("Cut");
    expect(clipboard).toBe(" world");
    expect(field.value).toBe("hello");
    field.setSelectionRange(5, 5);
    run("Paste");
    await settle();
    expect(field.value).toBe("hello world");
    expect(inputs).toEqual(["hello", "hello world"]);
    run("Select all");
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, 11]);
  });

  test("keyboard navigation skips separators and disabled items and wraps", () => {
    const items: MenuItem[] = [
      { label: "A", action: noop },
      { separator: true },
      { label: "B", action: noop, disabled: true },
      { label: "C", action: noop },
    ];
    expect(nextEnabled(items, -1, 1)).toBe(0);
    expect(nextEnabled(items, 0, 1)).toBe(3);
    expect(nextEnabled(items, 3, 1)).toBe(0);
    expect(nextEnabled(items, 0, -1)).toBe(3);
    expect(nextEnabled([{ separator: true }], -1, 1)).toBe(-1);
  });

  test("a menu at the edge of the viewport moves inside", () => {
    const viewport = { width: 1000, height: 700 };
    expect(placeMenu(100, 100, 200, 150, viewport)).toEqual({ x: 100, y: 100 });
    expect(placeMenu(900, 100, 200, 150, viewport)).toEqual({ x: 700, y: 100 });
    expect(placeMenu(100, 650, 200, 150, viewport)).toEqual({ x: 100, y: 546 });
  });
});

const AREA = { x: 0, y: 0, w: 1024, h: 768 };

async function mountDesktop(value: DesktopState) {
  const { server, connection } = await fakeConnection("alice");
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(
    () => (
      <>
        <Desktop connection={connection} user="alice" onlogout={noop} />
        <ContextMenuHost />
      </>
    ),
    host,
  );
  const channel = server.take().find((f) => f.type === FrameType.Open && parseJson<{ kind: string }>(f).kind === "state")!.channel;
  server.push(readyFrame(channel));
  server.push(encodeJsonFrame(channel, FrameType.Control, { type: "value", key: "desktop", value }));
  await settle();
  return host;
}

function oneWindow(): DesktopState {
  return openWindow(emptyDesktop(), { id: "a", app: "echo", title: "Alpha", w: 400, h: 300 }, AREA);
}

/** Sends a right click to `target`. Returns `true` if the shell blocked the browser menu. */
function rightClick(target: Element, init: MouseEventInit = {}): boolean {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 60, ...init });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

const items = (host: HTMLElement) =>
  [...host.querySelectorAll<HTMLElement>(".context-menu [role=menuitem]")].map((item) => item.firstElementChild?.textContent);
const choose = (host: HTMLElement, label: string) =>
  [...host.querySelectorAll<HTMLElement>(".context-menu [role=menuitem]")].find((item) => item.textContent?.startsWith(label))!.click();

describe("context menus of the shell", () => {
  test("the title bar gives the menu of the window, and an item runs its action", async () => {
    const host = await mountDesktop(oneWindow());
    expect(rightClick(host.querySelector('[data-window="a"] .wm-titlebar')!)).toBe(true);
    await settle();
    expect(items(host)).toEqual([
      "Minimize",
      "Maximize",
      "Snap to the left half",
      "Snap to the right half",
      "Move to workspace 2",
      "Move to workspace 3",
      "Move to workspace 4",
      "Close",
    ]);
    choose(host, "Snap to the left half");
    await settle();
    expect(host.querySelector<HTMLElement>('[data-window="a"]')!.dataset.mode).toBe("left");
    expect(items(host)).toEqual([]);
  });

  test("the empty desktop gives a menu that opens apps and tiles the windows", async () => {
    const host = await mountDesktop(emptyDesktop());
    expect(rightClick(host.querySelector(".wm-surface")!)).toBe(true);
    await settle();
    expect(items(host)).toEqual(["Open About farglass", "Open Echo", "Open Settings", "Open Terminal", "Tile the windows"]);
    choose(host, "Open Echo");
    await settle();
    expect(host.querySelector(".wm-window")?.getAttribute("aria-label")).toBe("Echo");
  });

  test("the window button of the panel gives the menu of the window", async () => {
    const host = await mountDesktop(oneWindow());
    rightClick(host.querySelector(".wm-task")!);
    await settle();
    choose(host, "Move to workspace 3");
    await settle();
    expect(host.querySelector<HTMLElement>('[data-window="a"]')!.hidden).toBe(false);
    expect(host.querySelector('[aria-label="Workspace 3"]')!.getAttribute("aria-pressed")).toBe("true");
  });

  test("an app with no menu gives no menu, and the browser menu never shows", async () => {
    const host = await mountDesktop(oneWindow());
    const content = host.querySelector('[data-window="a"] .wm-content')!;
    expect(rightClick(content)).toBe(true);
    await settle();
    expect(items(host)).toEqual([]);
    // Shift does not give the menu of the browser.
    expect(rightClick(host.querySelector(".wm-surface")!, { shiftKey: true })).toBe(true);
    await settle();
    expect(items(host)).toContain("Tile the windows");
  });

  test("a text field gets the text menu of the shell", async () => {
    const host = await mountDesktop(oneWindow());
    const input = document.createElement("input");
    input.value = "abc";
    host.querySelector('[data-window="a"] .wm-content')!.append(input);
    expect(rightClick(input)).toBe(true);
    await settle();
    expect(items(host)).toEqual(["Cut", "Copy", "Paste", "Select all"]);
  });

  test("Shift+F10 opens the menu of the focused element, and the arrow keys and Enter choose", async () => {
    const host = await mountDesktop(oneWindow());
    const task = host.querySelector<HTMLButtonElement>(".wm-task")!;
    task.focus();
    let reached = false;
    task.addEventListener("keydown", () => (reached = true));
    task.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", shiftKey: true, bubbles: true, cancelable: true }));
    expect(reached).toBe(false);
    await settle();
    await settle();
    const menu = host.querySelector<HTMLElement>(".context-menu")!;
    expect(menu.querySelector("[data-active=true]")?.textContent).toBe("Minimize");
    const press = (key: string) => menu.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    press("End");
    await settle();
    expect(menu.querySelector("[data-active=true]")?.textContent).toBe("CloseAlt+Shift+W");
    press("Enter");
    await settle();
    expect(host.querySelector('[data-window="a"]')).toBeNull();
  });
});
