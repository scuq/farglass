import { afterEach, describe, expect, test, vi } from "vitest";
import { dataFrame, encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { hostTime, mustSeek, parseVideoMode, placement, RedirectManager, SMART_IDLE_MS, sameTitle, smartHostDraws, stepRate } from "./redirect";

afterEach(() => vi.unstubAllGlobals());

describe("placement", () => {
  test("the video goes into the window below the browser frame, scaled by the zoom and clipped to the viewport", () => {
    const viewport = { x: 0, y: 90, width: 1000, height: 700 };
    expect(placement({ rect: { x: 10, y: 20, width: 400, height: 225 }, viewport, zoom: 1.25 })).toEqual({ x: 12.5, y: 115, width: 500, height: 281.25 });
    // Scrolled half out at the top: the part under the toolbar does not show.
    expect(placement({ rect: { x: 0, y: -100, width: 400, height: 200 }, viewport, zoom: 1 })).toEqual({ x: 0, y: 90, width: 400, height: 100 });
    expect(placement({ rect: { x: 0, y: -300, width: 400, height: 200 }, viewport, zoom: 1 })).toBeNull();
    // The window geometry from the compositor: no shadow. A frame of 1000 x 790 has no side frame and the toolbar above.
    expect(placement({ rect: { x: 10, y: 20, width: 400, height: 225 }, viewport: { x: 12, y: 102, width: 1000, height: 700 }, zoom: 1 }, { width: 1000, height: 790 })).toEqual({
      x: 10,
      y: 110,
      width: 400,
      height: 225,
    });
  });

  test("the time of the host moves on while it plays, and the client seeks only for a large difference", () => {
    expect(hostTime({ time: 10, paused: false, rate: 2 }, 500)).toBe(11);
    expect(hostTime({ time: 10, paused: true, rate: 1 }, 5000)).toBe(10);
    expect(mustSeek(10.2, 10)).toBe(false);
    expect(mustSeek(11, 10)).toBe(true);
  });
});

describe("RedirectManager", () => {
  test("an append in two data frames goes to its SourceBuffer in order, and showing goes to the agent", async () => {
    const appended: number[][] = [];
    class FakeBuffer extends EventTarget {
      updating = false;
      timestampOffset = 0;
      appendBuffer(data: Uint8Array) {
        appended.push([...data]);
      }
      remove() {}
      abort() {}
    }
    class FakeMediaSource extends EventTarget {
      readyState = "open";
      duration = Number.NaN;
      static isTypeSupported = () => true;
      addSourceBuffer() {
        return new FakeBuffer();
      }
      endOfStream() {}
    }
    vi.stubGlobal("MediaSource", FakeMediaSource);
    URL.createObjectURL = vi.fn(() => "blob:media");
    URL.revokeObjectURL = vi.fn();
    const { server, connection } = await fakeConnection("alice");
    const manager = new RedirectManager(connection);
    const [open] = server.take();
    expect(parseJson(open!)).toEqual({ kind: "redirect" });
    server.push(readyFrame(1));
    const event = (message: object) => server.push(encodeJsonFrame(1, FrameType.Control, message));
    event({ type: "source", media: "1:7:m", source: "s1", mime: 'video/webm; codecs="vp8"' });
    event({ type: "append", media: "1:7:m", source: "s1", offset: 0, size: 5 });
    server.push(dataFrame(1, new Uint8Array([1, 2])));
    expect(appended).toEqual([]);
    server.push(dataFrame(1, new Uint8Array([3, 4, 5])));
    expect(appended).toEqual([[1, 2, 3, 4, 5]]);
    manager.setShowing("1:7:m", true);
    manager.setShowing("1:7:m", true);
    expect(server.take().map((f) => parseJson(f))).toEqual([{ type: "showing", media: "1:7:m", showing: true }]);
    manager.close();
  });
});

describe("health", () => {
  test("a video with no first frame while the host plays goes back to the host, and the diagnosis says why", async () => {
    class FakeMediaSource extends EventTarget {
      readyState = "open";
      duration = Number.NaN;
      static isTypeSupported = () => true;
      addSourceBuffer() {
        return Object.assign(new EventTarget(), { updating: false, timestampOffset: 0, appendBuffer() {}, remove() {}, abort() {}, changeType() {} });
      }
      endOfStream() {}
    }
    vi.stubGlobal("MediaSource", FakeMediaSource);
    URL.createObjectURL = vi.fn(() => "blob:media");
    URL.revokeObjectURL = vi.fn();
    const { server, connection } = await fakeConnection("alice");
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "performance"] });
    const manager = new RedirectManager(connection);
    server.take();
    server.push(readyFrame(1));
    const event = (message: object) => server.push(encodeJsonFrame(1, FrameType.Control, message));
    const rect = { x: 0, y: 0, width: 640, height: 360 };
    event({ type: "source", media: "m", source: "s", mime: 'video/webm; codecs="vp9"' });
    event({ type: "change-type", media: "m", source: "s", mime: 'video/mp4; codecs="av01.0.08M.08"' });
    event({ type: "state", media: "m", time: 1, paused: false, rate: 1, rect, viewport: rect, zoom: 1, title: "A video", visible: true });
    vi.advanceTimersByTime(5000);
    const diagnosis = () => (globalThis as unknown as { farglassRedirect: () => { failed: string | null; sources: { mime: string }[] }[] }).farglassRedirect()[0]!;
    expect(diagnosis().failed).toBeNull();
    vi.advanceTimersByTime(4000);
    expect(diagnosis().failed).toBe("no first frame");
    expect(manager.playersFor(4, "A video - Brave")).toEqual([]);
    manager.close();
    vi.useRealTimers();
  });
});

describe("video mode", () => {
  test("the mode comes from the state store, and on the host the client shows no video", async () => {
    expect(parseVideoMode(null)).toBe("smart");
    expect(parseVideoMode({ mode: "client" })).toBe("client");
    expect(parseVideoMode({ mode: "host" })).toBe("host");
    expect(parseVideoMode({ mode: "other" })).toBe("smart");
    const { connection } = await fakeConnection("alice");
    const manager = new RedirectManager(connection);
    manager.setMode("host");
    expect(manager.playersFor(4, "Any - Brave")).toEqual([]);
    manager.close();
  });
});

describe("smart mode (F073)", () => {
  test("the mode smart comes from the state store", () => {
    expect(parseVideoMode({ mode: "smart" })).toBe("smart");
  });

  test("the host draws a video while the pointer moves on it, and while it is paused under the pointer", () => {
    const pointer = { media: "m1", at: 1000 };
    expect(smartHostDraws(pointer, "m1", 1000 + SMART_IDLE_MS - 1, false)).toBe(true);
    expect(smartHostDraws(pointer, "m1", 1000 + SMART_IDLE_MS, false)).toBe(false);
    expect(smartHostDraws(pointer, "m1", 1000 + SMART_IDLE_MS * 10, true)).toBe(true);
    expect(smartHostDraws(pointer, "m2", 1000, false)).toBe(false);
    expect(smartHostDraws(null, "m1", 1000, true)).toBe(false);
  });

  test("the client plays a little faster or slower to come back to the time of the host, with no seek", () => {
    expect(stepRate(1, 0.01)).toBe(1);
    expect(stepRate(1, 0.1)).toBeCloseTo(1.05);
    expect(stepRate(1, -0.1)).toBeCloseTo(0.95);
    expect(stepRate(2, 0.39)).toBeCloseTo(2.2);
  });

  test("only the smart mode gives a video to the host under the pointer", async () => {
    const { connection } = await fakeConnection("alice");
    const manager = new RedirectManager(connection);
    manager.setMode("client");
    await Promise.resolve();
    manager.pointer("m1");
    expect(manager.hostDraws("m1")).toBe(false);
    manager.setMode("smart");
    // Solid commits the write of a signal on the next microtask.
    await Promise.resolve();
    expect(manager.hostDraws("m1")).toBe(true);
    expect(manager.hostDraws("m2")).toBe(false);
    manager.pointer(null);
    expect(manager.hostDraws("m1")).toBe(false);
    manager.close();
  });
});

describe("placement (F052)", () => {
  test("a window title matches the page title only with no more than the browser name", () => {
    expect(sameTitle("A video", "A video")).toBe(true);
    expect(sameTitle("A video - Chromium", "A video")).toBe(true);
    expect(sameTitle("A video — Brave", "A video")).toBe(true);
    expect(sameTitle("Song - Artist - YouTube - Chromium", "Song - Artist - YouTube")).toBe(true);
    // A page cannot claim a window of a different page with a short title.
    expect(sameTitle("A video of the bank - Chromium", "A")).toBe(false);
    expect(sameTitle("Terminal", "Term")).toBe(false);
    expect(sameTitle("Bank - Login - Chromium", "Bank")).toBe(false);
  });
});
