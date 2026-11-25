import { describe, expect, test } from "vitest";
import { closeFrame, decodeFrame, encodeJsonFrame, FrameType, helloFrame, openFrame } from ".";
import { frameReader, sendOrder, WebTransportTransport, webTransportUrl } from "./webtransport";

/** A WebTransport session in memory: each stream records what the shell writes. */
class FakeSession {
  readonly streams: { written: Uint8Array[]; closed: boolean; order: number | undefined; push: (bytes: Uint8Array) => void }[] = [];
  #closed!: (info: { closeCode: number; reason: string }) => void;
  readonly closed = new Promise<{ closeCode: number; reason: string }>((resolve) => (this.#closed = resolve));
  readonly ready = Promise.resolve();

  async createBidirectionalStream(options?: { sendOrder?: number }) {
    const record = { written: [] as Uint8Array[], closed: false, order: options?.sendOrder, push: (_: Uint8Array) => {} };
    this.streams.push(record);
    const readable = new ReadableStream<Uint8Array>({
      start(controller) {
        record.push = (bytes) => controller.enqueue(bytes);
      },
    });
    const writable = new WritableStream<Uint8Array>({
      write(chunk) {
        record.written.push(chunk);
      },
      close() {
        record.closed = true;
      },
    });
    return { readable, writable };
  }

  close(info: { closeCode: number; reason: string }) {
    this.#closed(info);
  }
}

const settle = async () => {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
};

describe("WebTransport transport", () => {
  test("the frame reader joins split chunks and splits joined frames", () => {
    const frames: number[] = [];
    const feed = frameReader((frame) => frames.push(decodeFrame(frame).channel));
    const a = encodeJsonFrame(1, FrameType.Control, { type: "x" });
    const b = encodeJsonFrame(3, FrameType.Control, { type: "y" });
    const joined = new Uint8Array([...a, ...b]);
    feed(joined.slice(0, 5));
    feed(joined.slice(5, a.byteLength + 3));
    expect(frames).toEqual([1]);
    feed(joined.slice(a.byteLength + 3));
    expect(frames).toEqual([1, 3]);
  });

  test("each channel gets its own stream, in the order of its frames, and a close frame ends it", async () => {
    const session = new FakeSession();
    const transport = new WebTransportTransport(session as unknown as WebTransport);
    const received: number[] = [];
    transport.onmessage = (message) => received.push(decodeFrame(message).channel);
    transport.send(helloFrame());
    transport.send(openFrame(1, { kind: "echo" }));
    transport.send(openFrame(3, { kind: "state" }));
    transport.send(closeFrame(1, { reason: "done" }));
    await settle();
    expect(session.streams).toHaveLength(3);
    expect(session.streams.map((stream) => stream.written.map((bytes) => decodeFrame(bytes).type))).toEqual([
      [FrameType.Control],
      [FrameType.Open, FrameType.Close],
      [FrameType.Open],
    ]);
    expect(session.streams.map((stream) => stream.closed)).toEqual([false, true, false]);

    session.streams[2]!.push(encodeJsonFrame(3, FrameType.Ready, {}));
    session.streams[0]!.push(encodeJsonFrame(0, FrameType.Control, { type: "hello", version: 1 }));
    await settle();
    expect(received.sort()).toEqual([0, 3]);
  });

  test("the bytes of a channel that wait for its stream count until the stream takes them", async () => {
    const session = new FakeSession();
    const transport = new WebTransportTransport(session as unknown as WebTransport);
    const open = openFrame(5, { kind: "camera" });
    const control = encodeJsonFrame(5, FrameType.Control, { type: "arm" });
    transport.send(open);
    transport.send(control);
    expect(transport.buffered(5)).toBe(open.byteLength + control.byteLength);
    expect(transport.buffered(7)).toBe(0);
    await settle();
    expect(transport.buffered(5)).toBe(0);
  });

  test("the input of the desktop goes first, and the camera goes last", async () => {
    expect(sendOrder(openFrame(1, { kind: "surface-input" }))).toBeGreaterThan(sendOrder(openFrame(3, { kind: "echo" })));
    expect(sendOrder(openFrame(3, { kind: "echo" }))).toBe(0);
    expect(sendOrder(openFrame(5, { kind: "camera" }))).toBeLessThan(sendOrder(openFrame(7, { kind: "mic" })));
    expect(sendOrder(helloFrame())).toBe(0);
    const session = new FakeSession();
    const transport = new WebTransportTransport(session as unknown as WebTransport);
    transport.send(openFrame(1, { kind: "surface-input" }));
    transport.send(openFrame(3, { kind: "camera" }));
    await settle();
    expect(session.streams.map((stream) => stream.order)).toEqual([100, -100]);
  });

  test("the end of the session reports its reason one time", async () => {
    const session = new FakeSession();
    const transport = new WebTransportTransport(session as unknown as WebTransport);
    const reasons: string[] = [];
    transport.onclose = (reason) => reasons.push(reason);
    session.close({ closeCode: 0, reason: "bye" });
    await settle();
    transport.send(helloFrame());
    expect(reasons).toEqual(["bye"]);
    expect(session.streams).toHaveLength(0);
  });

  test("the URL uses the host of the page, the port of the offer, and the ticket", () => {
    const location = new URL("https://[2001:db8::1]:8443/") as unknown as Location;
    expect(webTransportUrl({ port: 9443, path: "/wt", ticket: "abc", hashes: [] }, location).href).toBe("https://[2001:db8::1]:9443/wt?ticket=abc");
  });
});
