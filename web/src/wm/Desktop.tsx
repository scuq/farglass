import { onCleanup } from "solid-js";
import { APPEARANCE_KEY, applyAppearance, parseAppearance } from "../appearance";
import { Notifications } from "../notify/Notifications";
import { startHostApps } from "../launcher/hostApps";
import { startSurfaceManager, stopSurfaceManager } from "../surface/manager";
import { Panel } from "../panel/Panel";
import { PolkitDialog } from "../polkit/PolkitDialog";
import type { Connection } from "../proto";
import { trackActivity } from "./activity";
import { createDesktopController } from "./controller";
import { Surface } from "./Surface";
import "./wm.css";

/**
 * The desktop: the panel and the window surface.
 *
 * The state of the desktop is in the state store of the agent. A reload gives it
 * back, and each tab of the user shows the same windows.
 */
export function Desktop(props: { connection: Connection; user: string; onlogout: () => void }) {
  const controller = createDesktopController(props.connection);
  // The idle lock of farglassd needs the activity of the user.
  onCleanup(trackActivity(() => props.connection.activity()));
  // The windows of the Wayland apps of the session.
  startSurfaceManager(props.connection, controller);
  onCleanup(() => stopSurfaceManager(props.connection));
  // The desktop entries of the host for the launcher, and the icons of their windows (F031).
  onCleanup(startHostApps(props.connection));
  // The appearance from the settings app, also when a different tab changes it.
  onCleanup(controller.state.watch(APPEARANCE_KEY, (value) => applyAppearance(parseAppearance(value))));
  return (
    <div class="wm">
      <Panel controller={controller} user={props.user} onlogout={props.onlogout} onlock={() => props.connection.lock()} />
      <Surface controller={controller} />
      <Notifications connection={props.connection} />
      <PolkitDialog connection={props.connection} />
    </div>
  );
}
