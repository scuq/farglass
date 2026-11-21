---
id: F071
title: LCARS theme as the default theme of the shell
status: done
phase:
tags: [web, ui]
decisions: [ADR-049]
files: [web/index.html, web/src/main.tsx, web/src/theme/, web/src/appearance.ts, web/src/appearance.test.ts, web/src/apps/settings/pages/Appearance.tsx, web/src/apps/settings/settings.test.tsx, web/src/apps/terminal/xterm.ts]
---

## Goal
The whole shell has the LCARS look of the icon by default.

## Requirements
- A theme LCARS in Settings > Appearance, and it is the default with the accent orange
- Panel, windows, menus, launcher, notifications, overview, layout picker, settings, and terminal in LCARS colors and shapes
- Light, dark, and the system choice stay; errors keep a text and a red distinct from the decoration

## Acceptance
Settings > Appearance lists LCARS first, and DEFAULT_APPEARANCE is lcars with orange (appearance.test.ts). A Chromium screenshot of farglassd in dev_user mode at 1280x800 shows the panel as an LCARS bar, the launcher with an amber selection, focused and unfocused windows with LCARS title bars, and the settings app with an LCARS page column and LCARS swatches. All web tests pass.
