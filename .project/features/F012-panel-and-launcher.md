---
id: F012
title: Panel and launcher
status: done
phase: P03
tags: []
decisions: [ADR-001, ADR-023, ADR-024]
files: [web/src/panel/, web/src/wm/, web/src/apps.tsx, web/src/About.tsx, web/src/App.tsx, web/src/shell.css, spikes/e2e/]
---

## Goal
A panel with the open windows, a clock, a session menu, and a launcher with search.

## Requirements
- Menus with the Popover API and CSS anchor positioning
- The session menu has lock and logout
- The launcher opens with the Super key when Keyboard Lock is active

## Acceptance
- The panel shows Apps, the workspaces, the windows of the workspace, Tile, the clock, and the session menu. A click on a window button gives the focus or minimizes.
- The launcher and the session menu are popovers in the top layer. Chromium confirmed that CSS anchor positioning puts them under their buttons, and that Escape and a click outside close them.
- The launcher searches the title, the id, and the keywords. Enter opens the selected app, and the search field has the keyboard focus. Unit tests cover the search order.
- The session menu has Log out, which logs out each tab, and Lock, which is disabled until the lock screen of F021.
- A tap of the Super key opens the launcher with Keyboard Lock. Chromium and unit tests confirmed it, and that Super with a second key does not open it. Alt+F1 opens it on each client.
- The clock changes at each new minute. A test with a fake time confirms it.
