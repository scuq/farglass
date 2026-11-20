/** Test helpers: a transport that the test controls as the server. */

import type { Transport } from "./connection";
import { decodeFrame, type Frame, helloFrame } from "./frame";
import { Connection } from "./connection";
import { CONNECTION_CHANNEL, FrameType, PROTOCOL_VERSION } from "./generated/wire";
import { encodeJsonFrame } from "./frame";

/** A transport that the test controls as the server. */
export class FakeServer implements Transport {
  onmessage: ((message: ArrayBuffer) => void) | null = null;
  onclose: ((reason: string) => void) | null = null;
  readonly received: Frame[] = [];
  closed = false;

  send(message: Uint8Array<ArrayBuffer>): void {
    this.received.push(decodeFrame(message.slice()));
  }

  close(): void {
    this.closed = true;
  }

  /** Sends an encoded frame to the shell. */
  push(frame: Uint8Array<ArrayBuffer>): void {
    this.onmessage?.(frame.slice().buffer);
  }

  /** Returns the frames that the shell sent since the last call. */
  take(): Frame[] {
    return this.received.splice(0);
  }
}

/**
 * Returns a connection on a fake server. `user` goes in the hello message of the server,
 * `locked` names the user of a locked session, and `pin` asks for the machine PIN (F059).
 */
export async function fakeConnection(
  user?: string,
  locked?: string,
  pin = false,
): Promise<{ server: FakeServer; connection: Connection }> {
  const server = new FakeServer();
  const promise = Connection.start(server);
  server.push(
    user === undefined && locked === undefined && !pin
      ? helloFrame()
      : encodeJsonFrame(CONNECTION_CHANNEL, FrameType.Control, {
          type: "hello",
          version: PROTOCOL_VERSION,
          ...(user === undefined ? {} : { user }),
          ...(locked === undefined ? {} : { locked }),
          ...(pin ? { pin: true } : {}),
        }),
  );
  const connection = await promise;
  server.take();
  return { server, connection };
}
