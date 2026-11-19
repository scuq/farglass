import { render } from "@solidjs/web";
import { afterEach, describe, expect, test } from "vitest";
import { encodeJsonFrame, type Frame, FrameType, parseJson, readyFrame } from "../../proto";
import { type FakeServer, fakeConnection } from "../../proto/testing";
import { StateClient } from "../../state";
import type { AppContext } from "../types";
import { addresses } from "./pages/Network";
import { hostnameError } from "./pages/System";
import Settings from "./Settings";

let dispose: (() => void) | undefined;

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.accent;
});

const settle = async () => {
  for (let i = 0; i < 5; i++) await new Promise((resolve) => setTimeout(resolve, 0));
};

type Call = { destination: string; path: string; interface: string; method: string; args: unknown[]; interactive: boolean };
/** Answers a call with return values, or with an error name. */
type Answer = (call: Call) => unknown[] | { error: string } | undefined;

/** A fake host: it answers the calls of the dbus channel with `answer` and records them. */
class FakeHost {
  readonly calls: Call[] = [];
  readonly state: Record<string, unknown>[] = [];
  #dbus = -1;
  #state = -1;

  constructor(
    readonly server: FakeServer,
    readonly answer: Answer,
  ) {}

  /** Handles each frame of the shell, until no new frame comes. */
  async pump(): Promise<void> {
    for (let round = 0; round < 10; round++) {
      await settle();
      const frames = this.server.take();
      if (frames.length === 0) return;
      for (const frame of frames) this.#handle(frame);
    }
  }

  #handle(frame: Frame) {
    if (frame.type === FrameType.Open) {
      const open = parseJson<{ kind: string }>(frame);
      if (open.kind === "dbus") this.#dbus = frame.channel;
      if (open.kind === "state") this.#state = frame.channel;
      this.server.push(readyFrame(frame.channel));
      return;
    }
    if (frame.type !== FrameType.Control) return;
    const message = parseJson<Record<string, unknown>>(frame);
    if (frame.channel === this.#state) {
      this.state.push(message);
      if (message.type === "get") {
        this.server.push(encodeJsonFrame(frame.channel, FrameType.Control, { type: "value", key: message.key, value: null }));
      }
      return;
    }
    if (frame.channel !== this.#dbus) return;
    if (message.type === "subscribe") {
      this.server.push(encodeJsonFrame(frame.channel, FrameType.Control, { type: "reply", id: message.id, values: [] }));
      return;
    }
    if (message.type !== "call") return;
    const call = message as unknown as Call & { id: number };
    this.calls.push(call);
    const result = this.answer(call) ?? { error: "org.freedesktop.DBus.Error.UnknownMethod" };
    const event = Array.isArray(result)
      ? { type: "reply", id: call.id, values: result }
      : { type: "error", id: call.id, name: result.error, message: result.error };
    this.server.push(encodeJsonFrame(frame.channel, FrameType.Control, event));
  }
}

/** Returns the properties as GetAll gives them: each value in a variant. */
const all = (properties: Record<string, unknown>) => [Object.fromEntries(Object.entries(properties).map(([key, value]) => [key, { type: "s", value }]))];

async function mount(answer: Answer, page?: string) {
  const { server, connection } = await fakeConnection("alice");
  const state = new StateClient(connection);
  const host = new FakeHost(server, answer);
  const unwatch: (() => void)[] = [];
  const context: AppContext = {
    windowId: "w1",
    user: "alice",
    connection,
    openChannel: (open, handlers) => connection.openChannel(open, handlers),
    state: {
      watch: (key, onvalue) => void unwatch.push(state.watch(key, onvalue)),
      set: (key, value) => state.set(key, value),
      key: (name) => `settings.${name}.w1`,
      watchShared: (key, onvalue) => void unwatch.push(state.watch(key, onvalue)),
      setShared: (key, value) => state.set(key, value, { local: true }),
    },
    setTitle: () => {},
    openApp: () => {},
    onClose: () => {},
    onFocus: () => {},
    isFocused: () => true,
    contextMenu: () => {},
  };
  const element = document.createElement("div");
  document.body.append(element);
  dispose = render(() => <Settings context={context} />, element);
  await host.pump();
  if (page) {
    element.querySelector<HTMLButtonElement>(`[data-page="${page}"]`)!.click();
    await host.pump();
  }
  return { element, host };
}

const button = (element: HTMLElement, text: string) =>
  [...element.querySelectorAll<HTMLButtonElement>("button")].find((each) => each.textContent?.trim() === text)!;

describe("settings", () => {
  test("a theme goes to the state store of the session, with the watchers of this tab", async () => {
    const { element, host } = await mount(() => undefined);
    const dark = element.querySelector<HTMLInputElement>('input[name="theme"][value="dark"]')!;
    dark.click();
    await host.pump();
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(host.state.at(-1)).toEqual({ type: "set", key: "appearance", value: { theme: "dark", accent: "blue", textSize: 100 } });
    expect(element.querySelector('[data-page="appearance"]')?.getAttribute("aria-current")).toBe("page");
  });

  test("the list of pages moves with the arrow keys and keeps the page in the state of the window", async () => {
    const { element, host } = await mount(() => undefined);
    element.querySelector<HTMLButtonElement>('[data-page="appearance"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    await host.pump();
    expect(element.querySelector('[aria-current="page"]')?.textContent).toBe("Date and time");
    expect(host.state).toContainEqual({ type: "set", key: "settings.page.w1", value: "datetime" });
  });

  test("the system page shows the facts and checks a hostname before the call", async () => {
    const { element, host } = await mount((call) => {
      if (call.method === "GetAll") return all({ StaticHostname: "fw01", OperatingSystemPrettyName: "Debian GNU/Linux 13", KernelName: "Linux" });
      if (call.method === "SetStaticHostname") return [];
    }, "system");
    expect(element.querySelector('[data-field="os"]')?.textContent).toBe("Debian GNU/Linux 13");
    const input = element.querySelector<HTMLInputElement>("#settings-hostname")!;
    expect(input.value).toBe("fw01");
    input.value = "-bad-";
    input.form!.requestSubmit();
    await host.pump();
    expect(element.querySelector("[role=alert]")?.textContent).toContain("hyphen");
    expect(host.calls.some((call) => call.method === "SetStaticHostname")).toBe(false);
    input.value = "fw02.cyberdyne.example";
    input.form!.requestSubmit();
    await host.pump();
    const set = host.calls.find((call) => call.method === "SetStaticHostname")!;
    expect([set.args, set.interactive]).toEqual([["fw02.cyberdyne.example", true], true]);
    expect(element.querySelector(".settings-outcome")?.textContent).toBe("The hostname is fw02.cyberdyne.example.");
  });

  test("a refused authorization says that nothing changed", async () => {
    const { element, host } = await mount((call) => {
      if (call.method === "GetAll") return all({ Timezone: "UTC", NTP: true, CanNTP: true, NTPSynchronized: true, TimeUSec: 0 });
      if (call.method === "ListTimezones") return [["Europe/Vienna", "UTC"]];
      if (call.method === "SetTimezone") return { error: "org.freedesktop.PolicyKit1.Error.NotAuthorized" };
    }, "datetime");
    const input = element.querySelector<HTMLInputElement>("#settings-timezone")!;
    input.value = "Mars/Olympus";
    input.form!.requestSubmit();
    await host.pump();
    expect(element.querySelector("[role=alert]")?.textContent).toBe("Mars/Olympus is not a timezone of this host.");
    input.value = "Europe/Vienna";
    input.form!.requestSubmit();
    await host.pump();
    expect(element.querySelector("[role=alert]")?.textContent).toBe("The change was not authorized. Nothing changed.");
  });

  test("ending a session needs the confirm step, and the session of this tab says so", async () => {
    const { element, host } = await mount((call) => {
      if (call.method === "ListSessions") return [[["4", 1000, "alice", "", "/s/4"], ["2", 1001, "bob", "seat0", "/s/2"]]];
      if (call.method === "ListUsers") return [[[1000, "alice", "/u/1000"]]];
      if (call.method === "Get") return [{ type: "s", value: "active" }];
      if (call.method === "GetAll" && call.path === "/s/4") return all({ Type: "web", Class: "user", State: "active", Service: "farglass-session" });
      if (call.method === "GetAll") return all({ Type: "wayland", Class: "user", State: "online", Service: "gdm" });
      if (call.method === "TerminateSession") return [];
    }, "users");
    const rows = [...element.querySelectorAll<HTMLElement>("tr[data-session]")];
    expect(rows.map((row) => row.dataset.session)).toEqual(["4", "2"]);
    expect(rows[0]!.textContent).toContain("This session");

    button(rows[0]!, "End").click();
    await host.pump();
    expect(element.querySelector(".settings-confirm")?.textContent).toContain("This is the session of this desktop.");
    button(element.querySelector(".settings-confirm")!, "Cancel").click();
    await host.pump();
    expect(element.querySelector(".settings-confirm")).toBeNull();

    button(rows[1]!, "End").click();
    await host.pump();
    button(element.querySelector(".settings-confirm")!, "End the session").click();
    await host.pump();
    const end = host.calls.filter((call) => call.method === "TerminateSession");
    expect(end.map((call) => call.args)).toEqual([["2"]]);
  });

  test("power actions need the confirm step, and an action that logind refuses is not available", async () => {
    const { element, host } = await mount((call) => {
      if (call.method === "CanReboot") return ["yes"];
      if (call.method === "CanPowerOff") return ["na"];
      if (call.method === "Reboot") return [];
    }, "power");
    expect(button(element, "Power off").disabled).toBe(true);
    button(element, "Restart").click();
    await host.pump();
    expect(host.calls.some((call) => call.method === "Reboot")).toBe(false);
    button(element.querySelector(".settings-confirm")!, "Restart").click();
    await host.pump();
    const reboot = host.calls.find((call) => call.method === "Reboot")!;
    expect([reboot.args, reboot.interactive]).toEqual([[true], true]);
  });

  test("the network page shows each device with its state and addresses", async () => {
    const { element, host } = await mount((call) => {
      if (call.method === "GetAll" && call.args[0] === "org.freedesktop.NetworkManager") return [{ State: { type: "u", value: 70 }, Devices: { type: "ao", value: ["/d/1", "/d/2"] } }];
      if (call.method === "GetAll" && call.path === "/d/1")
        return [{ Interface: { type: "s", value: "enp0s1" }, DeviceType: { type: "u", value: 1 }, State: { type: "u", value: 100 }, Ip4Config: { type: "o", value: "/ip/1" }, Ip6Config: { type: "o", value: "/" }, ActiveConnection: { type: "o", value: "/" } }];
      if (call.method === "GetAll" && call.path === "/d/2")
        return [{ Interface: { type: "s", value: "lo" }, DeviceType: { type: "u", value: 32 }, State: { type: "u", value: 100 } }];
      if (call.method === "GetAll" && call.path === "/ip/1")
        return [{ AddressData: { type: "aa{sv}", value: [{ address: { type: "s", value: "192.0.2.10" }, prefix: { type: "u", value: 24 } }] }, Gateway: { type: "s", value: "192.0.2.1" } }];
    }, "network");
    expect(element.querySelector('[data-field="network-state"]')?.textContent).toBe("Connected");
    const items = [...element.querySelectorAll(".settings-item")];
    expect(items).toHaveLength(1);
    expect(items[0]!.textContent).toContain("enp0s1");
    expect(items[0]!.textContent).toContain("Connected");
    expect(items[0]!.textContent).toContain("192.0.2.10/24");
  });
});

describe("checks", () => {
  test("hostnames follow RFC 1123", () => {
    expect(hostnameError("fw01")).toBeNull();
    expect(hostnameError("fw01.cyberdyne.example")).toBeNull();
    expect(hostnameError("")).not.toBeNull();
    expect(hostnameError("a_b")).not.toBeNull();
    expect(hostnameError("x".repeat(65))).not.toBeNull();
  });

  test("addresses come from the AddressData of NetworkManager", () => {
    expect(addresses([{ address: { type: "s", value: "2001:db8::1" }, prefix: { type: "u", value: 64 } }])).toEqual(["2001:db8::1/64"]);
    expect(addresses(null)).toEqual([]);
  });
});
