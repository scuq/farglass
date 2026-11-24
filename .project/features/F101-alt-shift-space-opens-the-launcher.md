---
id: F101
title: Alt+Shift+Space opens the launcher
status: done
phase: P03
tags: [shell]
decisions: [ADR-024]
files: [web/src/keys/scheme.ts, web/src/wm/keys.ts, web/src/keys/scheme.test.tsx, web/src/panel/panel.test.ts, spikes/e2e/wm.mjs, README.md, CHANGELOG.md]
---

## Goal
The launcher opens with a simple key in each browser, also on a laptop where F1 needs Fn.

## Requirements
- Alt+Shift+Space opens and closes the launcher in the Windows and Linux scheme, with no Keyboard Lock
- The hints, the tooltips, the Keyboard settings, and the tour show Alt+Shift+Space
- The Super tap and Alt+F1 stay

## Acceptance
Alt+Shift+Space opens and closes the launcher in the Windows and Linux scheme with no Keyboard Lock. The hints, the Keyboard settings, and the tour show it first, and the Super tap and Alt+F1 still work. Tests: panel.test.ts checks Alt+Shift+Space and that Shift+Space and Alt+Space do not open the launcher, scheme.test.tsx checks the key against the reserved keys and in the tour, and wm.mjs presses it in Chromium.
