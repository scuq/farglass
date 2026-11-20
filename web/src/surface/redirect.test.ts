import { afterEach, describe, expect, test, vi } from "vitest";
import { dataFrame, encodeJsonFrame, FrameType, parseJson, readyFrame } from "../proto";
import { fakeConnection } from "../proto/testing";
import { hostTime, mustSeek, placement, RedirectManager } from "./redirect";

afterEach(() => vi.unstubAllGlobals());

describe("placement", () => {
  test("the video goes into the window below the browser frame, scaled by the zoom and clipped to the viewport", () => {
    const viewport = { x: 0, y: 90, width: 1000, height: 700 };
    expect(placement({ rect: { x: 10, y: 20, width: 400, height: 225 }, viewport, zoom: 1.25 })).toEqual({ x: 12.5, y: 115, width: 500, height: 281.25 });
    // Scrolled half out at the top: the part under the toolbar does not show.
    expect(placement({ rect: { x: 0, y: -100, width: 400, height: 200 }, viewport, zoom: 1 })).toEqual({ x: 0, y: 90, width: 400, height: 100 });
    expect(placement({ rect: { x: 0, y: -300, width: 400, height: 200 }, viewport, zoom: 1 })).toBeNull();
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
