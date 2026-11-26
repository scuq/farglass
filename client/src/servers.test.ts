import { mkdtempSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { ServerStore, normalizeUrl } from "./servers";

const file = () => join(mkdtempSync(join(tmpdir(), "farglass-client-")), "servers.json");

describe("normalizeUrl", () => {
  it("adds https and the port of farglassd", () => {
    expect(normalizeUrl("desk.example")).toBe("https://desk.example:8443");
    expect(normalizeUrl(" Desk.Example ")).toBe("https://desk.example:8443");
    expect(normalizeUrl("[2001:db8::1]")).toBe("https://[2001:db8::1]:8443");
  });
  it("keeps a port that the user gives, also 443", () => {
    expect(normalizeUrl("https://desk.example:9443/")).toBe("https://desk.example:9443");
    expect(normalizeUrl("desk.example:443")).toBe("https://desk.example");
    expect(normalizeUrl("[2001:db8::1]:9443")).toBe("https://[2001:db8::1]:9443");
  });
  it("refuses other schemes, paths, and user names", () => {
    expect(() => normalizeUrl("http://desk.example")).toThrow(/https/);
    expect(() => normalizeUrl("https://desk.example/app")).toThrow(/path/);
    expect(() => normalizeUrl("https://desk.example/?a=1")).toThrow(/path/);
    expect(() => normalizeUrl("https://bob:pw@desk.example")).toThrow(/user name/);
    expect(() => normalizeUrl("")).toThrow(/address/);
  });
});

describe("ServerStore", () => {
  it("keeps the servers in a private file", () => {
    const path = file();
    const store = new ServerStore(path);
    const server = store.add({ name: "", url: "desk.example" });
    expect(server.name).toBe("desk.example");
    expect(new ServerStore(path).list()).toEqual([server]);
    expect(statSync(path).mode & 0o777).toBe(0o600);
  });
  it("lists the last used server first", () => {
    const store = new ServerStore(file());
    const a = store.add({ name: "A", url: "a.example" });
    const b = store.add({ name: "B", url: "b.example" });
    expect(store.list().map((s) => s.id)).toEqual([a.id, b.id]);
    store.touch(b.id, 1000);
    expect(store.list().map((s) => s.id)).toEqual([b.id, a.id]);
  });
  it("forgets the pin when the address changes", () => {
    const store = new ServerStore(file());
    const s = store.add({ name: "A", url: "a.example" });
    store.setPin(s.id, "ab");
    expect(store.update(s.id, { name: "A2", url: "a.example" }).pin).toBe("ab");
    expect(store.update(s.id, { name: "A2", url: "b.example" }).pin).toBeUndefined();
    store.setPin(s.id, "cd");
    store.forgetPin(s.id);
    expect(store.get(s.id)?.pin).toBeUndefined();
  });
  it("starts empty with a damaged file, and skips damaged entries", () => {
    const path = file();
    writeFileSync(path, "{not json");
    expect(new ServerStore(path).list()).toEqual([]);
    writeFileSync(path, JSON.stringify({ servers: [{ id: "1", name: "A", url: "https://a:8443" }, { id: 2 }] }));
    expect(new ServerStore(path).list().map((s) => s.id)).toEqual(["1"]);
  });
  it("keeps the close to tray setting, true by default", () => {
    const path = file();
    const store = new ServerStore(path);
    expect(store.closeToTray).toBe(true);
    store.closeToTray = false;
    expect(new ServerStore(path).closeToTray).toBe(false);
  });
  it("removes a server", () => {
    const path = file();
    const store = new ServerStore(path);
    const s = store.add({ name: "A", url: "a.example" });
    store.remove(s.id);
    expect(JSON.parse(readFileSync(path, "utf8")).servers).toEqual([]);
  });
});
