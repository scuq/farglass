import { describe, expect, test } from "vitest";
import { dataFrame, encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { type ClipContent, ClipboardBridge, isPasteKey } from "./clipboard";

/** A clipboard of the browser that the test controls. */
function fakeAccess(initial: ClipContent | null = null) {
  const access = {
    content: initial,
    writes: [] as ClipContent[],
    refuse: false,
    read: async () => access.content,
    write: async (content: ClipContent) => {
      if (access.refuse) throw new DOMException("no focus", "NotAllowedError");
      access.writes.push(content);
      access.content = content;
    },
  };
  return access;
}

async function bridge(initial: ClipContent | null = null) {
  const { server, connection } = await fakeConnection("alice");
  const access = fakeAccess(initial);
  const clipboard = new ClipboardBridge(connection, access);
  const [open] = server.take();
  expect(parseJson(open!)).toEqual({ kind: "clipboard" });
  server.push(readyFrame(1));
  return { server, access, clipboard };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("ClipboardBridge", () => {
  test("a copy in an app reaches the browser, also in more than one data frame", async () => {
    const { server, access } = await bridge();
    server.push(encodeJsonFrame(1, FrameType.Control, { type: "offer", mime: "text/plain", size: 8 }));
    server.push(dataFrame(1, new TextEncoder().encode("café ")));
    expect(access.writes).toEqual([]);
    server.push(dataFrame(1, new TextEncoder().encode("ok")));
    await settle();
    expect(access.writes).toEqual([{ mime: "text/plain", text: "café ok" }]);
  });

  test("a paste sends a new browser clipboard and waits until the agent stored it", async () => {
    const { server, access, clipboard } = await bridge({ mime: "text/plain", text: "from the Mac" });
    let done = false;
    const sync = clipboard.sync().then(() => (done = true));
    await settle();
    const frames = server.take();
    expect(frames.map((f) => f.type)).toEqual([FrameType.Control, FrameType.Data]);
    expect(parseJson(frames[0]!)).toEqual({ type: "offer", mime: "text/plain", size: 12 });
    expect(new TextDecoder().decode(frames[1]!.payload)).toBe("from the Mac");
    expect(done).toBe(false);
    server.push(encodeJsonFrame(1, FrameType.Control, { type: "stored" }));
    await sync;
    // The same content again: nothing goes, and the paste does not wait.
    await clipboard.sync();
    expect(server.take()).toEqual([]);
    // A copy in an app is not sent back.
    server.push(encodeJsonFrame(1, FrameType.Control, { type: "offer", mime: "image/png", size: 3 }));
    server.push(dataFrame(1, new Uint8Array([1, 2, 3])));
    await settle();
    expect(access.content).toEqual({ mime: "image/png", bytes: new Uint8Array([1, 2, 3]) });
    await clipboard.sync();
    expect(server.take()).toEqual([]);
  });

  test("a copy that the browser refused is not replaced by the older browser clipboard", async () => {
    const { server, access, clipboard } = await bridge({ mime: "text/plain", text: "old" });
    access.refuse = true;
    server.push(encodeJsonFrame(1, FrameType.Control, { type: "offer", mime: "text/plain", size: 3 }));
    server.push(dataFrame(1, new TextEncoder().encode("new")));
    await settle();
    await clipboard.sync();
    expect(server.take()).toEqual([]);
    access.refuse = false;
    await clipboard.sync();
    expect(access.content).toEqual({ mime: "text/plain", text: "new" });
    expect(server.take()).toEqual([]);
  });
});

describe("isPasteKey", () => {
  const key = (code: string, mods: Partial<KeyboardEventInit> = {}) => new KeyboardEvent("keydown", { code, ...mods });
  test("the paste keys of Linux apps, and Cmd+V on a Mac", () => {
    expect(isPasteKey(key("KeyV", { ctrlKey: true }), false)).toBe(true);
    expect(isPasteKey(key("KeyV", { ctrlKey: true, shiftKey: true }), false)).toBe(true);
    expect(isPasteKey(key("Insert", { shiftKey: true }), false)).toBe(true);
    expect(isPasteKey(key("KeyV", { metaKey: true }), true)).toBe(true);
    expect(isPasteKey(key("KeyV", { metaKey: true }), false)).toBe(false);
    expect(isPasteKey(key("KeyV"), false)).toBe(false);
    expect(isPasteKey(key("KeyC", { ctrlKey: true }), false)).toBe(false);
  });
});
