---
id: F020
title: Notifications
status: done
phase: P04
tags: []
decisions: [ADR-033]
files: [crates/farglass-session/src/notify.rs, crates/farglass-session/src/link.rs, crates/farglass-session/src/main.rs, crates/farglass-proto/src/message.rs, web/src/notify/, web/src/wm/Desktop.tsx, spikes/e2e/notify.mjs, docs/protocol.md]
---

## Goal
Desktop notifications from host programs show in the shell.

## Requirements
- The agent owns org.freedesktop.Notifications
- Actions and close events go back to the sender
- Notifications wait while no client is connected

## Acceptance
The agent owns org.freedesktop.Notifications (it waits in the bus queue when a local desktop owns it) and keeps up to 100 notifications, so a notification sent before a tab opens shows when the shell connects. The shell shows toasts at the top right; the time starts on screen and pauses on hover, focus, and a hidden tab; critical ones stay. Action buttons and a click on the toast send ActionInvoked, and dismiss, expire, and CloseNotification send NotificationClosed with reasons 2, 1, and 3. Tested by Rust unit and D-Bus tests, 6 vitest tests, and spikes/e2e/notify.mjs (12 steps through gdbus in Chromium).
