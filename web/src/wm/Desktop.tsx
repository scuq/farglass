import { Notifications } from "../notify/Notifications";
import { Panel } from "../panel/Panel";
import { PolkitDialog } from "../polkit/PolkitDialog";
import type { Connection } from "../proto";
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
  return (
    <div class="wm">
      <Panel controller={controller} user={props.user} onlogout={props.onlogout} />
      <Surface controller={controller} />
      <Notifications connection={props.connection} />
      <PolkitDialog connection={props.connection} />
    </div>
  );
}
