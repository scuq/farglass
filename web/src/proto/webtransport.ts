/**
 * The channel protocol on WebTransport (F023): each channel has its own
 * bidirectional stream, thus a late frame of one channel does not stop the
 * frames of the other channels. The frames are the same as on the WebSocket.
 *
 * The shell opens the stream of a channel with the first frame of the
 * channel: the hello message for the connection channel, the open frame for
 * each other channel. After its close frame, the shell ends the stream.
 */

import { Connection, connect, endpointUrl, type Transport } from "./connection";
import { FrameType, HEADER_LEN } from "./generated/wire";

/** The time for the WebTransport session to open. After it, the shell uses the WebSocket. */
export const WEBTRANSPORT_TIMEOUT_MS = 3000;

/** After a failure, the shell uses the WebSocket for this time before it tries WebTransport again. */
export const WEBTRANSPORT_RETRY_MS = 5 * 60_000;

/** The answer of `POST /transport`. */
export interface TransportOffer {
  readonly port: number;
  readonly path: string;
  readonly ticket: string;
  /** The SHA-256 hashes of a self-signed certificate, in hexadecimal. Empty for a certificate of the administrator. */
  readonly hashes: readonly string[];
}

/** Returns a frame reader: it takes the chunks of a stream and gives each complete frame. */
export function frameReader(onframe: (frame: Uint8Array<ArrayBuffer>) => void): (chunk: Uint8Array) => void {
  let pending = new Uint8Array(0);
  return (chunk) => {
    let bytes: Uint8Array;
    if (pending.byteLength === 0) {
      bytes = chunk;
    } else {
      bytes = new Uint8Array(pending.byteLength + chunk.byteLength);
      bytes.set(pending);
      bytes.set(chunk, pending.byteLength);
    }
    let offset = 0;
    while (bytes.byteLength - offset >= HEADER_LEN) {
      const length = bytes[offset + 5]! | (bytes[offset + 6]! << 8) | (bytes[offset + 7]! << 16);
      const end = offset + HEADER_LEN + length;
      if (bytes.byteLength < end) {
        break;
      }
      onframe(bytes.slice(offset, end));
      offset = end;
    }
    pending = bytes.slice(offset);
  };
}

/** A transport on a WebTransport session, with one stream for each channel. */
export class WebTransportTransport implements Transport {
  readonly #session: WebTransport;
  /** The writer of each channel, in the order of the frames: each frame waits for the one before it. */
  readonly #queues = new Map<number, Promise<WritableStreamDefaultWriter<Uint8Array> | null>>();
  #closed = false;
  onmessage: ((message: ArrayBuffer) => void) | null = null;
  onclose: ((reason: string) => void) | null = null;

  constructor(session: WebTransport) {
    this.#session = session;
    session.closed.then(
      (info) => this.#stop(info.reason || "The WebTransport session closed."),
      (error: unknown) => this.#stop(error instanceof Error ? error.message : String(error)),
    );
  }

  send(message: Uint8Array<ArrayBuffer>): void {
    if (this.#closed) {
      return;
    }
    const channel = new DataView(message.buffer, message.byteOffset, message.byteLength).getUint32(0, true);
    const type = message[4];
    const before = this.#queues.get(channel) ?? this.#open();
    const after = before.then(async (writer) => {
      if (!writer) {
        return null;
      }
      await writer.write(message);
      if (type === FrameType.Close) {
        await writer.close();
        return null;
      }
      return writer;
    });
    after.catch((error: unknown) => this.#fail(error));
    if (type === FrameType.Close) {
      this.#queues.delete(channel);
    } else {
      this.#queues.set(channel, after.catch(() => null));
    }
  }

  close(): void {
    if (!this.#closed) {
      this.#session.close({ closeCode: 0, reason: "The shell closed the connection." });
    }
  }

  /** Opens the stream of a new channel and reads its frames. */
  async #open(): Promise<WritableStreamDefaultWriter<Uint8Array>> {
    const stream = await this.#session.createBidirectionalStream();
    void this.#read(stream.readable);
    return stream.writable.getWriter();
  }

  async #read(readable: ReadableStream<Uint8Array>): Promise<void> {
    const reader = readable.getReader();
    const feed = frameReader((frame) => this.onmessage?.(frame.buffer));
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) {
          return;
        }
        feed(value);
      }
    } catch {
      // The stream stopped with the session. The session reports the cause.
    }
  }

  #fail(error: unknown): void {
    if (!this.#closed) {
      this.#session.close({ closeCode: 1, reason: error instanceof Error ? error.message : String(error) });
    }
  }

  #stop(reason: string): void {
    if (this.#closed) {
      return;
    }
    this.#closed = true;
    this.#queues.clear();
    this.onclose?.(reason);
  }
}

const hexBytes = (hex: string) => new Uint8Array(hex.match(/../g)?.map((pair) => Number.parseInt(pair, 16)) ?? []);

/** Returns the URL of the WebTransport endpoint for `offer` on the host that served the shell. */
export function webTransportUrl(offer: TransportOffer, location: Location = window.location): URL {
  const url = new URL(offer.path, location.href);
  url.protocol = "https:";
  url.port = String(offer.port);
  url.search = new URLSearchParams({ ticket: offer.ticket }).toString();
  return url;
}

let failedAt = Number.NEGATIVE_INFINITY;

/**
 * Marks WebTransport as failed: the next connections use the WebSocket for
 * {@link WEBTRANSPORT_RETRY_MS}. For a session that opened and then ended with no
 * cause from farglassd, for example a network that drops QUIC after the handshake.
 */
export function markWebTransportFailed(): void {
  failedAt = performance.now();
}

/**
 * Connects through WebTransport.
 *
 * @throws Error if the browser has no WebTransport, the host has no HTTP/3, or the session does not open in time.
 */
export async function connectWebTransport(fetcher: typeof fetch = fetch): Promise<Connection> {
  if (typeof WebTransport !== "function") {
    throw new Error("The browser has no WebTransport.");
  }
  const response = await fetcher("/transport", { method: "POST", credentials: "same-origin" });
  if (!response.ok) {
    throw new Error(`The host offers no WebTransport (${response.status}).`);
  }
  const offer = (await response.json()) as TransportOffer;
  // The QUIC endpoint has the port of farglassd. Behind a proxy with a different port, the
  // Content Security Policy and the Origin check refuse the session, thus the WebSocket stays.
  const pagePort = Number(window.location.port || "443");
  if (offer.port !== pagePort) {
    throw new Error(`The page has port ${pagePort}, and WebTransport has port ${offer.port}.`);
  }
  const options: WebTransportOptions =
    offer.hashes.length > 0 ? { serverCertificateHashes: offer.hashes.map((hash) => ({ algorithm: "sha-256", value: hexBytes(hash) })) } : {};
  const session = new WebTransport(webTransportUrl(offer), options);
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      session.ready,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error("The WebTransport session did not open in time.")), WEBTRANSPORT_TIMEOUT_MS);
      }),
    ]);
  } catch (error) {
    session.close();
    throw error;
  } finally {
    clearTimeout(timer);
  }
  const connection = await Connection.start(new WebTransportTransport(session));
  connection.transport = "webtransport";
  return connection;
}

/**
 * Connects through WebTransport if the browser and the host have it, and else through the WebSocket.
 * After a WebTransport failure, the shell uses the WebSocket for {@link WEBTRANSPORT_RETRY_MS}.
 */
export async function connectBest(): Promise<Connection> {
  if (typeof WebTransport === "function" && performance.now() - failedAt > WEBTRANSPORT_RETRY_MS) {
    try {
      return await connectWebTransport();
    } catch (error) {
      failedAt = performance.now();
      console.info(`farglass uses the WebSocket: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  return connect(endpointUrl());
}
