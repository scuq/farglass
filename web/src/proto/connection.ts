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
  /** The bytes that wait to go out for `channel`: on a WebSocket, for all channels. A transport with no count gives 0. */
  buffered?(channel: number): number;
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

  /** The bytes of this channel that wait to go out, for a sender that drops late data (F116). */
  get buffered(): number {
    return this.connection.buffered(this.id);
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

/** The answer of the server to a machine PIN (F059). */
export interface PinResult {
  /** Reports whether the PIN is correct. */
  readonly accepted: boolean;
  /** With too many wrong PINs: the seconds until the next try, else 0. */
  readonly retryAfter: number;
}

/** One connection to farglassd. */
export class Connection {
  #state: ConnectionState = "connecting";
  readonly #transport: Transport;
  readonly #ids = new ChannelIdAllocator();
  readonly #channels = new Map<number, Channel>();
  /** Called when the connection stops, with the cause. */
  onclose: ((reason: string) => void) | null = null;
  /** The user of the connection, from the hello message of the server. `null` before a login. */
  user: string | null = null;
  /** The user of a locked session, from the hello message. The connection then has no user until a login. */
  locked: string | null = null;
  /** Reports whether the host asks for its machine PIN before a login (F059). */
  pin = false;
  #pinAnswer: ((result: PinResult) => void) | null = null;
  /** The transport under the connection. */
  transport: "websocket" | "webtransport" = "websocket";
  /** Reports whether the server ended the connection because the user logged out. */
  loggedOut = false;
  /** Reports whether the server ended the connection because the session is locked now. */
  lockedNow = false;
  /** Reports whether the server ended a connection with a locked session because a different tab unlocked it. */
  unlocked = false;

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
        connection.#bytesReceived += message.byteLength;
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

  /** Asks the server to lock the session. The server then ends each connection of the user. */
  lock(): void {
    this.#sendConnection({ type: "lock" });
  }

  /**
   * Gives the machine PIN to the server (F059). Resolves with the answer.
   * After an accepted PIN, the shell can open a login channel.
   */
  sendPin(pin: string): Promise<PinResult> {
    return new Promise((resolve) => {
      this.#pinAnswer = resolve;
      this.#sendConnection({ type: "pin", pin });
    });
  }

  /** Tells the server that the user used the keyboard or the pointer. */
  activity(): void {
    this.#sendConnection({ type: "activity" });
  }

  #sendConnection(message: ConnectionMessage): void {
    this.sendRaw(encodeJsonFrame(CONNECTION_CHANNEL, FrameType.Control, message));
  }

  /** Closes the connection. Each open channel gets a close callback. */
  close(): void {
    this.#transport.close();
    this.#stopped("The shell closed the connection.");
  }

  /** The bytes that wait to go out for channel `channel`. */
  buffered(channel: number): number {
    return this.#transport.buffered?.(channel) ?? 0;
  }

  /** @internal Sends one encoded frame. */
  sendRaw(frame: Uint8Array<ArrayBuffer>): void {
    if (this.#state === "open") {
      this.#bytesSent += frame.byteLength;
      this.#transport.send(frame);
    }
  }

  #bytesSent = 0;
  #bytesReceived = 0;

  /** The bytes of the frames that the shell sent and received on this connection, for the resource score (F046). */
  get bytes(): { readonly sent: number; readonly received: number } {
    return { sent: this.#bytesSent, received: this.#bytesReceived };
  }

  #hello(frame: Frame): void {
    if (frame.channel !== CONNECTION_CHANNEL) {
      throw new ProtocolError("invalid-connection-frame", "The first frame was not a hello message.");
    }
    const hello = parseJson<ConnectionMessage>(frame);
    if (hello.type !== "hello") {
      throw new ProtocolError("invalid-connection-frame", "The first frame was not a hello message.");
    }
    if (hello.version !== PROTOCOL_VERSION) {
      throw new ProtocolError(
        "invalid-connection-frame",
        `The server has protocol version ${hello.version}, the shell has ${PROTOCOL_VERSION}.`,
      );
    }
    this.user = hello.user ?? null;
    this.locked = hello.locked ?? null;
    this.pin = hello.pin ?? false;
    this.#state = "open";
  }

  #receive(frame: Frame): void {
    if (frame.channel === CONNECTION_CHANNEL) {
      const message = parseJson<ConnectionMessage>(frame);
      // The server closes the connection after each of these messages.
      if (message.type === "logged-out") {
        this.loggedOut = true;
        return;
      }
      if (message.type === "locked") {
        this.lockedNow = true;
        return;
      }
      if (message.type === "unlocked") {
        this.unlocked = true;
        return;
      }
      if (message.type === "pin-result") {
        if (message.accepted) {
          this.pin = false;
        }
        const answer = this.#pinAnswer;
        this.#pinAnswer = null;
        answer?.({ accepted: message.accepted, retryAfter: message.retry_after ?? 0 });
        return;
      }
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
    buffered: () => socket.bufferedAmount,
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
