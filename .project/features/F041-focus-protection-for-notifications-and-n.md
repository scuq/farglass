---
id: F041
title: Focus protection for notifications and new windows
status: done
phase: P04
tags: [shell]
decisions: [ADR-001]
files: [web/src/wm/focus.ts, web/src/wm/model.ts, web/src/wm/controller.ts, web/src/notify/Notifications.tsx, web/src/apps/settings/pages/Focus.tsx, web/src/panel/, web/src/surface/manager.ts, spikes/e2e/]
---

## Goal
The desktop does not interrupt the user.

## Requirements
- Hold notifications while the user types or moves the pointer, and show them at a pause
- Urgent notifications show immediately
- A window that the user did not request opens with no focus and shows a mark in the panel
- A setting turns each rule on or off

## Acceptance
Notifications wait while the user types or moves the pointer and show at a 1.5 s pause or after 30 s, critical ones at once; a new Linux window gets the focus only after a launcher start, a click or Enter, or as a dialog of the focused window, else it opens behind with a mark on its panel button; the Focus settings page turns each rule off or on. Tests: the input watch, the model, the notification hold, and the settings page tests, and spikes/e2e/surface.mjs (a window started by the test opens behind with a mark that goes with the focus) pass; the other e2e tests raise their windows through the panel as a user does.
