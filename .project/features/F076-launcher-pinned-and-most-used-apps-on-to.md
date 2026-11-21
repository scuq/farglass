---
id: F076
title: Launcher: pinned and most used apps on top, one entry for the settings, new settings icon
status: done
phase:
tags: [web, shell]
decisions: [ADR-024]
files: [web/src/panel/Launcher.tsx, web/src/panel/Panel.tsx, web/src/panel/panel.css, web/src/panel/Panel.test.tsx, web/src/launcher/usage.ts, web/src/launcher/usage.test.ts, web/src/apps/settings/icon.svg, web/src/theme/lcars.css, web/src/apps/terminal/xterm.ts, README.md]
---

## Goal
The launcher shows first what the user starts most, and the settings are one entry.

## Requirements
- With no search text: the pinned apps, then the most used apps (a start counts less as it gets older, half after 14 days), then all other apps
- Alt+P or the button of the selected entry pins it or removes the pin
- The pins and the use are in the state store, thus each tab and each login of the user gets them
- The pages of the settings are not entries. A query that finds a page makes the settings entry open at that page
- The settings icon is the sliders icon
- The LCARS amber is dimmer: #e6a446 instead of #ffb23f (the login screen keeps its own amber)

## Acceptance
Panel.test.tsx: Alt+P pins Echo and the selection stays on it; a start of Terminal puts it under Most used; the button removes the pin; the state store gets key launcher with pinned and used. usage.test.ts: the score decays, and a pinned app is not under Most used. A query for timezone gives one entry, Settings, 'Opens the page Date and time'. In Chromium (dev_user, 1280x800): Brave pinned with Alt+P, Terminal under Most used, the sections stay after a reload, and Enter on the timezone result opens Settings at Date and time. The five login screenshots stay byte-identical with the dimmer amber; dark text on it has a contrast of 9:1. All web tests pass (263); the e2e tests launcher, wm, and settings pass.
