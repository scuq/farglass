---
id: F042
title: Launcher as explorer
status: done
phase: P03
tags: [shell]
decisions: [ADR-001]
files: [web/src/launcher/calc.ts, web/src/panel/Launcher.tsx, web/src/apps/settings/pages.ts, web/src/wm/controller.ts]
---

## Goal
One search field finds apps, settings pages, and files, and calculates expressions.

## Requirements
- Results come while you type, with the keyboard only
- Settings pages are results
- Calculations and unit conversions run in the client browser
- File results come from the fs channel when it exists

## Acceptance
One search field finds the apps of the shell, the apps of the host, and the settings pages by name or keyword, and a settings result opens the settings at that page (in the open settings window if one exists); calculations and unit conversions (length, mass, time, data, speed, temperature, volume, area, angle) run in the client with no eval and come first, and Enter copies the value; the keyboard alone works. File results wait for the fs channel, which does not exist yet. Tests: calc.test.ts and the launcher panel tests pass, and the settings and wm e2e tests pass.
