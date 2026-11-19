// @vitest-environment node
import { describe, expect, test } from "vitest";
import vectors from "../../../crates/farglass-proto/tests/vectors.json";
import {
  ChannelIdAllocator,
  CONNECTION_CHANNEL,
  type Close,
  closeFrame,
  dataFrame,
  decodeFrame,
  encodeFrame,
  FrameType,
  HEADER_LEN,
  helloFrame,
  isFrameType,
  MAX_PAYLOAD_LEN,
  openFrame,
  parseJson,
  PROTOCOL_VERSION,
  ProtocolError,
  readyFrame,
} from "./index";

function fromHex(hex: string): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  }
  return bytes;
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Returns the code of the ProtocolError that `run` throws. */
function errorCode(run: () => unknown): string {
  try {
    run();
  } catch (error) {
    if (error instanceof ProtocolError) {
      return error.code;
    }
    throw error;
  }
  throw new Error("The function did not throw.");
}

describe("the vectors that Rust and TypeScript share", () => {
  test.each(vectors.valid)("decode and encode: $name", (vector) => {
    const frame = decodeFrame(fromHex(vector.frame));
    expect(frame.channel).toBe(vector.channel);
    expect(frame.type).toBe(vector.type);
    expect(toHex(frame.payload)).toBe(vector.payload);

    expect(isFrameType(vector.type)).toBe(true);
    const encoded = encodeFrame(vector.channel, vector.type as FrameType, fromHex(vector.payload));
    expect(toHex(encoded)).toBe(vector.frame);

    if ("json" in vector) {
      expect(parseJson(frame)).toEqual(vector.json);
    }
  });

  test.each(vectors.invalid)("refuse: $name", (vector) => {
    expect(errorCode(() => decodeFrame(fromHex(vector.frame)))).toBe(vector.error);
  });
});

describe("encodeFrame", () => {
  test("accepts the largest payload", () => {
    const payload = new Uint8Array(MAX_PAYLOAD_LEN).fill(0xab);
    const frame = decodeFrame(encodeFrame(1, FrameType.Data, payload));
    expect(frame.payload.byteLength).toBe(MAX_PAYLOAD_LEN);
    expect(frame.payload.every((byte) => byte === 0xab)).toBe(true);
  });

  test("refuses a payload above the limit", () => {
    const payload = new Uint8Array(MAX_PAYLOAD_LEN + 1);
    expect(errorCode(() => dataFrame(1, payload))).toBe("payload-too-large");
  });

  test.each([-1, 1.5, 2 ** 32, Number.NaN])("refuses the channel number %d", (channel) => {
    expect(errorCode(() => dataFrame(channel, new Uint8Array(1)))).toBe("invalid-channel");
  });

  test("refuses an unknown frame type", () => {
    expect(errorCode(() => encodeFrame(1, 9 as FrameType))).toBe("unknown-frame-type");
  });

  test("lets only control frames on the connection channel", () => {
    for (const type of Object.values(FrameType)) {
      const run = () => encodeFrame(CONNECTION_CHANNEL, type);
      if (type === FrameType.Control) {
        expect(run).not.toThrow();
      } else {
        expect(errorCode(run)).toBe("invalid-connection-frame");
      }
    }
  });
});

describe("decodeFrame", () => {
  test("accepts an ArrayBuffer", () => {
    const frame = decodeFrame(dataFrame(3, new Uint8Array([1, 2, 3])).buffer);
    expect(frame).toEqual({ channel: 3, type: FrameType.Data, payload: new Uint8Array([1, 2, 3]) });
  });

  test("reads a frame at an offset in a larger buffer", () => {
    const encoded = dataFrame(0x0403_0201, new Uint8Array([7, 8]));
    const buffer = new Uint8Array(encoded.byteLength + 6);
    buffer.set(encoded, 3);
    const frame = decodeFrame(buffer.subarray(3, 3 + encoded.byteLength));
    expect(frame.channel).toBe(0x0403_0201);
    expect(Array.from(frame.payload)).toEqual([7, 8]);
  });

  test("does not copy the payload", () => {
    const encoded = dataFrame(1, new Uint8Array([1, 2, 3]));
    const frame = decodeFrame(encoded);
    expect(frame.payload.buffer).toBe(encoded.buffer);
    expect(frame.payload.byteOffset).toBe(HEADER_LEN);
  });
});

describe("JSON frames", () => {
  test("the hello frame carries the protocol version", () => {
    const frame = decodeFrame(helloFrame());
    expect(frame.channel).toBe(CONNECTION_CHANNEL);
    expect(frame.type).toBe(FrameType.Control);
    expect(parseJson(frame)).toEqual({ type: "hello", version: PROTOCOL_VERSION });
  });

  test("open, ready, and close use their frame types", () => {
    const close: Close = { reason: "cancelled", message: "The window closed." };
    expect(decodeFrame(openFrame(1, { kind: "echo" })).type).toBe(FrameType.Open);
    expect(decodeFrame(readyFrame(1)).type).toBe(FrameType.Ready);
    const frame = decodeFrame(closeFrame(1, close));
    expect(frame.type).toBe(FrameType.Close);
    expect(parseJson<Close>(frame)).toEqual(close);
  });

  test("a payload that is not JSON gives invalid-json", () => {
    const notJson = decodeFrame(encodeFrame(1, FrameType.Control, new Uint8Array([0x7b])));
    expect(errorCode(() => parseJson(notJson))).toBe("invalid-json");
    const notUtf8 = decodeFrame(encodeFrame(1, FrameType.Control, new Uint8Array([0xff, 0xfe])));
    expect(errorCode(() => parseJson(notUtf8))).toBe("invalid-json");
  });
});

describe("ChannelIdAllocator", () => {
  test("gives odd numbers in sequence", () => {
    const ids = new ChannelIdAllocator();
    expect([ids.allocate(), ids.allocate(), ids.allocate()]).toEqual([1, 3, 5]);
  });
});
