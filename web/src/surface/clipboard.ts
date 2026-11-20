/**
 * The clipboard between the Wayland apps and the client computer (F030).
 *
 * A copy in an app comes on the clipboard channel, and the bridge writes it
 * to the clipboard of the browser with the Async Clipboard API. Before a
 * paste key goes to an app, the bridge reads the clipboard of the browser
 * and, if it changed, gives it to the agent. The key waits until the agent
 * stored it. The terminal uses the clipboard of the browser too, thus text
 * goes between the terminal and the apps the same way.
 */

import type { Channel, ClipboardMessage, ClipboardMime, Connection } from "../proto";
import { MAX_CLIPBOARD_LEN } from "../proto";

/** A clipboard content. */
export type ClipContent = { readonly mime: "text/plain"; readonly text: string } | { readonly mime: "image/png"; readonly bytes: Uint8Array };

/** The length of the data frames of a content. */
const CHUNK = 256 * 1024;

/** The longest wait for the agent to store a content: a paste key does not wait longer. */
const STORE_WAIT_MS = 1000;

/** The text of an FNV-1a hash of `bytes`: a short key to compare two images. */
function hash(bytes: Uint8Array): string {
  let h = 0x811c9dc5;
  for (const byte of bytes) {
    h ^= byte;
    h = Math.imul(h, 0x01000193);
  }
  return `${bytes.length}:${(h >>> 0).toString(16)}`;
}

/** Returns a key that is the same for two equal contents. */
export function contentKey(content: ClipContent): string {
  return content.mime === "text/plain" ? `t:${content.text}` : `p:${hash(content.bytes)}`;
}

/** Reads the clipboard of the browser: text first, then a PNG image. `null` if it is empty or the browser refuses. */
async function readBrowser(): Promise<ClipContent | null> {
  const clipboard = navigator.clipboard;
  if (!clipboard) return null;
  if (typeof clipboard.read === "function") {
    try {
      for (const item of await clipboard.read()) {
        if (item.types.includes("text/plain")) {
          return { mime: "text/plain", text: await (await item.getType("text/plain")).text() };
        }
        if (item.types.includes("image/png")) {
          return { mime: "image/png", bytes: new Uint8Array(await (await item.getType("image/png")).arrayBuffer()) };
        }
      }
      return null;
    } catch {
      // Some browsers give text only through readText.
    }
  }
  try {
    return { mime: "text/plain", text: await clipboard.readText() };
  } catch {
    return null;
  }
}

/** Writes a content to the clipboard of the browser. Rejects if the browser refuses, for example with no focus. */
async function writeBrowser(content: ClipContent): Promise<void> {
  if (content.mime === "text/plain") {
    await navigator.clipboard.writeText(content.text);
    return;
  }
  const bytes = content.bytes.slice();
  await navigator.clipboard.write([new ClipboardItem({ "image/png": new Blob([bytes], { type: "image/png" }) })]);
}

/** What the bridge needs of the browser. A test gives its own. */
export interface ClipboardAccess {
  read(): Promise<ClipContent | null>;
  write(content: ClipContent): Promise<void>;
}

const browserAccess: ClipboardAccess = { read: readBrowser, write: writeBrowser };

export class ClipboardBridge {
  readonly #channel: Channel;
  readonly #access: ClipboardAccess;
  #incoming: { mime: ClipboardMime; size: number; parts: Uint8Array[]; got: number } | null = null;
  /** The key of the content that the apps and the browser have both: the bridge does not send it again. */
  #last = "";
  /** A content of an app that the browser refused to write. The next press or key writes it. */
  #pending: ClipContent | null = null;
  #stored: (() => void)[] = [];
  readonly #retry = () => {
    if (this.#pending) void this.#write(this.#pending);
  };
  readonly #focus = () => {
    // With the read permission, a copy on the client computer reaches the apps before a paste key.
    void navigator.permissions
      ?.query({ name: "clipboard-read" as PermissionName })
      .then((status) => {
        if (status.state === "granted") void this.sync();
      })
      .catch(() => {});
  };

  constructor(connection: Connection, access: ClipboardAccess = browserAccess) {
    this.#access = access;
    this.#channel = connection.openChannel(
      { kind: "clipboard" },
      { oncontrol: (message) => this.#control(message as ClipboardMessage), ondata: (data) => this.#data(data) },
    );
    if (access === browserAccess) {
      window.addEventListener("pointerdown", this.#retry, true);
      window.addEventListener("keydown", this.#retry, true);
      window.addEventListener("focus", this.#focus);
    }
  }

  close(): void {
    window.removeEventListener("pointerdown", this.#retry, true);
    window.removeEventListener("keydown", this.#retry, true);
    window.removeEventListener("focus", this.#focus);
    this.#channel.close();
    for (const resolve of this.#stored.splice(0)) resolve();
  }

  /**
   * Gives the clipboard of the browser to the apps if it changed. Resolves after the agent
   * stored it, or after 1 s. It never rejects: a paste with no access pastes the old content.
   */
  async sync(): Promise<void> {
    // A copy in an app that the browser did not take yet is newer than the clipboard of the browser.
    if (this.#pending) {
      await this.#write(this.#pending);
      if (this.#pending) return;
    }
    const content = await this.#access.read().catch(() => null);
    if (!content) return;
    const key = contentKey(content);
    if (key === this.#last) return;
    const bytes = content.mime === "text/plain" ? new TextEncoder().encode(content.text) : content.bytes;
    if (bytes.length > MAX_CLIPBOARD_LEN) return;
    this.#last = key;
    const stored = new Promise<void>((resolve) => {
      this.#stored.push(resolve);
      setTimeout(resolve, STORE_WAIT_MS);
    });
    this.#channel.control({ type: "offer", mime: content.mime, size: bytes.length } satisfies ClipboardMessage);
    for (let offset = 0; offset < bytes.length; offset += CHUNK) {
      this.#channel.send(bytes.subarray(offset, offset + CHUNK));
    }
    await stored;
  }

  #control(message: ClipboardMessage): void {
    if (message.type === "stored") {
      this.#stored.shift()?.();
      return;
    }
    this.#incoming = { mime: message.mime, size: message.size, parts: [], got: 0 };
    this.#complete();
  }

  #data(data: Uint8Array): void {
    if (!this.#incoming) return;
    this.#incoming.parts.push(data.slice());
    this.#incoming.got += data.length;
    this.#complete();
  }

  #complete(): void {
    const incoming = this.#incoming;
    if (!incoming || incoming.got < incoming.size) return;
    this.#incoming = null;
    const bytes = new Uint8Array(incoming.got);
    let offset = 0;
    for (const part of incoming.parts) {
      bytes.set(part, offset);
      offset += part.length;
    }
    const content: ClipContent = incoming.mime === "text/plain" ? { mime: "text/plain", text: new TextDecoder().decode(bytes) } : { mime: "image/png", bytes };
    this.#last = contentKey(content);
    void this.#write(content);
  }

  async #write(content: ClipContent): Promise<void> {
    this.#pending = content;
    try {
      await this.#access.write(content);
      if (this.#pending === content) this.#pending = null;
    } catch {
      // The browser writes only with the focus or after a user action: the next press tries again.
    }
  }
}

/** Reports whether a key event is a paste key of a Linux app: Ctrl+V, Ctrl+Shift+V, or Shift+Insert. On a Mac, also Cmd+V. */
export function isPasteKey(event: KeyboardEvent, mac: boolean): boolean {
  if (event.altKey) return false;
  if (event.code === "KeyV") return event.ctrlKey || (mac && event.metaKey);
  return event.code === "Insert" && event.shiftKey && !event.ctrlKey;
}
