import { render } from "@solidjs/web";
import { afterEach, describe, expect, test, vi } from "vitest";
import { encodeJsonFrame, FrameType, type Notification, type NotifyEvent, parseJson, readyFrame } from "../proto";
import { type FakeServer, fakeConnection } from "../proto/testing";
import { MAX_SHOWN, Notifications } from "./Notifications";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

const settle = () => vi.advanceTimersByTimeAsync(0);

async function mount(): Promise<{ host: HTMLElement; server: FakeServer }> {
  const { server, connection } = await fakeConnection("alice");
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <Notifications connection={connection} />, host);
  const [open] = server.take();
  expect(parseJson(open!)).toEqual({ kind: "notify" });
  server.push(readyFrame(1));
  return { host, server };
}

const push = (server: FakeServer, event: NotifyEvent) => server.push(encodeJsonFrame(1, FrameType.Control, event));
const replies = (server: FakeServer) => server.take().map((frame) => parseJson<Record<string, unknown>>(frame));
const note = (id: number, more: Partial<Notification> = {}): Notification => ({
  id,
  app: "mail",
  summary: `Mail ${id}`,
  body: "From Alice",
  actions: [],
  urgency: "normal",
  timeout_ms: 6000,
  ...more,
});
/** A notification with no end, as the agent sends it: with no timeout_ms. */
const critical = (id: number): Notification => {
  const { timeout_ms: _, ...rest } = note(id, { urgency: "critical" });
  return rest;
};
const cards = (host: HTMLElement) => [...host.querySelectorAll<HTMLElement>(".notification")];

describe("Notifications", () => {
  test("a notification shows, and its time ends with an expire reply", async () => {
    const { host, server } = await mount();
    push(server, { type: "show", notification: note(1) });
    await settle();
    expect(cards(host).map((card) => card.getAttribute("aria-label"))).toEqual(["Mail 1"]);
    expect(cards(host)[0]!.getAttribute("role")).toBe("status");
    await vi.advanceTimersByTimeAsync(5999);
    expect(replies(server)).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(replies(server)).toEqual([{ type: "expire", id: 1 }]);
    expect(cards(host)).toEqual([]);
  });

  test("the focus pauses the time, and the time continues after", async () => {
    const { host, server } = await mount();
    push(server, { type: "show", notification: note(1) });
    await settle();
    await vi.advanceTimersByTimeAsync(4000);
    // jsdom has no :hover. The focus takes the same path, and the e2e test checks the pointer.
    host.querySelector<HTMLButtonElement>(".notification-close")!.focus();
    await vi.advanceTimersByTimeAsync(20000);
    expect(cards(host)).toHaveLength(1);
    host.querySelector<HTMLButtonElement>(".notification-close")!.blur();
    await vi.advanceTimersByTimeAsync(1999);
    expect(cards(host)).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(cards(host)).toEqual([]);
  });

  test("a critical notification stays, is an alert, and the close button dismisses it", async () => {
    const { host, server } = await mount();
    push(server, { type: "show", notification: critical(1) });
    await settle();
    await vi.advanceTimersByTimeAsync(60000);
    const [card] = cards(host);
    expect(card!.getAttribute("role")).toBe("alert");
    card!.querySelector<HTMLButtonElement>(".notification-close")!.click();
    await settle();
    expect(replies(server)).toEqual([{ type: "dismiss", id: 1 }]);
    expect(cards(host)).toEqual([]);
  });

  test("a click invokes the default action, and the buttons invoke the others", async () => {
    const { host, server } = await mount();
    const actions = [
      { key: "default", label: "Open" },
      { key: "later", label: "Later" },
    ];
    push(server, { type: "show", notification: note(1, { actions }) });
    await settle();
    const [card] = cards(host);
    const buttons = [...card!.querySelectorAll<HTMLButtonElement>(".notification-actions button")];
    expect(buttons.map((button) => button.textContent)).toEqual(["Later"]);
    buttons[0]!.click();
    card!.querySelector<HTMLButtonElement>("button.notification-text")!.click();
    expect(replies(server)).toEqual([
      { type: "action", id: 1, key: "later" },
      { type: "action", id: 1, key: "default" },
    ]);
    push(server, { type: "close", id: 1 });
    await settle();
    expect(cards(host)).toEqual([]);
  });

  test("a replacement keeps the place and starts the time again", async () => {
    const { host, server } = await mount();
    push(server, { type: "show", notification: note(1) });
    push(server, { type: "show", notification: note(2) });
    await settle();
    await vi.advanceTimersByTimeAsync(5000);
    push(server, { type: "show", notification: note(1, { summary: "Download 90%" }) });
    await settle();
    expect(cards(host).map((card) => card.getAttribute("aria-label"))).toEqual(["Download 90%", "Mail 2"]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(cards(host).map((card) => card.getAttribute("aria-label"))).toEqual(["Download 90%"]);
  });

  test("more notifications than the screen holds wait, and their time starts on the screen", async () => {
    const { host, server } = await mount();
    for (let id = 1; id <= MAX_SHOWN + 1; id++) {
      push(server, { type: "show", notification: note(id) });
    }
    await settle();
    expect(cards(host)).toHaveLength(MAX_SHOWN);
    expect(host.querySelector(".notification-more")?.textContent).toBe("1 more");
    await vi.advanceTimersByTimeAsync(6000);
    expect(cards(host).map((card) => card.getAttribute("aria-label"))).toEqual([`Mail ${MAX_SHOWN + 1}`]);
    await vi.advanceTimersByTimeAsync(5999);
    expect(cards(host)).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(cards(host)).toEqual([]);
  });
});
