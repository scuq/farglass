---
id: F031
title: Linux apps in the launcher
status: done
phase: P05
tags: []
decisions: [ADR-007]
files: [crates/farglass-session/src/launch.rs, crates/farglass-session/src/link.rs, crates/farglass-proto/src/message.rs, web/src/launcher/, web/src/panel/Launcher.tsx, web/src/panel/Panel.tsx, web/src/wm/Surface.tsx, spikes/e2e/launcher.mjs]
---

## Goal
The launcher lists desktop entries and starts them on the compositor.

## Requirements
- Read the desktop entries and icons of the host
- Each app in its own systemd --user scope
- A surface window shows in the panel as other windows do

## Acceptance
The launcher lists the desktop entries of XDG_DATA_HOME and XDG_DATA_DIRS in the language of LANG, with hicolor or pixmaps icons; a click starts the app with the session Wayland socket in its own systemd --user scope, and its windows show the entry icon on the frame and in the panel. Tests: launch unit tests (Exec field codes, locale, visibility rules, directory order, icons), the apps channel test with a real start, the panel test (list, icon, failure, start), and spikes/e2e/launcher.mjs pass.
