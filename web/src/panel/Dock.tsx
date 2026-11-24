import { createMemo, For, onCleanup, untrack } from "solid-js";
import { APPS } from "../apps/registry";
import logo from "../brand/farglass.svg";
import { entryOfWindow, hostApps, isShellIcon, windowIcon } from "../launcher/hostApps";
import { type LauncherState, togglePin } from "../launcher/usage";
import { setContextMenu } from "../menu/menu";
import type { MenuItem } from "../menu/menu";
import type { DesktopController } from "../wm/controller";
import { windowById } from "../wm/model";
import { type DockItem, dockItems, MAX_DOTS, windowToShow } from "./dock";

/**
 * The dock on the left (F098): one icon for each pinned app and each app with
 * windows on the workspace, with a dot for each window.
 */
export function Dock(props: { controller: DesktopController; launcher: LauncherState; onlauncher: (next: LauncherState) => void }) {
  const c = props.controller;
  let order: string[] = [];

  // A pin of an app that is not there, for example an app that the host removed, does not show.
  const known = (key: string) => {
    if (key.startsWith("host:")) return hostApps()?.apps().some((app) => `host:${app.id}` === key) ?? false;
    const app = APPS[key];
    return app !== undefined && !app.hidden;
  };

  const items = createMemo(() => {
    const desktop = c.desktop();
    const windows = desktop.windows.filter((win) => win.workspace === desktop.workspace);
    const result = dockItems(windows, desktop.recent, props.launcher.pinned.filter(known), untrack(() => order), entryOfWindow);
    order = result.order;
    return result.items;
  });

  const hostId = (key: string) => (key.startsWith("host:") ? key.slice("host:".length) : null);
  const firstWindow = (item: DockItem) => windowById(c.desktop(), item.windows[0] ?? "");

  const title = (item: DockItem) => {
    const host = hostId(item.key);
    if (host) return hostApps()?.apps().find((app) => app.id === host)?.name ?? host;
    return APPS[item.key]?.title ?? firstWindow(item)?.title ?? "";
  };
  const icon = (item: DockItem): string => {
    const host = hostId(item.key);
    if (host) return hostApps()?.icon(host) ?? logo;
    const first = firstWindow(item);
    return APPS[item.key]?.icon ?? (first ? windowIcon(first) : logo);
  };
  const canStart = (item: DockItem) => hostId(item.key) !== null || (APPS[item.key] !== undefined && !APPS[item.key]!.hidden);

  const start = (item: DockItem) => {
    const host = hostId(item.key);
    if (!host) {
      c.openApp(item.key);
      return;
    }
    const apps = hostApps();
    if (!apps) return;
    // The window of the app gets the focus: the user asked for it (F041).
    c.expectLaunch(apps.apps().find((app) => app.id === host)?.names ?? []);
    apps.launch(host).catch(() => {});
  };

  const click = (item: DockItem) => {
    const target = windowToShow(item, c.focusedId());
    if (target === null) {
      start(item);
    } else if (item.windows.length === 1 && target === c.focusedId() && !windowById(c.desktop(), target)?.minimized) {
      // One window with the focus: a click minimizes it, as in the bar.
      c.minimize(target);
    } else {
      c.activate(target);
    }
  };

  const menu = (key: string): MenuItem[] => {
    const item = items().find((each) => each.key === key);
    if (!item) return [];
    const entries: MenuItem[] = [];
    if (canStart(item)) {
      entries.push({ label: item.windows.length > 0 ? "New window" : "Open", action: () => start(item) });
      entries.push({ label: item.pinned ? "Unpin from the dock" : "Pin to the dock", action: () => props.onlauncher(togglePin(props.launcher, key)) });
    }
    if (item.windows.length > 0) {
      entries.push({ label: item.windows.length === 1 ? "Close" : `Close ${item.windows.length} windows`, action: () => item.windows.forEach((id) => c.close(id)) });
    }
    return entries;
  };

  return (
    <nav class="dock" aria-label="Dock">
      <For each={items()} keyed={(item: DockItem) => item.key}>
        {(item) => {
          const key = untrack(() => item().key);
          let removeMenu: (() => void) | undefined;
          onCleanup(() => removeMenu?.());
          const focused = () => item().windows.includes(c.focusedId() ?? "");
          const attention = () => item().windows.some((id) => windowById(c.desktop(), id)?.attention);
          const count = () => item().windows.length;
          return (
            <button
              type="button"
              class="dock-item"
              ref={(el) => {
                removeMenu = setContextMenu(el, () => menu(key));
              }}
              aria-pressed={String(focused()) as "true" | "false"}
              aria-label={count() > 0 ? `${title(item())}, ${count()} ${count() === 1 ? "window" : "windows"}` : title(item())}
              title={title(item())}
              data-attention={attention() ? "true" : undefined}
              data-pinned={item().pinned ? "true" : undefined}
              onClick={() => click(item())}
            >
              <img class="dock-icon" src={icon(item())} alt="" data-shell-icon={isShellIcon(icon(item())) ? "true" : undefined} />
              <span class="dock-dots" aria-hidden="true">
                <For each={Array.from({ length: Math.min(count(), MAX_DOTS) }, (_, index) => index)}>{() => <span class="dock-dot" />}</For>
              </span>
            </button>
          );
        }}
      </For>
    </nav>
  );
}
