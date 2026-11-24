import { createSignal, For, onCleanup, Show, untrack } from "solid-js";
import { setContextMenu } from "../menu/menu";
import { windowMenu } from "../wm/menus";
import { soundPlayer } from "../audio/player";
import { isShellIcon, windowIcon } from "../launcher/hostApps";
import { EMPTY_LAUNCHER, LAUNCHER_KEY, type LauncherState, parseLauncherState } from "../launcher/usage";
import { describeUsage, isBusy, statsClient } from "../stats";
import type { DesktopController } from "../wm/controller";
import type { Win } from "../wm/model";
import { enterLockedFullscreen, exitLockedFullscreen, lockedFullscreen } from "../wm/guard";
import { APPEARANCE_KEY, parseAppearance } from "../appearance";
import { CLOCK_KEY, DEFAULT_CLOCK, parseClockSettings } from "../clock";
import { Clock } from "./Clock";
import { Dock } from "./Dock";
import { KeyboardIndicator, type KeyboardState } from "./KeyboardIndicator";
import { Launcher } from "./Launcher";
import { Popover } from "./Popover";
import "./panel.css";

/**
 * The panel at the top: the launcher, the workspaces, the open windows of the
 * workspace, the clock, and the session menu.
 */
export function Panel(props: { controller: DesktopController; user: string; onlogout: () => void; onlock: () => void; keyboard?: KeyboardState }) {
  const c = props.controller;
  const [sessionOpen, setSessionOpen] = createSignal(false);
  // The pins and the use of the apps in the launcher (F076). The panel keeps them: the launcher has them at once when it opens.
  const [launcherState, setLauncherState] = createSignal<LauncherState>(EMPTY_LAUNCHER);
  onCleanup(c.state.watch(LAUNCHER_KEY, (stored) => setLauncherState(parseLauncherState(stored))));
  const saveLauncherState = (next: LauncherState) => {
    setLauncherState(next);
    c.state.set(LAUNCHER_KEY, next);
  };
  // The layout of the panel (F098): the bar, or the dock on the left with a slim bar.
  const [dock, setDock] = createSignal(false);
  onCleanup(c.state.watch(APPEARANCE_KEY, (stored) => setDock(parseAppearance(stored).panel === "dock")));
  // The hour format and the crockford timestamp of the clock (F099).
  const [clock, setClock] = createSignal(DEFAULT_CLOCK);
  onCleanup(c.state.watch(CLOCK_KEY, (stored) => setClock(parseClockSettings(stored))));

  // A minimized window shows how busy its app is, and the details in its tooltip (F045).
  // The dot is for minimized windows only: an open window shows what its app does. The tooltip is for each window.
  const usage = (win: Win) => statsClient()?.usageOf(win.id);
  const activity = (win: Win) => {
    const each = win.minimized ? usage(win) : undefined;
    return each ? (isBusy(each) ? "busy" : "idle") : undefined;
  };
  const tip = (win: Win) => {
    const each = usage(win);
    if (each) return `${win.title}: ${isBusy(each) ? "busy" : "idle"}, ${describeUsage(each)}`;
    return win.attention ? "Opened in the background" : undefined;
  };

  const tasks = () =>
    c
      .desktop()
      .windows.filter((win) => win.workspace === c.desktop().workspace)
      // The stack order changes with each focus. The title order stays.
      .toSorted((a, b) => a.title.localeCompare(b.title, undefined, { numeric: true }));

  return (
    <>
    <header class="panel">
      <button
        type="button"
        class="panel-apps"
        aria-haspopup="dialog"
        aria-expanded={String(c.launcherOpen()) as "true" | "false"}
        aria-controls="launcher"
        onClick={() => c.setLauncherOpen(!c.launcherOpen())}
        title={`Open the launcher (${c.hint("launcher")})`}
      >
        Apps
      </button>
      <Popover id="launcher" class="panel-launcher" label="Launcher" open={c.launcherOpen()} onClose={() => c.setLauncherOpen(false)}>
        <Launcher
          onopen={(appId) => {
            c.setLauncherOpen(false);
            c.openApp(appId);
          }}
          onclose={() => c.setLauncherOpen(false)}
          onlaunch={(names) => c.expectLaunch(names)}
          onsetting={(page) => {
            c.setLauncherOpen(false);
            c.openApp("settings", { page });
          }}
          usage={launcherState()}
          onusage={saveLauncherState}
        />
      </Popover>

      <nav class="panel-workspaces" aria-label="Workspaces">
        <For each={Array.from({ length: c.desktop().workspaces }, (_, i) => i)}>
          {(index) => (
            <button
              type="button"
              aria-pressed={String(c.desktop().workspace === index) as "true" | "false"}
              aria-label={`Workspace ${index + 1}`}
              onClick={() => c.switchWorkspace(index)}
            >
              {index + 1}
            </button>
          )}
        </For>
      </nav>

      {/* With the dock, the list is empty: it stays as the space between the left and the right side of the bar. */}
      <nav class="panel-tasks" aria-label="Windows">
        <For each={dock() ? [] : tasks()} keyed={(win: Win) => win.id}>
          {(win) => {
            const id = untrack(() => win().id);
            let removeMenu: (() => void) | undefined;
            onCleanup(() => removeMenu?.());
            return (
            <button
              type="button"
              class="wm-task"
              ref={(el) => {
                removeMenu = setContextMenu(el, () => windowMenu(c, id));
              }}
              aria-pressed={String(c.focusedId() === win().id) as "true" | "false"}
              data-minimized={String(win().minimized)}
              data-attention={win().attention ? "true" : undefined}
              data-activity={activity(win())}
              title={tip(win())}
              onClick={() => (c.focusedId() === win().id ? c.minimize(win().id) : c.activate(win().id))}
            >
              {windowIcon(win()) ? <img class="panel-task-icon" src={windowIcon(win())} alt="" data-shell-icon={isShellIcon(windowIcon(win())) ? "true" : undefined} /> : null}
              {win().title}
            </button>
            );
          }}
        </For>
      </nav>

      <Show when={soundPlayer()?.available()}>
        <button
          type="button"
          class="panel-sound"
          aria-pressed={String(soundPlayer()?.muted() ?? false) as "true" | "false"}
          data-packets={soundPlayer()?.packets()}
          data-playing={String(soundPlayer()?.state()?.playing ?? false)}
          data-level={(soundPlayer()?.state()?.level ?? 0).toFixed(2)}
          title="Turn the sound of the apps off or on"
          onClick={() => soundPlayer()?.setMuted(!soundPlayer()?.muted())}
        >
          {soundPlayer()?.muted() ? "Sound off" : "Sound"}
        </button>
      </Show>
      <button type="button" class="panel-overview" aria-pressed={String(c.overviewOpen()) as "true" | "false"} onClick={() => c.setOverviewOpen(!c.overviewOpen())} title={`Show all workspaces and windows (${c.hint("overview")})`}>
        Overview
      </button>
      <button type="button" class="panel-tile" aria-haspopup="dialog" onClick={() => c.setLayoutsOpen(!c.layoutsOpen())} title={`Select a layout for the windows of the workspace (${c.hint("layouts")})`}>
        Tile
      </button>
      <Show when={props.keyboard}>{(keyboard) => <KeyboardIndicator controller={c} keyboard={keyboard()} />}</Show>
      <Clock settings={clock} />
      <button
        type="button"
        class="panel-session"
        aria-haspopup="menu"
        aria-expanded={String(sessionOpen()) as "true" | "false"}
        aria-controls="session-menu"
        onClick={() => setSessionOpen(!sessionOpen())}
      >
        <span class="visually-hidden">Logged in as </span>
        {props.user}
      </button>
      <Popover id="session-menu" class="panel-session-menu" label="Session" open={sessionOpen()} onClose={() => setSessionOpen(false)}>
        <div role="menu" aria-label="Session">
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setSessionOpen(false);
              void (lockedFullscreen() ? exitLockedFullscreen() : enterLockedFullscreen()).catch(() => {});
            }}
          >
            {lockedFullscreen() ? "Leave full screen" : "Full screen"}
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setSessionOpen(false);
              props.onlock();
            }}
          >
            Lock
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setSessionOpen(false);
              props.onlogout();
            }}
          >
            Log out
          </button>
        </div>
      </Popover>
    </header>
    <Show when={dock()}>
      <Dock controller={c} launcher={launcherState()} onlauncher={saveLauncherState} />
    </Show>
    </>
  );
}
