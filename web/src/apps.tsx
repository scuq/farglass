/**
 * The apps of the shell. F013 makes each app an ES module that loads on first
 * use. Until then, this table names the apps.
 */

import type { JSX } from "@solidjs/web";
import { Echo } from "./Echo";
import type { Connection } from "./proto";

/** What an app gets to draw a window. */
export interface AppContext {
  readonly connection: Connection;
  /** The id of the window. It stays the same after a reload, thus an app can keep state under it. */
  readonly windowId: string;
}

/** One app. */
export interface App {
  readonly id: string;
  readonly title: string;
  /** The size of a new window. */
  readonly size: { readonly w: number; readonly h: number };
  readonly render: (context: AppContext) => JSX.Element;
}

/** The apps, by id. */
export const APPS: Readonly<Record<string, App>> = {
  echo: {
    id: "echo",
    title: "Echo",
    size: { w: 520, h: 380 },
    render: (context) => <Echo connection={context.connection} stateKey={`echo.log.${context.windowId}`} />,
  },
};
