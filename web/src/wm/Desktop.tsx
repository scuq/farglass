import { createSignal, onCleanup, Show } from "solid-js";
import { APPEARANCE_KEY, applyAppearance, parseAppearance } from "../appearance";
import { Notifications } from "../notify/Notifications";
import { startSound } from "../audio/player";
import { startBookmarks } from "../launcher/bookmarks";
import { startHostApps } from "../launcher/hostApps";
import { startStats } from "../stats";
import { resolveLayout, xkbOf } from "../keys/layout";
import { DEFAULT_KEYBOARD, KEYBOARD_KEY, parseKeyboardSettings, withLayout } from "../keys/scheme";
import { startSurfaceManager, stopSurfaceManager } from "../surface/manager";
import { parseVideoMode, VIDEO_KEY } from "../surface/redirect";
import { KeyHints } from "../keys/KeyHints";
import { Panel } from "../panel/Panel";
import { startTray } from "../panel/tray";
import { Tour, TOUR_EVENT } from "../tour/Tour";
import { PolkitDialog } from "../polkit/PolkitDialog";
import type { Connection } from "../proto";
import { trackActivity } from "./activity";
import { guardUnload, startLockGuard } from "./guard";
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
  // The browser asks before Ctrl+W or a reload closes the desktop, and fullscreen locks the keyboard (F096).
  onCleanup(guardUnload());
  onCleanup(startLockGuard());
  // The idle lock of farglassd needs the activity of the user.
  onCleanup(trackActivity(() => props.connection.activity()));
  // The windows of the Wayland apps of the session.
  const surfaces = startSurfaceManager(props.connection, controller);
  // Where the videos of the host browser play, from the settings (F040).
  onCleanup(controller.state.watch(VIDEO_KEY, (value) => surfaces.redirect.setMode(parseVideoMode(value))));
  // The keyboard layout of the Linux apps, from the settings or the keyboard of the client (F094, F095).
  // The panel shows the layout and switches it (F097).
  let setting: string | null = null;
  let keymap = "";
  let asked = 0;
  const [keyboard, setKeyboard] = createSignal(DEFAULT_KEYBOARD);
  const [layout, setLayout] = createSignal<string | null>(null);
  const sendKeymap = async () => {
    if (setting === null) {
      return;
    }
    const ask = ++asked;
    const next = await resolveLayout(setting);
    // A newer change of the setting or of the focus wins.
    if (ask !== asked) {
      return;
    }
    setLayout(next);
    if (next !== keymap) {
      keymap = next;
      surfaces.request({ type: "keymap", ...xkbOf(next) });
    }
  };
  onCleanup(
    controller.state.watch(KEYBOARD_KEY, (value) => {
      const settings = parseKeyboardSettings(value);
      setKeyboard(settings);
      setting = settings.layout;
      void sendKeymap();
    }),
  );
  // The user can change the layout of the operating system while the browser is in the background.
  const onFocus = () => void sendKeymap();
  window.addEventListener("focus", onFocus);
  onCleanup(() => window.removeEventListener("focus", onFocus));
  // The browser tells no change of the layout: with Automatic, ask again while the window has the focus.
  const poll = setInterval(() => {
    if (setting === "auto" && document.hasFocus()) void sendKeymap();
  }, 3000);
  onCleanup(() => clearInterval(poll));
  const selectLayout = (next: "auto" | string) => controller.state.set(KEYBOARD_KEY, withLayout(keyboard(), next), { local: true });
  onCleanup(() => stopSurfaceManager(props.connection));
  // The desktop entries of the host for the launcher, and the icons of their windows (F031).
  onCleanup(startHostApps(props.connection));
  // The bookmarks file for the bookmarks app and the launcher (F100).
  onCleanup(startBookmarks(props.connection));
  // The system tray: the items of the programs that stay in a tray (F102).
  onCleanup(startTray(props.connection));
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
      <Panel controller={controller} user={props.user} onlogout={props.onlogout} onlock={() => props.connection.lock()} keyboard={{ settings: keyboard, layout, select: selectLayout }} />
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
