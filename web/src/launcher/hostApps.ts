/**
 * The apps of the host (F031): the desktop entries that the agent reads.
 *
 * The launcher lists them with the apps of the shell, and a click starts the
 * app on the Wayland compositor of the session. Its window then opens as each
 * surface window does. The icon of a desktop entry also shows on the windows
 * of its app: the Wayland app id of a window names the entry.
 */

import { createSignal, type Signal } from "solid-js";
import { APPS } from "../apps/registry";
import logo from "../brand/farglass.svg";
import type { AppsEvent, AppsRequest, Channel, Connection, HostApp } from "../proto";
import { surfaceManagerFor, surfaceOf } from "../surface/manager";

export class HostApps {
  readonly #channel: Channel;
  readonly #connection: Connection;
  readonly #apps = createSignal<readonly HostApp[]>([]);
  readonly #icons = new Map<string, Signal<string | undefined>>();
  /** The icon whose bytes come in the next data frame. */
  #iconNext: { id: string; mime: string } | null = null;
  readonly #launches = new Map<string, { resolve: () => void; reject: (message: string) => void }[]>();

  constructor(connection: Connection) {
    this.#connection = connection;
    this.#channel = connection.openChannel(
      { kind: "apps" },
      { oncontrol: (message) => this.#event(message as AppsEvent), ondata: (data) => this.#iconData(data) },
    );
  }

  close(): void {
    this.#channel.close();
    for (const [, icon] of this.#icons) {
      const url = icon[0]();
      if (url) URL.revokeObjectURL(url);
    }
    for (const waiting of this.#launches.values()) {
      for (const { reject } of waiting) reject("The connection closed.");
    }
    this.#launches.clear();
  }

  /** The apps of the host, sorted by name. */
  apps(): readonly HostApp[] {
    return this.#apps[0]();
  }

  /** Reads the desktop entries again, for example when the launcher opens. */
  refresh(): void {
    this.#send({ type: "list" });
  }

  /** Starts the app `id`. Resolves when it started. Rejects with a message for the user if it did not. */
  launch(id: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const waiting = this.#launches.get(id) ?? [];
      waiting.push({ resolve, reject });
      this.#launches.set(id, waiting);
      this.#send({ type: "launch", id });
    });
  }

  /** Returns the URL of the icon of the app `id`, or undefined while it loads or if the app has none. The first call asks the agent. */
  icon(id: string): string | undefined {
    let icon = this.#icons.get(id);
    if (!icon) {
      icon = createSignal<string | undefined>(undefined);
      this.#icons.set(id, icon);
      if (this.#apps[0]().find((app) => app.id === id)?.icon) {
        this.#send({ type: "icon", id });
      }
    }
    return icon[0]();
  }

  /** Returns the app of a window with the Wayland app id `appId`. */
  appFor(appId: string | null): HostApp | undefined {
    if (!appId) return undefined;
    const name = appId.toLowerCase();
    return this.#apps[0]().find((app) => app.names.includes(name));
  }

  /** Returns the icon of the app of the surface window `windowId`. */
  iconForWindow(windowId: string): string | undefined {
    const info = surfaceManagerFor(this.#connection)?.surfaces().get(surfaceOf(windowId));
    const app = this.appFor(info?.appId ?? null);
    return app ? this.icon(app.id) : undefined;
  }

  #send(request: AppsRequest): void {
    this.#channel.control(request);
  }

  #event(event: AppsEvent): void {
    switch (event.type) {
      case "list": {
        this.#apps[1](event.apps);
        // An icon that was asked before the list came: ask again now.
        for (const [id, icon] of this.#icons) {
          if (!icon[0]() && event.apps.find((app) => app.id === id)?.icon) this.#send({ type: "icon", id });
        }
        break;
      }
      case "icon":
        this.#iconNext = event.mime ? { id: event.id, mime: event.mime } : null;
        break;
      case "launched":
      case "failed": {
        const waiting = this.#launches.get(event.id)?.shift();
        if (event.type === "launched") waiting?.resolve();
        else waiting?.reject(event.message);
        break;
      }
    }
  }

  #iconData(data: Uint8Array): void {
    const next = this.#iconNext;
    this.#iconNext = null;
    if (!next) return;
    const url = URL.createObjectURL(new Blob([data.slice()], { type: next.mime }));
    let icon = this.#icons.get(next.id);
    if (!icon) {
      icon = createSignal<string | undefined>(undefined);
      this.#icons.set(next.id, icon);
    }
    const old = icon[0]();
    icon[1](url);
    if (old) URL.revokeObjectURL(old);
  }
}

// A plain variable: the desktop sets it before its panel and its windows render.
let active: HostApps | undefined;

/** Returns the apps of the host of the desktop that shows, or undefined with no desktop. */
export function hostApps(): HostApps | undefined {
  return active;
}

/** Starts the apps of the host for `connection`. Returns the function that stops them. */
export function startHostApps(connection: Connection): () => void {
  const apps = new HostApps(connection);
  active = apps;
  return () => {
    apps.close();
    if (active === apps) active = undefined;
  };
}

/** Returns the icon of a window: the icon of the host app for a surface window, else the icon of the shell app, else the farglass icon. */
export function windowIcon(win: { readonly id: string; readonly app: string }): string {
  if (win.app === "linux") {
    const icon = active?.iconForWindow(win.id);
    if (icon) return icon;
  }
  return APPS[win.app]?.icon ?? logo;
}
