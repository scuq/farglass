/** The context menus of the window manager: a window, and the empty desktop. */

import { APPS } from "../apps/registry";
import type { MenuItem } from "../menu/menu";
import type { DesktopController } from "./controller";
import { windowById } from "./model";

/** Returns the menu of the window `id`: the title bar and its button in the panel. */
export function windowMenu(c: DesktopController, id: string): MenuItem[] {
  const win = windowById(c.desktop(), id);
  if (!win) {
    return [];
  }
  const maximized = win.snap === "maximized";
  const others = Array.from({ length: c.desktop().workspaces }, (_, index) => index).filter(
    (index) => index !== win.workspace,
  );
  return [
    { label: win.minimized ? "Show" : "Minimize", action: () => (win.minimized ? c.activate(id) : c.minimize(id)) },
    { label: maximized ? "Restore" : "Maximize", action: () => c.toggleMaximize(id), shortcut: "Super+Up" },
    { label: "Snap to the left half", action: () => c.snap(id, "left"), shortcut: "Super+Left" },
    { label: "Snap to the right half", action: () => c.snap(id, "right"), shortcut: "Super+Right" },
    { separator: true },
    ...others.map((index) => ({
      label: `Move to workspace ${index + 1}`,
      action: () => c.moveToWorkspace(id, index),
    })),
    { separator: true },
    { label: "Close", action: () => c.close(id) },
  ];
}

/** Returns the menu of the empty desktop: open each app, and tile the windows. */
export function desktopMenu(c: DesktopController): MenuItem[] {
  const apps = Object.values(APPS)
    .toSorted((a, b) => a.title.localeCompare(b.title))
    .map((app) => ({ label: `Open ${app.title}`, action: () => c.openApp(app.id) }));
  return [...apps, { separator: true }, { label: "Tile the windows", action: () => c.tile(), shortcut: "Super+T" }];
}
