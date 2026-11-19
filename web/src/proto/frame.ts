/**
 * The frame codec of the channel protocol.
 *
 * The Rust crate farglass-proto defines the protocol.
 * `docs/protocol.md` is the reference for the wire format.
 */

import type { Close, ConnectionMessage, Open, Ready } from "./generated/messages";
import {
  CONNECTION_CHANNEL,
  type FrameErrorCode,
  FrameType,
  HEADER_LEN,
  MAX_PAYLOAD_LEN,
  PROTOCOL_VERSION,
} from "./generated/wire";

/** The code of a protocol error. */
export type ProtocolErrorCode = FrameErrorCode | "invalid-channel";

/**
 * An error from the encode or the decode of a frame.
 *
 * After a decode error, the receiver must close the connection.
 */
export class ProtocolError extends Error {
  override readonly name = "ProtocolError";

  constructor(
    /** The code of the error. The code does not change between versions. */
    readonly code: ProtocolErrorCode,
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options);
  }
}

/** One frame: a channel, a type, and a payload. */
export interface Frame {
  readonly channel: number;
  readonly type: FrameType;
  /** The payload. After a decode, it is a view into the message, not a copy. */
  readonly payload: Uint8Array;
}

const NO_PAYLOAD = new Uint8Array(0);
const MAX_CHANNEL = 0xffff_ffff;
const FRAME_TYPES: ReadonlySet<number> = new Set(Object.values(FrameType));
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

/** Reports whether `value` is the number of a frame type. */
export function isFrameType(value: number): value is FrameType {
  return FRAME_TYPES.has(value);
}

/** Throws a ProtocolError if the header values are not valid. */
function checkHeader(channel: number, type: number, payloadLen: number): asserts type is FrameType {
  if (!Number.isInteger(channel) || channel < 0 || channel > MAX_CHANNEL) {
    throw new ProtocolError("invalid-channel", `The channel number ${channel} is not a u32.`);
  }
  if (!isFrameType(type)) {
    throw new ProtocolError("unknown-frame-type", `The frame type ${type} is unknown.`);
  }
  if (payloadLen > MAX_PAYLOAD_LEN) {
    throw new ProtocolError(
      "payload-too-large",
      `The payload length ${payloadLen} is larger than the limit ${MAX_PAYLOAD_LEN}.`,
    );
  }
  if (channel === CONNECTION_CHANNEL && type !== FrameType.Control) {
    throw new ProtocolError(
      "invalid-connection-frame",
      `A frame of type ${type} is not valid on the connection channel.`,
    );
  }
}

/**
 * Returns the encoded frame.
 *
 * Bytes 0 to 3 are the channel, byte 4 is the type, and bytes 5 to 7 are the length.
 * The numbers are little-endian.
 *
 * @throws ProtocolError if the channel, the type, or the payload length is not valid.
 */
export function encodeFrame(
  channel: number,
  type: FrameType,
  payload: Uint8Array = NO_PAYLOAD,
): Uint8Array<ArrayBuffer> {
  checkHeader(channel, type, payload.byteLength);
  const out = new Uint8Array(HEADER_LEN + payload.byteLength);
  const view = new DataView(out.buffer);
  view.setUint32(0, channel, true);
  view.setUint32(4, (payload.byteLength << 8) | type, true);
  out.set(payload, HEADER_LEN);
  return out;
}

/**
 * Decodes a message that holds one full frame, for example a WebSocket message.
 *
 * @throws ProtocolError if the message is not one valid frame.
 */
export function decodeFrame(message: ArrayBuffer | Uint8Array): Frame {
  const bytes = message instanceof Uint8Array ? message : new Uint8Array(message);
  if (bytes.byteLength < HEADER_LEN) {
    throw new ProtocolError(
      "truncated",
      `The message has ${bytes.byteLength} bytes, but the header has ${HEADER_LEN}.`,
    );
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const channel = view.getUint32(0, true);
  const typeAndLen = view.getUint32(4, true);
  const type = typeAndLen & 0xff;
  const payloadLen = typeAndLen >>> 8;
  checkHeader(channel, type, payloadLen);

  const frameLen = HEADER_LEN + payloadLen;
  if (bytes.byteLength < frameLen) {
    throw new ProtocolError(
      "truncated",
      `The message has ${bytes.byteLength} bytes, but the frame has ${frameLen}.`,
    );
  }
  if (bytes.byteLength > frameLen) {
    throw new ProtocolError(
      "trailing-bytes",
      `The message has ${bytes.byteLength - frameLen} bytes after the frame.`,
    );
  }
  return { channel, type, payload: bytes.subarray(HEADER_LEN) };
}

/**
 * Returns the encoded frame with `message` as the JSON payload.
 *
 * @throws ProtocolError if the channel, the type, or the payload length is not valid.
 */
export function encodeJsonFrame(channel: number, type: FrameType, message: unknown): Uint8Array<ArrayBuffer> {
  return encodeFrame(channel, type, encoder.encode(JSON.stringify(message)));
}

/**
 * Parses the JSON payload of a frame.
 *
 * The function does not examine the shape of the value.
 * The shell trusts the host, which also supplies the shell.
 *
 * @throws ProtocolError with the code `invalid-json` if the payload is not UTF-8 or not JSON.
 */
export function parseJson<T>(frame: Frame): T {
  try {
    return JSON.parse(decoder.decode(frame.payload)) as T;
  } catch (cause) {
    throw new ProtocolError("invalid-json", "The JSON payload is not valid.", { cause });
  }
}

/** Returns the encoded hello frame of the shell for the connection channel. */
export function helloFrame(): Uint8Array<ArrayBuffer> {
  const hello: ConnectionMessage = { type: "hello", version: PROTOCOL_VERSION };
  return encodeJsonFrame(CONNECTION_CHANNEL, FrameType.Control, hello);
}

/** Returns the encoded open frame for `channel`. */
export function openFrame(channel: number, open: Open): Uint8Array<ArrayBuffer> {
  return encodeJsonFrame(channel, FrameType.Open, open);
}

/** Returns the encoded ready frame for `channel`. */
export function readyFrame(channel: number, ready: Ready = {}): Uint8Array<ArrayBuffer> {
  return encodeJsonFrame(channel, FrameType.Ready, ready);
}

/** Returns the encoded data frame for `channel`. */
export function dataFrame(channel: number, payload: Uint8Array): Uint8Array<ArrayBuffer> {
  return encodeFrame(channel, FrameType.Data, payload);
}

/** Returns the encoded close frame for `channel`. */
export function closeFrame(channel: number, close: Close): Uint8Array<ArrayBuffer> {
  return encodeJsonFrame(channel, FrameType.Close, close);
}

/**
 * Gives the channel numbers that the shell can open, in sequence.
 *
 * The shell opens channels with odd numbers.
 * A connection uses each number one time only.
 */
export class ChannelIdAllocator {
  #next = 1;

  /**
   * Returns the next channel number.
   *
   * @throws ProtocolError with the code `invalid-channel` when the shell used all its numbers.
   */
  allocate(): number {
    const id = this.#next;
    if (id > MAX_CHANNEL) {
      throw new ProtocolError("invalid-channel", "The connection has no more channel numbers.");
    }
    this.#next = id + 2;
    return id;
  }
}
