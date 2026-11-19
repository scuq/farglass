// @vitest-environment node
import { afterEach, describe, expect, test, vi } from "vitest";
import { FakeServer } from "./testing";
import {
  type Close,
  Connection,
  closeFrame,
  dataFrame,
  decodeFrame,
  encodeJsonFrame,
  FrameType,
  helloFrame,
  parseJson,
  PROTOCOL_VERSION,
  readyFrame,
} from "./index";

async function started(): Promise<{ server: FakeServer; connection: Connection }> {
  const server = new FakeServer();
  const promise = Connection.start(server);
  server.push(helloFrame());
  const connection = await promise;
  return { server, connection };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("Connection", () => {
  test("the shell sends its hello message first and opens after the hello of the server", async () => {
    const server = new FakeServer();
    const promise = Connection.start(server);
    const [hello] = server.take();
    expect(hello?.channel).toBe(0);
    expect(parseJson(hello!)).toEqual({ type: "hello", version: PROTOCOL_VERSION });

    server.push(helloFrame());
    const connection = await promise;
    expect(connection.state).toBe("open");
  });

  test("a different protocol version fails the start", async () => {
    const server = new FakeServer();
    const promise = Connection.start(server);
    server.push(encodeJsonFrame(0, FrameType.Control, { type: "hello", version: PROTOCOL_VERSION + 1 }));
    await expect(promise).rejects.toThrow("protocol version");
    expect(server.closed).toBe(true);
  });

  test("no hello message in 10 seconds fails the start", async () => {
    vi.useFakeTimers();
    const server = new FakeServer();
    const promise = Connection.start(server);
    vi.advanceTimersByTime(10_000);
    await expect(promise).rejects.toThrow("no hello message");
  });
});

describe("Channel", () => {
  test("open, ready, data in two directions, and close", async () => {
    const { server, connection } = await started();
    server.take();
    const data: string[] = [];
    const closes: Close[] = [];
    const channel = connection.openChannel(
      { kind: "echo" },
      { ondata: (bytes) => data.push(new TextDecoder().decode(bytes)), onclose: (c) => closes.push(c) },
    );
    expect(channel.id).toBe(1);

    // The opener can send before the answer.
    channel.send(new TextEncoder().encode("early"));
    const [open, early] = server.take();
    expect(open?.type).toBe(FrameType.Open);
    expect(parseJson(open!)).toEqual({ kind: "echo" });
    expect(early?.type).toBe(FrameType.Data);

    server.push(readyFrame(1));
    await channel.ready;
    server.push(dataFrame(1, new TextEncoder().encode("echo")));
    expect(data).toEqual(["echo"]);

    channel.close();
    expect(parseJson<Close>(server.take()[0]!)).toEqual({ reason: "done" });
    server.push(closeFrame(1, { reason: "done" }));
    expect(closes).toEqual([{ reason: "done" }]);
    // The shell closed first, thus it does not answer.
    expect(server.take()).toEqual([]);
  });

  test("a refusal rejects ready, and the shell answers the close frame", async () => {
    const { server, connection } = await started();
    const channel = connection.openChannel({ kind: "echo" });
    server.take();
    server.push(closeFrame(1, { reason: "access-denied", message: "The connection has no login." }));
    await expect(channel.ready).rejects.toEqual({
      reason: "access-denied",
      message: "The connection has no login.",
    });
    const [answer] = server.take();
    expect(answer?.type).toBe(FrameType.Close);
    expect(channel.open).toBe(false);
  });

  test("control messages go in the two directions as JSON", async () => {
    const { server, connection } = await started();
    const messages: unknown[] = [];
    const channel = connection.openChannel({ kind: "echo" }, { oncontrol: (m) => messages.push(m) });
    server.take();
    channel.control({ cols: 80 });
    expect(parseJson(server.take()[0]!)).toEqual({ cols: 80 });
    server.push(encodeJsonFrame(1, FrameType.Control, { rows: 24 }));
    expect(messages).toEqual([{ rows: 24 }]);
  });

  test("channel numbers are odd and increase", async () => {
    const { connection } = await started();
    const ids = [1, 2, 3].map(() => connection.openChannel({ kind: "echo" }).id);
    expect(ids).toEqual([1, 3, 5]);
  });

  test("the end of the connection closes each channel", async () => {
    const { server, connection } = await started();
    const closes: Close[] = [];
    connection.openChannel({ kind: "echo" }, { onclose: (c) => closes.push(c) });
    let reason = "";
    connection.onclose = (r) => {
      reason = r;
    };
    server.onclose?.("going away");
    expect(connection.state).toBe("closed");
    expect(reason).toBe("going away");
    expect(closes).toEqual([{ reason: "cancelled", message: "going away" }]);
  });

  test("a frame that is not valid stops the connection", async () => {
    const { server, connection } = await started();
    server.onmessage?.(new Uint8Array([1, 0, 0, 0, 9, 0, 0, 0]).buffer);
    expect(connection.state).toBe("closed");
    expect(server.closed).toBe(true);
  });
});
