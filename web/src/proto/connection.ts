/**
 * The client side of the channel protocol: one connection with numbered channels.
 *
 * The code uses an interface for the transport. WebSocket is the first transport.
 */

import type { Close, ConnectionMessage, Open } from "./generated/messages";
import { CONNECTION_CHANNEL, FrameType, PROTOCOL_VERSION } from "./generated/wire";
import {
  ChannelIdAllocator,
  closeFrame,
  dataFrame,
  decodeFrame,
  encodeJsonFrame,
  type Frame,
  helloFrame,
  openFrame,
  parseJson,
  ProtocolError,
} from "./frame";

/** A transport that carries one frame in each message. */
export interface Transport {
  send(message: Uint8Array<ArrayBuffer>): void;
  close(): void;
  onmessage: ((message: ArrayBuffer) => void) | null;
  onclose: ((reason: string) => void) | null;
}

/** The time for the server to send its hello message. */
const HELLO_TIMEOUT_MS = 10_000;

/** The callbacks of a channel. */
export interface ChannelHandlers {
  /** A data frame arrived. */
  ondata?: (data: Uint8Array) => void;
  /** A control message arrived. */
  oncontrol?: (message: unknown) => void;
  /** The channel stopped. `close` is the close frame of the peer, or a local cause. */
  onclose?: (close: Close) => void;
}

/** One channel that the shell opened. */
export class Channel {
  /** Resolves when the peer accepts the channel. Rejects with the close payload if it refuses. */
  readonly ready: Promise<void>;
  #resolveReady!: () => void;
  #rejectReady!: (close: Close) => void;
  #closedByShell = false;
  #closedByPeer = false;

  constructor(
    readonly id: number,
    readonly connection: Connection,
    readonly handlers: ChannelHandlers,
  ) {
    this.ready = new Promise((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });
    // A caller that never awaits `ready` must not cause an unhandled rejection.
    this.ready.catch(() => {});
  }

  /** Reports whether the channel can send. */
  get open(): boolean {
    return !this.#closedByShell && !this.#closedByPeer && this.connection.state === "open";
  }

  /** Sends raw bytes. The opener can send before the channel is ready. */
  send(data: Uint8Array): void {
    if (this.open) {
      this.connection.sendRaw(dataFrame(this.id, data));
    }
  }

  /** Sends one control message. */
  control(message: unknown): void {
    if (this.open) {
      this.connection.sendRaw(encodeJsonFrame(this.id, FrameType.Control, message));
    }
  }

  /** Closes the channel. The peer answers with its own close frame. */
  close(close: Close = { reason: "done" }): void {
    if (this.#closedByShell || this.#closedByPeer) {
      return;
    }
    this.#closedByShell = true;
    this.connection.sendRaw(closeFrame(this.id, close));
  }

  /** @internal Handles one frame of this channel. */
  receive(frame: Frame): void {
    if (this.#closedByPeer) {
      return;
    }
    switch (frame.type) {
      case FrameType.Ready:
        this.#resolveReady();
        break;
      case FrameType.Data:
        if (!this.#closedByShell) {
          this.handlers.ondata?.(frame.payload);
        }
        break;
      case FrameType.Control:
        if (!this.#closedByShell) {
          this.handlers.oncontrol?.(parseJson(frame));
        }
        break;
      case FrameType.Close:
        this.#peerClosed(parseJson<Close>(frame));
        break;
      case FrameType.Open:
        throw new ProtocolError("invalid-channel", `The server sent an open frame on channel ${this.id}.`);
    }
  }

  /** @internal The peer or the connection closed the channel. */
  stop(close: Close): void {
    if (!this.#closedByPeer) {
      this.#closedByShell = true;
      this.#peerClosed(close);
    }
  }

  /** Reports whether the two sides closed the channel. */
  get finished(): boolean {
    return this.#closedByShell && this.#closedByPeer;
  }

  #peerClosed(close: Close): void {
    this.#closedByPeer = true;
    this.#rejectReady(close);
    // Answer the close frame of the peer, unless the shell closed first.
    if (!this.#closedByShell) {
      this.#closedByShell = true;
      this.connection.sendRaw(closeFrame(this.id, { reason: "done" }));
    }
    this.handlers.onclose?.(close);
  }
}

/** The state of a connection. */
export type ConnectionState = "connecting" | "open" | "closed";

/** One connection to farglassd. */
export class Connection {
  #state: ConnectionState = "connecting";
  readonly #transport: Transport;
  readonly #ids = new ChannelIdAllocator();
  readonly #channels = new Map<number, Channel>();
  /** Called when the connection stops, with the cause. */
  onclose: ((reason: string) => void) | null = null;

  private constructor(transport: Transport) {
    this.#transport = transport;
  }

  /**
   * Starts the protocol on `transport` and resolves after the hello exchange.
   *
   * @throws ProtocolError if the server has a different protocol version or breaks the protocol.
   */
  static start(transport: Transport): Promise<Connection> {
    const connection = new Connection(transport);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        connection.#fail("The server sent no hello message.");
        reject(new ProtocolError("truncated", "The server sent no hello message."));
      }, HELLO_TIMEOUT_MS);

      transport.onclose = (reason) => {
        clearTimeout(timer);
        connection.#stopped(reason);
        reject(new Error(`The connection closed: ${reason}`));
      };
      transport.onmessage = (message) => {
        try {
          const frame = decodeFrame(message);
          if (connection.#state === "connecting") {
            clearTimeout(timer);
            connection.#hello(frame);
            resolve(connection);
          } else {
            connection.#receive(frame);
          }
        } catch (error) {
          clearTimeout(timer);
          connection.#fail(error instanceof Error ? error.message : String(error));
          reject(error);
        }
      };
      transport.send(helloFrame());
    });
  }

  /** Returns the state of the connection. */
  get state(): ConnectionState {
    return this.#state;
  }

  /** Opens a channel of the kind `open`. The answer of the server resolves `channel.ready`. */
  openChannel(open: Open, handlers: ChannelHandlers = {}): Channel {
    if (this.#state !== "open") {
      throw new Error("The connection is not open.");
    }
    const channel = new Channel(this.#ids.allocate(), this, handlers);
    this.#channels.set(channel.id, channel);
    this.sendRaw(openFrame(channel.id, open));
    return channel;
  }

  /** Closes the connection. Each open channel gets a close callback. */
  close(): void {
    this.#transport.close();
    this.#stopped("The shell closed the connection.");
  }

  /** @internal Sends one encoded frame. */
  sendRaw(frame: Uint8Array<ArrayBuffer>): void {
    if (this.#state === "open") {
      this.#transport.send(frame);
    }
  }

  #hello(frame: Frame): void {
    if (frame.channel !== CONNECTION_CHANNEL) {
      throw new ProtocolError("invalid-connection-frame", "The first frame was not a hello message.");
    }
    const hello = parseJson<ConnectionMessage>(frame);
    if (hello.type !== "hello" || hello.version !== PROTOCOL_VERSION) {
      throw new ProtocolError(
        "invalid-connection-frame",
        `The server has protocol version ${hello.version}, the shell has ${PROTOCOL_VERSION}.`,
      );
    }
    this.#state = "open";
  }

  #receive(frame: Frame): void {
    if (frame.channel === CONNECTION_CHANNEL) {
      throw new ProtocolError("invalid-connection-frame", "The server sent a second hello message.");
    }
    const channel = this.#channels.get(frame.channel);
    if (!channel) {
      // A frame of a channel that is closed on the two sides.
      return;
    }
    channel.receive(frame);
    if (channel.finished) {
      this.#channels.delete(channel.id);
    }
  }

  #fail(reason: string): void {
    this.#transport.close();
    this.#stopped(reason);
  }

  #stopped(reason: string): void {
    if (this.#state === "closed") {
      return;
    }
    this.#state = "closed";
    const close: Close = { reason: "cancelled", message: reason };
    for (const channel of this.#channels.values()) {
      channel.stop(close);
    }
    this.#channels.clear();
    this.onclose?.(reason);
  }
}

/** Returns a transport on a WebSocket. */
export function webSocketTransport(socket: WebSocket): Transport {
  socket.binaryType = "arraybuffer";
  const transport: Transport = {
    send: (message) => socket.send(message),
    close: () => socket.close(1000),
    onmessage: null,
    onclose: null,
  };
  socket.addEventListener("message", (event) => {
    if (event.data instanceof ArrayBuffer) {
      transport.onmessage?.(event.data);
    }
  });
  socket.addEventListener("close", (event) => {
    transport.onclose?.(event.reason || `WebSocket close code ${event.code}`);
  });
  return transport;
}

/**
 * Connects to the WebSocket endpoint of farglassd at `url`.
 *
 * @throws Error if the WebSocket does not open or the hello exchange fails.
 */
export function connect(url: string | URL): Promise<Connection> {
  const socket = new WebSocket(url);
  return new Promise((resolve, reject) => {
    socket.addEventListener("open", () => {
      Connection.start(webSocketTransport(socket)).then(resolve, reject);
    });
    socket.addEventListener("close", (event) => {
      reject(new Error(event.reason || `The WebSocket closed with code ${event.code}.`));
    });
  });
}

/** Returns the URL of the WebSocket endpoint on the host that served the shell. */
export function endpointUrl(location: Location = window.location): URL {
  const url = new URL("/ws", location.href);
  url.protocol = location.protocol === "http:" ? "ws:" : "wss:";
  return url;
}
