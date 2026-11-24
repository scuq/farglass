import { onCleanup, Show } from "solid-js";
import { APPEARANCE_KEY, applyAppearance, parseAppearance } from "../appearance";
import { Notifications } from "../notify/Notifications";
import { startSound } from "../audio/player";
import { startHostApps } from "../launcher/hostApps";
import { startStats } from "../stats";
import { resolveLayout, xkbOf } from "../keys/layout";
import { KEYBOARD_KEY, parseKeyboardSettings } from "../keys/scheme";
import { startSurfaceManager, stopSurfaceManager } from "../surface/manager";
import { parseVideoMode, VIDEO_KEY } from "../surface/redirect";
import { KeyHints } from "../keys/KeyHints";
import { Panel } from "../panel/Panel";
import { Tour, TOUR_EVENT } from "../tour/Tour";
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
  const surfaces = startSurfaceManager(props.connection, controller);
  // Where the videos of the host browser play, from the settings (F040).
  onCleanup(controller.state.watch(VIDEO_KEY, (value) => surfaces.redirect.setMode(parseVideoMode(value))));
  // The keyboard layout of the Linux apps, from the settings or the keyboard of the client (F094, F095).
  let setting: string | null = null;
  let keymap = "";
  let asked = 0;
  const sendKeymap = async () => {
    if (setting === null) {
      return;
    }
    const ask = ++asked;
    const layout = await resolveLayout(setting);
    // A newer change of the setting or of the focus wins.
    if (ask === asked && layout !== keymap) {
      keymap = layout;
      surfaces.request({ type: "keymap", ...xkbOf(layout) });
    }
  };
  onCleanup(
    controller.state.watch(KEYBOARD_KEY, (value) => {
      setting = parseKeyboardSettings(value).layout;
      void sendKeymap();
    }),
  );
  // The user can change the layout of the operating system while the browser is in the background.
  const onFocus = () => void sendKeymap();
  window.addEventListener("focus", onFocus);
  onCleanup(() => window.removeEventListener("focus", onFocus));
  onCleanup(() => stopSurfaceManager(props.connection));
  // The desktop entries of the host for the launcher, and the icons of their windows (F031).
  onCleanup(startHostApps(props.connection));
  // The tour of the shortcuts starts from the settings (F044).
  const startTour = () => controller.setTourOpen(true);
  window.addEventListener(TOUR_EVENT, startTour);
  onCleanup(() => window.removeEventListener(TOUR_EVENT, startTour));
  // The use of the host by the apps of each window (F045) and by farglass (F046).
  onCleanup(startStats(props.connection));
  // The sound of the apps (F038).
  onCleanup(startSound(props.connection));
  // The appearance from the settings app, also when a different tab changes it.
  onCleanup(controller.state.watch(APPEARANCE_KEY, (value) => applyAppearance(parseAppearance(value))));
  return (
    <div class="wm">
      <Panel controller={controller} user={props.user} onlogout={props.onlogout} onlock={() => props.connection.lock()} />
      <Surface controller={controller} />
      <Notifications
        connection={props.connection}
        hold={{
          enabled: () => controller.focusSettings().holdNotifications,
          busy: () => controller.input.busy(),
          onPause: (handler) => controller.input.onPause(handler),
        }}
      />
      <PolkitDialog connection={props.connection} />
      <KeyHints controller={controller} />
      <Show when={controller.tourOpen()}>
        <Tour controller={controller} />
      </Show>
    </div>
  );
}
