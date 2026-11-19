/**
 * The D-Bus client of the shell: method calls, properties, and signals of the
 * host, through a dbus channel to the agent. The agent permits only the
 * services of its allowlist.
 */

import type { Bus, Channel, Connection, DbusEvent, DbusRequest } from "./proto";

/** An error of a D-Bus call, with its D-Bus error name. */
export class DbusError extends Error {
  override readonly name = "DbusError";

  constructor(
    /** The D-Bus error name, for example `org.freedesktop.DBus.Error.AccessDenied`. */
    readonly dbusName: string,
    message: string,
  ) {
    super(message || dbusName);
  }
}

/** A D-Bus variant in the JSON form of the channel. */
export interface Variant {
  readonly type: string;
  readonly value: unknown;
}

/** The signals that a subscription gets. Each field that is not set matches all. */
export interface SignalFilter {
  readonly sender: string;
  readonly path?: string;
  readonly interface?: string;
  readonly member?: string;
}

/** One signal. */
export interface Signal {
  readonly path: string;
  readonly interface: string;
  readonly member: string;
  readonly values: readonly unknown[];
}

type Pending = { resolve: (values: unknown[]) => void; reject: (error: Error) => void };

/** A connection to one bus of the host. */
export class DbusClient {
  readonly #channel: Channel;
  readonly #pending = new Map<number, Pending>();
  readonly #signals = new Map<number, (signal: Signal) => void>();
  #nextId = 1;

  constructor(connection: Connection, bus: Bus) {
    this.#channel = connection.openChannel(
      { kind: "dbus", bus },
      {
        oncontrol: (message) => this.#event(message as DbusEvent),
        onclose: (close) => {
          const error = new DbusError("org.freedesktop.DBus.Error.Disconnected", close.message ?? close.reason);
          for (const pending of this.#pending.values()) {
            pending.reject(error);
          }
          this.#pending.clear();
          this.#signals.clear();
        },
      },
    );
  }

  /**
   * Calls a method. `signature` is the D-Bus signature of `args`, for example `"sb"`.
   * With `interactive`, the service can ask polkit for a password in the shell.
   *
   * @returns The return values in the JSON form.
   * @throws DbusError with the D-Bus error name of the service or of the agent.
   */
  call(
    destination: string,
    path: string,
    iface: string,
    method: string,
    signature = "",
    args: readonly unknown[] = [],
    options: { interactive?: boolean } = {},
  ): Promise<unknown[]> {
    const id = this.#nextId++;
    const request: DbusRequest = {
      type: "call",
      id,
      destination,
      path,
      interface: iface,
      method,
      signature,
      args: [...args],
      interactive: options.interactive ?? false,
    };
    return this.#send(id, request);
  }

  /** Returns the value of a property, with no variant around it. */
  async getProperty(destination: string, path: string, iface: string, name: string): Promise<unknown> {
    const [variant] = await this.call(destination, path, "org.freedesktop.DBus.Properties", "Get", "ss", [iface, name]);
    return (variant as Variant).value;
  }

  /** Returns all properties of an interface, with no variants around them. */
  async getAllProperties(destination: string, path: string, iface: string): Promise<Record<string, unknown>> {
    const [all] = await this.call(destination, path, "org.freedesktop.DBus.Properties", "GetAll", "s", [iface]);
    return Object.fromEntries(Object.entries(all as Record<string, Variant>).map(([key, variant]) => [key, variant.value]));
  }

  /** Sets a property. `type` is the D-Bus signature of the value, for example `"s"`. */
  async setProperty(
    destination: string,
    path: string,
    iface: string,
    name: string,
    type: string,
    value: unknown,
    options: { interactive?: boolean } = {},
  ): Promise<void> {
    const variant: Variant = { type, value };
    await this.call(destination, path, "org.freedesktop.DBus.Properties", "Set", "ssv", [iface, name, variant], options);
  }

  /**
   * Subscribes to signals. `handler` gets each signal that matches `filter`.
   *
   * @returns A function that ends the subscription.
   * @throws DbusError if the agent refuses the sender.
   */
  async subscribe(filter: SignalFilter, handler: (signal: Signal) => void): Promise<() => void> {
    const id = this.#nextId++;
    this.#signals.set(id, handler);
    const request: DbusRequest = { type: "subscribe", id, ...filter };
    try {
      await this.#send(id, request);
    } catch (error) {
      this.#signals.delete(id);
      throw error;
    }
    return () => {
      if (this.#signals.delete(id)) {
        const unsubscribe: DbusRequest = { type: "unsubscribe", id: this.#nextId++ };
        // The unsubscribe uses the number of the subscription.
        this.#channel.control({ ...unsubscribe, id });
      }
    };
  }

  /** Closes the channel. The subscriptions end. */
  close(): void {
    this.#channel.close();
  }

  #send(id: number, request: DbusRequest): Promise<unknown[]> {
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#channel.control(request);
    });
  }

  #event(event: DbusEvent): void {
    switch (event.type) {
      case "reply":
        this.#pending.get(event.id)?.resolve(event.values);
        this.#pending.delete(event.id);
        break;
      case "error":
        this.#pending.get(event.id)?.reject(new DbusError(event.name, event.message));
        this.#pending.delete(event.id);
        break;
      case "signal":
        this.#signals.get(event.subscription)?.({
          path: event.path,
          interface: event.interface,
          member: event.member,
          values: event.values,
        });
        break;
    }
  }
}
