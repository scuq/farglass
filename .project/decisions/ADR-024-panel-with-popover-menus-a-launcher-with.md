---
id: ADR-024
title: Panel with popover menus, a launcher with search, and a desktop controller
status: accepted
date: 2026-11-19
tags: [shell, panel]
superseded_by:
files: [web/src/panel/, web/src/wm/controller.ts, web/src/apps.tsx]
---

## Context
F012 needs a panel with the windows, a clock, a session menu, and a launcher. Menus must be above all windows and close on a click outside. The lock screen comes with F021. Super reaches the browser only with Keyboard Lock (F017).

## Decision
- One panel at the top: Apps, the workspaces, the windows of the workspace, Tile, the clock, and the session menu. It replaces the header and the bar at the bottom of F011.
- Each menu is a Popover component: the Popover API gives the top layer, light dismiss, and Escape. CSS anchor positioning (anchor-name, position-anchor, position-area) puts it at its button. With no anchor positioning, a fixed position under the panel is the fallback. The component calls the Popover API only if the browser has it, and keeps its open state in sync through the toggle event.
- The launcher searches the title, the id, and the keywords of each app. Each word must match. A title that starts with the query comes first. The arrow keys and Enter select and open an app.
- A tap of Super (down and up with no other key) and Alt+F1 open the launcher.
- The session menu has Log out and a disabled Lock. F021 makes Lock work, because only farglassd can block the channels of a locked session.
- The state, the actions, and the keys of the desktop are in createDesktopController. The panel and the window surface share it.
- An app has a description and keywords for the launcher. The About app shows the user, the host, and the protocol version.

## Consequences
F013 moves the apps to modules, and the launcher lists them from the registry. F031 adds Linux apps to the same search. F021 enables Lock. F044 moves the Super tap and Alt+F1 into the keybinding schemes. The clock shows the time of the client, not of the host.
