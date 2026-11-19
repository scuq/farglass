import { render } from "@solidjs/web";
import { afterEach, describe, expect, test } from "vitest";
import { encodeJsonFrame, FrameType, type PolkitEvent, parseJson, readyFrame } from "../proto";
import { type FakeServer, fakeConnection } from "../proto/testing";
import { PolkitDialog } from "./PolkitDialog";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function mount(): Promise<{ host: HTMLElement; server: FakeServer }> {
  const { server, connection } = await fakeConnection("alice");
  const host = document.createElement("div");
  document.body.append(host);
  dispose = render(() => <PolkitDialog connection={connection} />, host);
  const [open] = server.take();
  expect(parseJson(open!)).toEqual({ kind: "polkit" });
  server.push(readyFrame(1));
  return { host, server };
}

const push = (server: FakeServer, event: PolkitEvent) => server.push(encodeJsonFrame(1, FrameType.Control, event));
const replies = (server: FakeServer) => server.take().map((frame) => parseJson<Record<string, unknown>>(frame));
const begin = (request: number, identities = ["alice"]): PolkitEvent => ({
  type: "begin",
  request,
  action: "org.freedesktop.timedate1.set-timezone",
  message: "Authentication is required to set the system timezone.",
  identities,
  identity: identities[0]!,
});

function submit(host: HTMLElement, text: string) {
  const input = host.querySelector<HTMLInputElement>("input")!;
  input.value = text;
  host.querySelector("form")!.dispatchEvent(new SubmitEvent("submit", { bubbles: true, cancelable: true }));
}

describe("PolkitDialog", () => {
  test("a request shows the message and the prompt, sends the answer, and closes at the end", async () => {
    const { host, server } = await mount();
    expect(host.querySelector("form")).toBeNull();
    push(server, begin(1));
    push(server, { type: "prompt", request: 1, text: "Password: ", echo: false });
    await settle();
    expect(host.querySelector(".polkit-message")?.textContent).toBe("Authentication is required to set the system timezone.");
    expect(host.textContent).toContain("Authenticate as alice");
    const input = host.querySelector<HTMLInputElement>("input")!;
    expect(input.type).toBe("password");
    expect(input.disabled).toBe(false);

    submit(host, "secret");
    await settle();
    expect(replies(server)).toEqual([{ type: "answer", request: 1, text: "secret" }]);
    expect(host.querySelector("[role=status]")?.textContent).toBe("Checking");
    push(server, { type: "done", request: 1, success: true });
    await settle();
    expect(host.querySelector("form")).toBeNull();
  });

  test("a wrong password shows an alert and starts a new attempt at once", async () => {
    const { host, server } = await mount();
    push(server, begin(1));
    push(server, { type: "failed", request: 1 });
    await settle();
    expect(host.querySelector("[role=alert]")?.textContent).toContain("failed");
    expect(replies(server)).toEqual([{ type: "start", request: 1, identity: "alice" }]);
  });

  test("a different user starts a new attempt, and Cancel cancels", async () => {
    const { host, server } = await mount();
    push(server, begin(1, ["alice", "root"]));
    await settle();
    const select = host.querySelector("select")!;
    select.value = "root";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    [...host.querySelectorAll("button")].find((b) => b.textContent === "Cancel")!.click();
    expect(replies(server)).toEqual([
      { type: "start", request: 1, identity: "root" },
      { type: "cancel", request: 1 },
    ]);
  });

  test("a second request waits until the first one ends", async () => {
    const { host, server } = await mount();
    push(server, begin(1));
    push(server, { ...begin(2), message: "Second" } as PolkitEvent);
    await settle();
    expect(host.querySelector(".polkit-message")?.textContent).toContain("timezone");
    push(server, { type: "done", request: 1, success: false });
    await settle();
    expect(host.querySelector(".polkit-message")?.textContent).toBe("Second");
  });
});
