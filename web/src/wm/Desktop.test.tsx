import { render } from "@solidjs/web";
import { afterEach, describe, expect, test } from "vitest";
import { encodeJsonFrame, type Frame, FrameType, parseJson, readyFrame } from "../proto";
import { type FakeServer, fakeConnection } from "../proto/testing";
import { Desktop } from "./Desktop";
import { type Desktop as DesktopState, emptyDesktop, openWindow } from "./model";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

const settle = (ms = 0) => new Promise((resolve) => setTimeout(resolve, ms));
const AREA = { x: 0, y: 0, w: 1024, h: 768 };

function twoWindows(): DesktopState {
  let state = emptyDesktop();
  state = openWindow(state, { id: "a", app: "echo", title: "Alpha", w: 400, h: 300 }, AREA);
  state = openWindow(state, { id: "b", app: "echo", title: "Beta", w: 400, h: 300 }, AREA);
  return state;
}

function stateChannelOf(frames: Frame[]): number {
  return frames.find((frame) => frame.type === FrameType.Open && parseJson<{ kind: string }>(frame).kind === "state")!.channel;
}

/** Mounts the desktop and answers its state request with `value`. */
async function mountDesktop(value: unknown): Promise<{ host: HTMLElement; server: FakeServer; channel: number }> {
  const { server, connection } = await fakeConnection("alice");
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <Desktop connection={connection} />, host);
  const channel = stateChannelOf(server.take());
  server.push(readyFrame(channel));
  server.push(encodeJsonFrame(channel, FrameType.Control, { type: "value", key: "desktop", value }));
  await settle();
  return { host, server, channel };
}

const win = (host: HTMLElement, id: string) => host.querySelector<HTMLElement>(`[data-window="${id}"]`)!;
const button = (root: Element, label: string) => root.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;

function key(type: "keydown" | "keyup", init: KeyboardEventInit) {
  window.dispatchEvent(new KeyboardEvent(type, { bubbles: true, cancelable: true, ...init }));
}

describe("Desktop", () => {
  test("the windows come back from the state store with their title and stack order", async () => {
    const { host } = await mountDesktop(twoWindows());
    expect(win(host, "a").getAttribute("aria-label")).toBe("Alpha");
    expect(win(host, "b").className).toContain("wm-focused");
    expect(Number(win(host, "b").style.zIndex)).toBeGreaterThan(Number(win(host, "a").style.zIndex));
    expect(win(host, "a").style.transform).toMatch(/^translate3d\(/);
  });

  test("a new session opens one echo window", async () => {
    const { host } = await mountDesktop(null);
    expect(host.querySelectorAll(".wm-window")).toHaveLength(1);
    expect(host.querySelector(".wm-window")?.getAttribute("aria-label")).toBe("Echo");
  });

  test("the title bar buttons maximize, restore, minimize, and close", async () => {
    const { host } = await mountDesktop(twoWindows());
    button(win(host, "a"), "Maximize").click();
    await settle();
    expect(win(host, "a").dataset.mode).toBe("maximized");
    expect(win(host, "a").className).toContain("wm-focused");
    button(win(host, "a"), "Restore").click();
    await settle();
    expect(win(host, "a").dataset.mode).toBe("normal");

    button(win(host, "a"), "Minimize").click();
    await settle();
    expect(win(host, "a").hidden).toBe(true);
    expect(win(host, "b").className).toContain("wm-focused");
    const task = [...host.querySelectorAll<HTMLButtonElement>(".wm-task")].find((b) => b.textContent === "Alpha")!;
    expect(task.dataset.minimized).toBe("true");
    task.click();
    await settle();
    expect(win(host, "a").hidden).toBe(false);

    button(win(host, "b"), "Close").click();
    await settle();
    expect(win(host, "b")).toBeNull();
  });

  test("Alt+` opens the switcher on the last used window, and the release of Alt focuses it", async () => {
    const { host } = await mountDesktop(twoWindows());
    key("keydown", { key: "Alt", code: "AltLeft", altKey: true });
    key("keydown", { key: "`", code: "Backquote", altKey: true });
    await settle();
    const selected = host.querySelector(".wm-switcher [aria-selected='true']");
    expect(selected?.textContent).toBe("Alpha");
    key("keyup", { key: "Alt", code: "AltLeft" });
    await settle();
    expect(host.querySelector(".wm-switcher")).toBeNull();
    expect(win(host, "a").className).toContain("wm-focused");
  });

  test("workspace buttons show only the windows of that workspace", async () => {
    const { host } = await mountDesktop(twoWindows());
    button(host, "Workspace 2").click();
    await settle();
    expect(win(host, "a").hidden).toBe(true);
    expect(win(host, "b").hidden).toBe(true);
    expect(button(host, "Workspace 2").getAttribute("aria-pressed")).toBe("true");
  });

  test("a change goes to the state store after a short wait", async () => {
    const { host, server, channel } = await mountDesktop(twoWindows());
    server.take();
    button(win(host, "a"), "Maximize").click();
    await settle(300);
    const sets = server
      .take()
      .filter((frame) => frame.channel === channel)
      .map((frame) => parseJson<{ type: string; key: string; value: DesktopState }>(frame));
    expect(sets).toHaveLength(1);
    expect(sets[0]!.key).toBe("desktop");
    expect(sets[0]!.value.windows.find((w) => w.id === "a")!.snap).toBe("maximized");
  });
});

describe("Desktop and its apps", () => {
  test("a change of a window does not start its app again", async () => {
    const { host, server } = await mountDesktop(twoWindows());
    const echoOpens = () =>
      server.received.filter((frame) => frame.type === FrameType.Open && parseJson<{ kind: string }>(frame).kind === "echo").length;
    const before = echoOpens();
    expect(before).toBe(2);
    button(win(host, "a"), "Maximize").click();
    await settle();
    button(win(host, "b"), "Minimize").click();
    await settle();
    button(host, "Workspace 2").click();
    await settle();
    expect(echoOpens()).toBe(before);
  });

  test("a second window of an app gets a number in its title", async () => {
    const { host } = await mountDesktop(null);
    [...host.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "New Echo")!.click();
    await settle();
    expect([...host.querySelectorAll(".wm-window")].map((w) => w.getAttribute("aria-label"))).toEqual(["Echo", "Echo 2"]);
  });
});

test("two echo windows have different element ids and different state keys", async () => {
  const { host, server } = await mountDesktop(twoWindows());
  const inputs = [...host.querySelectorAll<HTMLInputElement>(".echo input")];
  expect(inputs).toHaveLength(2);
  expect(inputs[0]!.id).not.toBe(inputs[1]!.id);
  const keys = server.received
    .filter((frame) => frame.type === FrameType.Control)
    .map((frame) => parseJson<{ type?: string; key?: string }>(frame))
    .filter((message) => message.type === "get" && message.key?.startsWith("echo.log."))
    .map((message) => message.key);
  expect(keys.sort()).toEqual(["echo.log.a", "echo.log.b"]);
});
