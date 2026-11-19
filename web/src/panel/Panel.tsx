import { createSignal, For } from "solid-js";
import type { DesktopController } from "../wm/controller";
import type { Win } from "../wm/model";
import { Clock } from "./Clock";
import { Launcher } from "./Launcher";
import { Popover } from "./Popover";
import "./panel.css";

/**
 * The panel at the top: the launcher, the workspaces, the open windows of the
 * workspace, the clock, and the session menu.
 */
export function Panel(props: { controller: DesktopController; user: string; onlogout: () => void }) {
  const c = props.controller;
  const [sessionOpen, setSessionOpen] = createSignal(false);

  const tasks = () =>
    c
      .desktop()
      .windows.filter((win) => win.workspace === c.desktop().workspace)
      // The stack order changes with each focus. The title order stays.
      .toSorted((a, b) => a.title.localeCompare(b.title, undefined, { numeric: true }));

  return (
    <header class="panel">
      <button
        type="button"
        class="panel-apps"
        aria-haspopup="dialog"
        aria-expanded={String(c.launcherOpen()) as "true" | "false"}
        aria-controls="launcher"
        onClick={() => c.setLauncherOpen(!c.launcherOpen())}
      >
        Apps
      </button>
      <Popover id="launcher" class="panel-launcher" label="Launcher" open={c.launcherOpen()} onClose={() => c.setLauncherOpen(false)}>
        <Launcher
          onopen={(appId) => {
            c.setLauncherOpen(false);
            c.openApp(appId);
          }}
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

      <nav class="panel-tasks" aria-label="Windows">
        <For each={tasks()} keyed={(win: Win) => win.id}>
          {(win) => (
            <button
              type="button"
              class="wm-task"
              aria-pressed={String(c.focusedId() === win().id) as "true" | "false"}
              data-minimized={String(win().minimized)}
              onClick={() => (c.focusedId() === win().id ? c.minimize(win().id) : c.activate(win().id))}
            >
              {win().title}
            </button>
          )}
        </For>
      </nav>

      <button type="button" class="panel-tile" onClick={() => c.tile()} title="Place the windows of the workspace in columns">
        Tile
      </button>
      <Clock />
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
            aria-disabled="true"
            title="The lock screen comes in a later version of farglass."
            onClick={(event) => event.preventDefault()}
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
  );
}
