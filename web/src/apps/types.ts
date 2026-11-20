/**
 * The app interface: what an app registers, and what it gets to draw a window.
 *
 * An app is a folder in `src/apps/` with two files:
 *
 * - `manifest.ts` exports `manifest`: the name, the icon, and the start function.
 *   The shell loads each manifest at its start, thus a manifest is small.
 * - The module that `start` imports. Its default export is the component of the
 *   window. The shell loads it when the first window of the app opens.
 */

import type { JSX } from "@solidjs/web";
import type { MenuProvider } from "../menu/menu";
import type { Channel, ChannelHandlers, Connection, Open } from "../proto";

/** The state store of the session, for an app. */
export interface AppState {
  /**
   * Asks for the value of `key`. `onvalue` gets the current value, or `null`,
   * and each change that a different tab makes. The watch stops when the window closes.
   */
  watch(key: string, onvalue: (value: unknown) => void): void;
  /** Sets the value of `key`. `null` removes the key. */
  set(key: string, value: unknown): void;
  /**
   * Returns a key for the state of this window, for example `echo.log.<window id>`.
   * The key stays the same after a reload. Closing the window removes its keys.
   */
  key(name: string): string;
  /**
   * Asks for the value of a key of the session, for example `appearance`.
   * Closing the window does not remove the key.
   */
  watchShared(key: string, onvalue: (value: unknown) => void): void;
  /** Sets the value of a key of the session. Each watcher, also in this tab, gets the value. */
  setShared(key: string, value: unknown): void;
}

/** What an app gets to draw one window. */
export interface AppContext {
  /** The id of the window. It stays the same after a reload. */
  readonly windowId: string;
  /** The user of the session. */
  readonly user: string | null;
  /** The connection to the host. Use it for clients that open their own channels, for example DbusClient. */
  readonly connection: Connection;
  /** Opens a channel to the agent. Closing the window closes the channel. */
  openChannel(open: Open, handlers?: ChannelHandlers): Channel;
  /** The state store of the session. */
  readonly state: AppState;
  /** Changes the title of the window. */
  setTitle(title: string): void;
  /** Opens a new window of the app `appId`. */
  openApp(appId: string): void;
  /**
   * Calls `handler` when the user closes the window. A reload or a reconnect does
   * not close the window, thus it does not call `handler`.
   */
  onClose(handler: () => void): void;
  /** Calls `handler` when the window gets the focus, for example to focus an input element. */
  onFocus(handler: () => void): void;
  /** Reports whether the window has the focus now. */
  isFocused(): boolean;
  /** Sets the context menu of an element of the app. Closing the window removes it. */
  contextMenu(element: Element, provider: MenuProvider): void;
  /** The windows of the desktop: the id, the title, and the app. It is reactive. Optional for older hosts. */
  windows?(): readonly { readonly id: string; readonly title: string; readonly app: string }[];
  /** Gives a window the focus, also on a different workspace. */
  activateWindow?(id: string): void;
}

/** The component of an app window: the default export of the app module. */
export type AppComponent = (props: { context: AppContext }) => JSX.Element;

/** What an app registers in its manifest. */
export interface AppManifest {
  /** The id. It must be the name of the folder of the app. */
  readonly id: string;
  /** The name of the app, and the title of its first window. */
  readonly title: string;
  /** One line for the launcher. */
  readonly description?: string;
  /** More words that the search of the launcher finds. */
  readonly keywords?: readonly string[];
  /** The URL of the icon, from `import icon from "./icon.svg"`. */
  readonly icon: string;
  /** The size of a new window. */
  readonly size: { readonly w: number; readonly h: number };
  /** Reports whether the launcher and the menus leave the app out, for example the windows of Wayland apps. */
  readonly hidden?: boolean;
  /** Loads the app module. Use `() => import("./Module")`. */
  readonly start: () => Promise<{ default: AppComponent }>;
}

/** Returns `manifest`. It only gives type checks to the manifest of an app. */
export function defineApp(manifest: AppManifest): AppManifest {
  return manifest;
}
