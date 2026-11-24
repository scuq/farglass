---
id: F098
title: Dock layout of the panel
status: done
phase: P03
tags: [shell, panel]
decisions: [ADR-024]
files: [web/src/panel/Dock.tsx, web/src/panel/dock.ts, web/src/panel/dock.test.ts, web/src/panel/Panel.tsx, web/src/panel/panel.css, web/src/theme/lcars.css, web/src/launcher/hostApps.ts, web/src/appearance.ts, web/src/appearance.test.ts, web/src/apps/settings/pages/Appearance.tsx, CHANGELOG.md, README.md]
---

## Goal
A second layout of the panel, like the Ubuntu dock: app icons on the left with a dot for each window. Settings > Appearance switches it. The top bar stays the default.

## Requirements
- Settings > Appearance: Panel is Top bar (default) or Dock on the left
- The dock shows the pinned apps of the launcher first, then the other apps with windows on the workspace, one icon for each app
- A dot for each window of the app, four at most; the app with the focus is marked
- A click on an app with no window starts it; on an app with windows it raises the last used one, and the next clicks go through its windows
- The context menu pins or unpins the app and opens a new window
- The top bar keeps Apps, the workspaces, the clock, and the session menu, with no window buttons

## Acceptance
Settings > Appearance > Panel switches between the bar and a dock on the left; the bar is the default. The dock shows the pinned apps of the launcher, then the other apps with windows on the workspace, one icon each with a dot for each window. A click starts an app, raises its last window, and the next clicks go through its windows; one window with the focus minimizes. The context menu pins, opens a new window, and closes. The LCARS theme gives the dock its tiles and the Apps block the width of the dock. dock.test.ts covers the order and the cycle; a Playwright run on a demo instance checked the dock with two terminals, gnome-calculator, a pin, and the switch back to the bar.
