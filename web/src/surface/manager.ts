/**
 * The windows of the Wayland compositor of the session (F024, F025).
 *
 * The manager keeps a surfaces channel open while the desktop shows. A window
 * of the window manager with the id `surface-ID` and the app `linux` shows
 * each top-level window after its first commit. A window of a surface that is
 * gone closes after the channel is in sync.
 */

import { createSignal } from "solid-js";
import type { Channel, Connection, SurfaceEvent, SurfaceInput, SurfaceRect, SurfaceRequest } from "../proto";
import { ClipboardBridge } from "./clipboard";
import { RedirectManager } from "./redirect";

/** The facts of one surface: a top-level window or a popup. */
export interface SurfaceInfo {
  readonly id: number;
  readonly title: string;
  readonly appId: string | null;
  /** For a popup: the surface that it belongs to. For a top-level window: the window that it belongs to, or null. */
  readonly parent: number | null;
  readonly popup: boolean;
  /** The popup is a subsurface, for example a bubble of Chromium: a click outside it does not close it. */
  readonly subsurface: boolean;
  /** For a popup: its place relative to the window geometry of its parent, in logical pixels. */
  readonly place: SurfaceRect | null;
  readonly width: number;
  readonly height: number;
  readonly scale: number;
  /** The window with no shadow, in logical pixels of the surface. */
  readonly geometry: SurfaceRect | null;
  readonly mapped: boolean;
}

/** What the manager needs of the window manager. */
export interface SurfaceWindows {
  /** Opens a window with the id `id` for the app `linux`, if it is not open. `hint` names the app and the window that it belongs to. */
  openSurface(id: string, title: string, w: number, h: number, hint?: { appId?: string | null; parent?: string | null }): void;
  close(id: string): void;
  /** The ids of the open windows of the app `linux`. */
  surfaceWindows(): string[];
  /** Calls `handler` after the desktop came from the state store. */
  whenLoaded(handler: () => void): void;
}

/** The height of the title bar of a window, which the window manager adds to the content. */
const TITLE_BAR = 32;

export const windowId = (surface: number) => `surface-${surface}`;
export const surfaceOf = (window: string) => Number(window.slice("surface-".length));

export class SurfaceManager {
  readonly #channel: Channel;
  readonly #windows: SurfaceWindows;
  // The logic uses a plain map: Solid commits a signal write on the next microtask. The signal is a copy for the view.
  #map = new Map<number, SurfaceInfo>();
  readonly #signal = createSignal<ReadonlyMap<number, SurfaceInfo>>(new Map());
  #loaded = false;
  #synced = false;
  /** The input to the apps goes on its own channel, thus on its own stream with WebTransport (F029). */
  readonly #input: Channel;
  readonly #cursor = createSignal("default");
  readonly #moveHandlers = new Map<number, () => void>();
  /** The clipboard between the apps and the client computer (F030). */
  readonly clipboard: ClipboardBridge;
  /** The videos of the host browser that the client plays (F040). */
  readonly redirect: RedirectManager;

  constructor(connection: Connection, windows: SurfaceWindows) {
    this.#windows = windows;
    this.clipboard = new ClipboardBridge(connection);
    this.redirect = new RedirectManager(connection);
    this.#channel = connection.openChannel({ kind: "surfaces" }, { oncontrol: (message) => this.#event(message as SurfaceEvent) });
    this.#input = connection.openChannel({ kind: "surface-input" });
    windows.whenLoaded(() => {
      this.#loaded = true;
      this.#reconcile();
    });
  }

  /**
   * Opens a window for each top-level window with content, and after the sync, closes
   * each window of a surface that is gone. It waits for the desktop of the state store,
   * which replaces the windows when it comes.
   */
  #reconcile(): void {
    if (!this.#loaded) {
      return;
    }
    const open = new Set(this.#windows.surfaceWindows());
    for (const info of this.#map.values()) {
      if (!info.popup && info.mapped && !open.has(windowId(info.id))) {
        const content = info.geometry ?? { x: 0, y: 0, width: info.width / info.scale, height: info.height / info.scale };
        this.#windows.openSurface(windowId(info.id), info.title || info.appId || "Linux app", content.width, content.height + TITLE_BAR, {
          appId: info.appId,
          parent: info.parent === null ? null : windowId(info.parent),
        });
      }
    }
    if (this.#synced) {
      for (const window of open) {
        if (!this.#map.has(surfaceOf(window))) {
          this.#windows.close(window);
        }
      }
    }
  }

  /** The surfaces, for the view. */
  surfaces(): ReadonlyMap<number, SurfaceInfo> {
    return this.#signal[0]();
  }

  /** Returns the popups of the surface `parent`. */
  popupsOf(parent: number): SurfaceInfo[] {
    return [...this.surfaces().values()].filter((info) => info.popup && info.parent === parent);
  }

  request(request: SurfaceRequest): void {
    this.#channel.control(request);
  }

  /** Sends keyboard or pointer input to the apps. */
  input(input: SurfaceInput): void {
    this.#input.control(input);
  }

  /** Calls `handler` when the app of the window `id` asks to move it, for example after a press on its own title bar. */
  onMoveRequest(id: number, handler: () => void): () => void {
    this.#moveHandlers.set(id, handler);
    return () => this.#moveHandlers.delete(id);
  }

  /** The CSS cursor that the app under the pointer selected. */
  cursor(): string {
    return this.#cursor[0]();
  }

  close(): void {
    this.clipboard.close();
    this.redirect.close();
    this.#channel.close();
    this.#input.close();
  }

  #set(id: number, change: (old: SurfaceInfo) => SurfaceInfo, popup = false): void {
    const old = this.#map.get(id) ?? {
      id,
      title: "",
      appId: null,
      parent: null,
      popup,
      subsurface: false,
      place: null,
      width: 0,
      height: 0,
      scale: 1,
      geometry: null,
      mapped: false,
    };
    this.#map = new Map(this.#map).set(id, change(old));
  }

  #remove(id: number): void {
    this.#map = new Map(this.#map);
    this.#map.delete(id);
  }

  #event(event: SurfaceEvent): void {
    switch (event.type) {
      case "toplevel": {
        this.#set(event.id, (old) => ({ ...old, title: event.title ?? old.title, appId: event.app_id ?? old.appId, parent: event.parent ?? null }));
        break;
      }
      case "popup":
        this.#set(event.id, (old) => ({ ...old, parent: event.parent, place: event.geometry, subsurface: event.subsurface ?? false }), true);
        break;
      case "commit": {
        const known = this.#map.get(event.id);
        this.#set(event.id, (old) => ({
          ...old,
          width: event.width,
          height: event.height,
          scale: event.scale,
          geometry: event.geometry ?? null,
          mapped: true,
        }));
        if (known && !known.popup && !known.mapped) {
          this.#reconcile();
        }
        break;
      }
      case "unmapped":
        this.#set(event.id, (old) => ({ ...old, mapped: false }));
        break;
      case "closed":
      case "popup-closed":
        this.#remove(event.id);
        if (event.type === "closed") {
          this.#windows.close(windowId(event.id));
        }
        break;
      case "synced":
        this.#synced = true;
        this.#reconcile();
        break;
      case "cursor":
        this.#cursor[1](event.name);
        break;
      case "move-request":
        this.#moveHandlers.get(event.id)?.();
        break;
    }
    this.#signal[1](this.#map);
  }
}

const managers = new WeakMap<Connection, SurfaceManager>();

/** Returns the manager of the desktop of `connection`. */
export function surfaceManagerFor(connection: Connection): SurfaceManager | undefined {
  return managers.get(connection);
}

/** Starts the manager of the desktop of `connection`. */
export function startSurfaceManager(connection: Connection, windows: SurfaceWindows): SurfaceManager {
  const manager = new SurfaceManager(connection, windows);
  managers.set(connection, manager);
  return manager;
}

/** Stops the manager of `connection`. */
export function stopSurfaceManager(connection: Connection): void {
  managers.get(connection)?.close();
  managers.delete(connection);
}
