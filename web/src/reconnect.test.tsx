import { render } from "@solidjs/web";
import { afterEach, describe, expect, test, vi } from "vitest";
import { App } from "./App";
import { CONNECTION_CHANNEL, type Connection, encodeJsonFrame, FrameType, parseJson, readyFrame } from "./proto";
import { fakeConnection } from "./proto/testing";
import { reconnectDelay } from "./reconnect";
import { StateClient } from "./state";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

function mount(view: () => unknown): HTMLElement {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(view as () => never, host);
  return host;
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("the wait grows from 1 s to 15 s", () => {
  expect([0, 1, 2, 3, 4, 5, 20].map(reconnectDelay)).toEqual([1000, 2000, 4000, 8000, 15_000, 15_000, 15_000]);
});

describe("App after a break", () => {
  test("a break shows the wait and then connects again to the desktop", async () => {
    const first = await fakeConnection("alice");
    const second = await fakeConnection("alice");
    const queue: Connection[] = [first.connection, second.connection];
    const host = mount(() => <App connect={() => Promise.resolve(queue.shift()!)} />);
    await settle();
    expect(host.textContent).toContain("Logged in as alice");

    vi.useFakeTimers();
    first.server.onclose?.("WebSocket close code 1006");
    await vi.advanceTimersByTimeAsync(0);
    expect(host.querySelector("h1")?.textContent).toBe("No connection to the host");
    expect(host.textContent).toContain("Your session continues on the host.");

    await vi.advanceTimersByTimeAsync(1000);
    expect(host.textContent).toContain("Logged in as alice");
    expect(queue).toHaveLength(0);
  });

  test("Connect now connects at once", async () => {
    const first = await fakeConnection("alice");
    const second = await fakeConnection("alice");
    const queue: Connection[] = [first.connection, second.connection];
    const host = mount(() => <App connect={() => Promise.resolve(queue.shift()!)} />);
    await settle();
    first.server.onclose?.("gone");
    await settle();
    [...host.querySelectorAll("button")].find((b) => b.textContent === "Connect now")!.click();
    await settle();
    expect(host.textContent).toContain("Logged in as alice");
  });

  test("a logout in a different tab shows the login screen with a notice", async () => {
    const first = await fakeConnection("alice");
    const second = await fakeConnection();
    const queue: Connection[] = [first.connection, second.connection];
    const host = mount(() => <App connect={() => Promise.resolve(queue.shift()!)} />);
    await settle();

    first.server.push(encodeJsonFrame(CONNECTION_CHANNEL, FrameType.Control, { type: "logged-out" }));
    first.server.onclose?.("logout");
    await settle();
    expect(host.textContent).toContain("You logged out.");
    expect(host.querySelector("h1")?.textContent).toContain("Log in to");
  });

  test("the Log out button calls the logout", async () => {
    const { connection } = await fakeConnection("alice");
    let calls = 0;
    const host = mount(() => (
      <App
        connect={() => Promise.resolve(connection)}
        logout={() => {
          calls += 1;
          return Promise.resolve();
        }}
      />
    ));
    await settle();
    [...host.querySelectorAll("button")].find((b) => b.textContent === "Log out")!.click();
    expect(calls).toBe(1);
  });
});

describe("StateClient", () => {
  test("watch asks for the key and gets the value and later changes", async () => {
    const { server, connection } = await fakeConnection("alice");
    const client = new StateClient(connection);
    const values: unknown[] = [];
    client.watch("windows", (value) => values.push(value));
    const [open, get] = server.take();
    expect(parseJson(open!)).toEqual({ kind: "state" });
    expect(parseJson(get!)).toEqual({ type: "get", key: "windows" });

    server.push(readyFrame(1));
    server.push(encodeJsonFrame(1, FrameType.Control, { type: "value", key: "windows", value: null }));
    server.push(encodeJsonFrame(1, FrameType.Control, { type: "value", key: "windows", value: [1] }));
    server.push(encodeJsonFrame(1, FrameType.Control, { type: "value", key: "other", value: 2 }));
    expect(values).toEqual([null, [1]]);

    client.set("windows", [1, 2]);
    expect(parseJson(server.take()[0]!)).toEqual({ type: "set", key: "windows", value: [1, 2] });
  });

  test("the echo panel restores its log from the state store", async () => {
    const { server, connection } = await fakeConnection("alice");
    const host = mount(() => <App connect={() => Promise.resolve(connection)} />);
    await settle();
    // The desktop asks for its state first. With no state, it opens an echo window.
    const desktopChannel = server
      .take()
      .find((frame) => frame.type === FrameType.Open && parseJson<{ kind: string }>(frame).kind === "state")!.channel;
    server.push(encodeJsonFrame(desktopChannel, FrameType.Control, { type: "value", key: "desktop", value: null }));
    await settle();
    const frames = server.take();
    const stateChannel = frames.find((frame) => frame.type === FrameType.Open && parseJson<{ kind: string }>(frame).kind === "state")!.channel;
    const saved = [
      { id: 0, text: "hello", kind: "sent" },
      { id: 1, text: "hello (1.0 ms)", kind: "echo" },
    ];
    const get = frames.find((frame) => frame.channel === stateChannel && frame.type === FrameType.Control)!;
    const { key } = parseJson<{ key: string }>(get);
    expect(key).toMatch(/^echo\.log\.[0-9a-f-]+$/);
    server.push(encodeJsonFrame(stateChannel, FrameType.Control, { type: "value", key, value: saved }));
    await settle();
    expect([...host.querySelectorAll(".echo-log li")].map((li) => li.textContent)).toEqual(["hello", "hello (1.0 ms)"]);
  });
});
