// @vitest-environment node
import { describe, expect, test } from "vitest";
import { DbusClient, DbusError } from "./dbus";
import { closeFrame, encodeJsonFrame, type Frame, FrameType, parseJson, readyFrame } from "./proto";
import { fakeConnection } from "./proto/testing";

async function client() {
  const { server, connection } = await fakeConnection("alice");
  const dbus = new DbusClient(connection, "system");
  const [open] = server.take();
  expect(parseJson(open!)).toEqual({ kind: "dbus", bus: "system" });
  server.push(readyFrame(1));
  return { server, dbus };
}

const event = (value: unknown) => encodeJsonFrame(1, FrameType.Control, value);
const request = (frame: Frame) => parseJson<Record<string, unknown>>(frame);

describe("DbusClient", () => {
  test("a call sends its request and gets the reply by id", async () => {
    const { server, dbus } = await client();
    const first = dbus.call("org.freedesktop.timedate1", "/org/freedesktop/timedate1", "org.freedesktop.timedate1", "SetNTP", "bb", [true, true], {
      interactive: true,
    });
    const second = dbus.call("org.freedesktop.DBus", "/", "org.freedesktop.DBus", "ListNames");
    const [a, b] = server.take().map(request);
    expect(a).toEqual({
      type: "call",
      id: 1,
      destination: "org.freedesktop.timedate1",
      path: "/org/freedesktop/timedate1",
      interface: "org.freedesktop.timedate1",
      method: "SetNTP",
      signature: "bb",
      args: [true, true],
      interactive: true,
    });
    expect(b!.id).toBe(2);
    server.push(event({ type: "reply", id: 2, values: [["a", "b"]] }));
    server.push(event({ type: "reply", id: 1, values: [] }));
    expect(await second).toEqual([["a", "b"]]);
    expect(await first).toEqual([]);
  });

  test("an error rejects the call with its D-Bus name", async () => {
    const { server, dbus } = await client();
    const call = dbus.call("org.freedesktop.systemd1", "/", "x.y", "Z");
    server.push(event({ type: "error", id: 1, name: "org.freedesktop.DBus.Error.AccessDenied", message: "refused" }));
    await expect(call).rejects.toMatchObject({ dbusName: "org.freedesktop.DBus.Error.AccessDenied", message: "refused" });
    await expect(call).rejects.toBeInstanceOf(DbusError);
  });

  test("properties: get unwraps the variant, set builds it", async () => {
    const { server, dbus } = await client();
    const get = dbus.getProperty("org.freedesktop.hostname1", "/org/freedesktop/hostname1", "org.freedesktop.hostname1", "Hostname");
    expect(request(server.take()[0]!)).toMatchObject({ method: "Get", signature: "ss", args: ["org.freedesktop.hostname1", "Hostname"] });
    server.push(event({ type: "reply", id: 1, values: [{ type: "s", value: "desk" }] }));
    expect(await get).toBe("desk");

    const all = dbus.getAllProperties("org.freedesktop.hostname1", "/org/freedesktop/hostname1", "org.freedesktop.hostname1");
    server.take();
    server.push(event({ type: "reply", id: 2, values: [{ Hostname: { type: "s", value: "desk" }, Chassis: { type: "s", value: "vm" } }] }));
    expect(await all).toEqual({ Hostname: "desk", Chassis: "vm" });

    const set = dbus.setProperty("a.b", "/", "a.b", "Name", "s", "x");
    expect(request(server.take()[0]!)).toMatchObject({ method: "Set", signature: "ssv", args: ["a.b", "Name", { type: "s", value: "x" }] });
    server.push(event({ type: "reply", id: 3, values: [] }));
    await set;
  });

  test("signals reach the handler until the unsubscribe", async () => {
    const { server, dbus } = await client();
    const signals: unknown[] = [];
    const subscribing = dbus.subscribe({ sender: "org.freedesktop.timedate1", member: "PropertiesChanged" }, (s) => signals.push(s.values));
    expect(request(server.take()[0]!)).toEqual({ type: "subscribe", id: 1, sender: "org.freedesktop.timedate1", member: "PropertiesChanged" });
    server.push(event({ type: "reply", id: 1, values: [] }));
    const unsubscribe = await subscribing;
    server.push(event({ type: "signal", subscription: 1, path: "/", interface: "i", member: "PropertiesChanged", values: [1] }));
    unsubscribe();
    expect(request(server.take()[0]!)).toEqual({ type: "unsubscribe", id: 1 });
    server.push(event({ type: "signal", subscription: 1, path: "/", interface: "i", member: "PropertiesChanged", values: [2] }));
    expect(signals).toEqual([[1]]);
  });

  test("the end of the channel rejects the open calls", async () => {
    const { server, dbus } = await client();
    const call = dbus.call("a.b", "/", "a.b", "M");
    server.push(closeFrame(1, { reason: "internal-error", message: "The session agent stopped." }));
    await expect(call).rejects.toMatchObject({ dbusName: "org.freedesktop.DBus.Error.Disconnected" });
  });
});
