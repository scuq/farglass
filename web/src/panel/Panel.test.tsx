import { render } from "@solidjs/web";
import { afterEach, describe, expect, test, vi } from "vitest";
import { encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { Desktop } from "../wm/Desktop";
import { emptyDesktop } from "../wm/model";
import { Clock } from "./Clock";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function mountDesktop(onlogout = () => {}) {
  const { server, connection } = await fakeConnection("alice");
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <Desktop connection={connection} user="alice" onlogout={onlogout} />, host);
  const channel = server.take().find((f) => f.type === FrameType.Open && parseJson<{ kind: string }>(f).kind === "state")!.channel;
  server.push(readyFrame(channel));
  // An empty desktop that is not new: the desktop opens no window.
  server.push(encodeJsonFrame(channel, FrameType.Control, { type: "value", key: "desktop", value: emptyDesktop() }));
  await settle();
  return { host, server };
}

const byText = (host: HTMLElement, selector: string, text: string) =>
  [...host.querySelectorAll<HTMLElement>(selector)].find((element) => element.textContent?.includes(text))!;

function type(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function key(type: "keydown" | "keyup", init: KeyboardEventInit, target: EventTarget = window) {
  target.dispatchEvent(new KeyboardEvent(type, { bubbles: true, cancelable: true, ...init }));
}

describe("launcher", () => {
  test("the Apps button opens the launcher with all apps, and a click opens an app", async () => {
    const { host } = await mountDesktop();
    expect(host.querySelector("#launcher [role=option]")).toBeNull();
    byText(host, "button", "Apps").click();
    await settle();
    const options = [...host.querySelectorAll("#launcher [role=option]")].map((li) => li.querySelector(".launcher-title")?.textContent);
    expect(options).toEqual(["About farglass", "Echo"]);
    byText(host, "#launcher [role=option]", "About farglass").click();
    await settle();
    expect(host.querySelector(".wm-window")?.getAttribute("aria-label")).toBe("About farglass");
    expect(host.querySelector("#launcher [role=option]")).toBeNull();
  });

  test("the search filters the apps, and Enter opens the selected app", async () => {
    const { host } = await mountDesktop();
    byText(host, "button", "Apps").click();
    await settle();
    const input = host.querySelector<HTMLInputElement>(".launcher-search")!;
    type(input, "ping");
    await settle();
    expect([...host.querySelectorAll("#launcher [role=option]")].map((li) => li.querySelector(".launcher-title")?.textContent)).toEqual(["Echo"]);
    type(input, "nothing here");
    await settle();
    expect(host.querySelector(".launcher-empty")?.textContent).toBe("No app matches.");
    type(input, "");
    await settle();
    key("keydown", { key: "ArrowDown" }, input);
    await settle();
    expect(host.querySelector("#launcher [aria-selected=true] .launcher-title")?.textContent).toBe("Echo");
    key("keydown", { key: "Enter" }, input);
    await settle();
    expect(host.querySelector(".wm-window")?.getAttribute("aria-label")).toBe("Echo");
  });

  test("Alt+F1 and a tap of Super open and close the launcher", async () => {
    const { host } = await mountDesktop();
    const open = () => host.querySelector("#launcher .launcher-search") !== null;
    key("keydown", { key: "F1", code: "F1", altKey: true });
    await settle();
    expect(open()).toBe(true);
    key("keydown", { key: "F1", code: "F1", altKey: true });
    await settle();
    expect(open()).toBe(false);

    key("keydown", { key: "Meta", code: "MetaLeft", metaKey: true });
    key("keyup", { key: "Meta", code: "MetaLeft" });
    await settle();
    expect(open()).toBe(true);
    // Super with a second key is a shortcut, not a tap.
    key("keydown", { key: "Meta", code: "MetaLeft", metaKey: true });
    key("keydown", { key: "ArrowUp", code: "ArrowUp", metaKey: true });
    key("keyup", { key: "Meta", code: "MetaLeft" });
    await settle();
    expect(open()).toBe(true);
  });
});

describe("session menu", () => {
  test("the menu has Lock, which is not available yet, and Log out", async () => {
    let logouts = 0;
    const { host } = await mountDesktop(() => (logouts += 1));
    const session = byText(host, ".panel-session", "alice");
    expect(session.textContent).toBe("Logged in as alice");
    session.click();
    await settle();
    const lock = byText(host, "[role=menuitem]", "Lock");
    expect(lock.getAttribute("aria-disabled")).toBe("true");
    lock.click();
    expect(logouts).toBe(0);
    byText(host, "[role=menuitem]", "Log out").click();
    expect(logouts).toBe(1);
  });
});

describe("clock", () => {
  test("the clock shows the time and changes at the next minute", async () => {
    vi.useFakeTimers();
    let now = new Date(2026, 10, 19, 13, 59, 30);
    const host = document.createElement("div");
    document.body.append(host);
    dispose = render(() => <Clock now={() => now} />, host);
    const time = host.querySelector("time")!;
    const before = time.textContent;
    expect(before).toMatch(/13.?59|1:59/);
    now = new Date(2026, 10, 19, 14, 0, 1);
    await vi.advanceTimersByTimeAsync(31_000);
    expect(time.textContent).not.toBe(before);
    expect(time.getAttribute("datetime")).toBe(now.toISOString());
  });
});
