import { render } from "@solidjs/web";
import { afterEach, expect, test } from "vitest";
import { App } from "./App";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
});

test("the shell shows that it connects to the host", () => {
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App connect={() => new Promise(() => {})} />, host);
  expect(host.querySelector("[role=status]")?.textContent).toBe("Connecting to the host");
});

import { encodeJsonFrame, FrameType, parseJson, readyFrame } from "./proto";
import { CONNECTION_CHANNEL } from "./proto/generated/wire";
import { type FakeServer, fakeConnection } from "./proto/testing";

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Returns a connect function that gives the next connection of `connections` for each call. */
function connections(...list: Array<() => ReturnType<typeof fakeConnection>>) {
  const servers: FakeServer[] = [];
  let calls = 0;
  const connect = async () => {
    const make = list[calls++];
    if (!make) {
      return new Promise<never>(() => {});
    }
    const { server, connection } = await make();
    servers.push(server);
    return connection;
  };
  return { connect, servers, calls: () => calls };
}

/** Answers the login channel of the lock screen: the password prompt, then the result. */
async function unlock(server: FakeServer, password: string, host: HTMLElement) {
  const open = server.take().find((frame) => frame.type === FrameType.Open)!;
  expect(parseJson(open)).toEqual({ kind: "login" });
  server.push(readyFrame(open.channel));
  server.push(encodeJsonFrame(open.channel, FrameType.Control, { type: "prompt", prompt: 1, text: "Password: ", echo: false }));
  await settle();
  const input = host.querySelector<HTMLInputElement>("#login-answer")!;
  input.value = password;
  host.querySelector("form")!.dispatchEvent(new SubmitEvent("submit", { bubbles: true, cancelable: true }));
  await settle();
  const answer = server.take().map((frame) => parseJson<Record<string, unknown>>(frame));
  expect(answer).toEqual([{ type: "answer", prompt: 1, text: password }]);
  server.push(encodeJsonFrame(open.channel, FrameType.Control, { type: "success", user: "alice", uid: 1000 }));
  await settle();
}

test("Lock in the session menu gives the lock screen, and the password gives the desktop back", async () => {
  const fake = connections(
    () => fakeConnection("alice"),
    () => fakeConnection(undefined, "alice"),
  );
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App connect={fake.connect} storeSession={async () => true} />, host);
  await settle();
  const [desktop] = fake.servers;
  host.querySelector<HTMLButtonElement>(".panel-session")!.click();
  await settle();
  [...host.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent === "Lock")!.click();
  const sent = desktop!.take().filter((frame) => frame.channel === CONNECTION_CHANNEL);
  expect(sent.map((frame) => parseJson(frame))).toEqual([{ type: "lock" }]);

  // farglassd locks the session, sends the locked message, and closes the connection.
  desktop!.push(encodeJsonFrame(CONNECTION_CHANNEL, FrameType.Control, { type: "locked" }));
  desktop!.onclose?.("closed");
  await settle();
  await settle();
  expect(fake.calls()).toBe(2);
  expect(host.querySelector(".wm")).toBeNull();
  expect(host.querySelector("h1")?.textContent).toBe("Locked");
  expect(host.querySelector(".login-locked")?.textContent).toContain("alice");

  await unlock(fake.servers[1]!, "secret", host);
  expect(host.querySelector(".wm")).not.toBeNull();
});

test("Log in as a different user forgets the cookie and gives the login screen", async () => {
  let logouts = 0;
  const fake = connections(
    () => fakeConnection(undefined, "alice"),
    () => fakeConnection(),
  );
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <App connect={fake.connect} logout={async () => void (logouts += 1)} />, host);
  await settle();
  host.querySelector<HTMLButtonElement>(".login-switch")!.click();
  await settle();
  await settle();
  expect(logouts).toBe(1);
  expect(fake.servers[0]!.closed).toBe(true);
  expect(fake.calls()).toBe(2);
  expect(host.querySelector("h1")?.textContent).toContain("Log in to");
});
