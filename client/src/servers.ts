/**
 * The list of farglass servers of the desktop client (F121).
 *
 * The list is a JSON file in the data folder of the user. Each server has a
 * name, the origin of farglassd, and the pin of its public key after the
 * first connection.
 */
import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

/** The port of farglassd if the address has no port. */
export const DEFAULT_PORT = 8443;

/** One farglass server. */
export interface Server {
  id: string;
  name: string;
  /** The origin of farglassd, for example "https://desk.example:8443". */
  url: string;
  /** The SHA-256 of the public key, from the first connection. */
  pin?: string;
  /** The time of the last connection, in milliseconds since 1970. */
  lastUsed?: number;
}

/** The fields that the user enters. */
export interface ServerInput {
  name: string;
  url: string;
}

/**
 * Returns the origin of the address that the user entered.
 * "desk" becomes "https://desk:8443". A path, a query, or a user name is refused.
 *
 * @throws Error with a message for the user if the address is not valid.
 */
export function normalizeUrl(text: string): string {
  const trimmed = text.trim();
  if (trimmed === "") throw new Error("Enter the address of the server.");
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  let url: URL;
  try {
    url = new URL(withScheme);
  } catch {
    throw new Error(`"${trimmed}" is not a valid address.`);
  }
  if (url.protocol !== "https:") throw new Error("The address must start with https://.");
  if (url.username || url.password) throw new Error("The address must not have a user name or a password.");
  if ((url.pathname !== "/" && url.pathname !== "") || url.search || url.hash) {
    throw new Error("The address must not have a path. Enter only the host and the port.");
  }
  // URL drops the port 443. Any other port stays, and no port gives the port of farglassd.
  const explicitPort = /^https:\/\/[^/]*:\d+\/?$/i.test(withScheme);
  if (!explicitPort) url.port = String(DEFAULT_PORT);
  return url.origin;
}

/** Returns a valid server from `input`, with the id `id`. */
export function makeServer(input: ServerInput, id: string = randomUUID()): Server {
  const url = normalizeUrl(input.url);
  const name = input.name.trim() || new URL(url).hostname;
  return { id, name, url };
}

const isServer = (value: unknown): value is Server => {
  if (typeof value !== "object" || value === null) return false;
  const s = value as Record<string, unknown>;
  return (
    typeof s["id"] === "string" &&
    typeof s["name"] === "string" &&
    typeof s["url"] === "string" &&
    (s["pin"] === undefined || typeof s["pin"] === "string") &&
    (s["lastUsed"] === undefined || typeof s["lastUsed"] === "number")
  );
};

/** The server list in the file `path`. */
export class ServerStore {
  readonly #path: string;
  #servers: Server[];
  #closeToTray: boolean;

  constructor(path: string) {
    this.#path = path;
    const data = ServerStore.#read(path);
    this.#servers = Array.isArray(data.servers) ? data.servers.filter(isServer) : [];
    this.#closeToTray = typeof data.closeToTray === "boolean" ? data.closeToTray : true;
  }

  static #read(path: string): { servers?: unknown; closeToTray?: unknown } {
    try {
      const data: unknown = JSON.parse(readFileSync(path, "utf8"));
      return typeof data === "object" && data !== null ? data : {};
    } catch {
      // No file or a damaged file gives an empty list. The next save replaces it.
      return {};
    }
  }

  /** Reports if a close of the window hides it in the tray. The default is true. */
  get closeToTray(): boolean {
    return this.#closeToTray;
  }

  set closeToTray(value: boolean) {
    this.#closeToTray = value;
    this.#save();
  }

  #save(): void {
    mkdirSync(dirname(this.#path), { recursive: true, mode: 0o700 });
    const tmp = `${this.#path}.tmp`;
    writeFileSync(tmp, JSON.stringify({ servers: this.#servers, closeToTray: this.#closeToTray }, null, 2), { mode: 0o600 });
    renameSync(tmp, this.#path);
  }

  /** Returns the servers, the last used first, then by name. */
  list(): Server[] {
    return [...this.#servers].sort((a, b) => (b.lastUsed ?? 0) - (a.lastUsed ?? 0) || a.name.localeCompare(b.name));
  }

  get(id: string): Server | undefined {
    return this.#servers.find((s) => s.id === id);
  }

  add(input: ServerInput): Server {
    const server = makeServer(input);
    this.#servers.push(server);
    this.#save();
    return server;
  }

  /** Changes the name and the address. A new address forgets the pin. */
  update(id: string, input: ServerInput): Server {
    const old = this.#need(id);
    const next = makeServer(input, id);
    const server: Server = { ...old, name: next.name, url: next.url };
    if (next.url !== old.url) delete server.pin;
    this.#replace(server);
    return server;
  }

  remove(id: string): void {
    this.#servers = this.#servers.filter((s) => s.id !== id);
    this.#save();
  }

  setPin(id: string, pin: string): void {
    this.#replace({ ...this.#need(id), pin });
  }

  forgetPin(id: string): void {
    const server = { ...this.#need(id) };
    delete server.pin;
    this.#replace(server);
  }

  touch(id: string, now: number = Date.now()): void {
    this.#replace({ ...this.#need(id), lastUsed: now });
  }

  #need(id: string): Server {
    const server = this.get(id);
    if (!server) throw new Error("The server is not in the list.");
    return server;
  }

  #replace(server: Server): void {
    this.#servers = this.#servers.map((s) => (s.id === server.id ? server : s));
    this.#save();
  }
}
