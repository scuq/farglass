---
id: F096
title: Leave prompt and fullscreen with Keyboard Lock
status: done
phase: P03
tags: [shell, keys]
decisions: [ADR-006]
files: [web/src/wm/guard.ts, web/src/wm/guard.test.ts, web/src/wm/Desktop.tsx, web/src/panel/Panel.tsx, web/src/apps/settings/pages/Keyboard.tsx, CHANGELOG.md, README.md]
---

## Goal
Ctrl+W does not close the farglass tab by accident.

## Requirements
- The browser asks before it closes or reloads the tab while the desktop is open
- Full screen in the session menu, with Keyboard Lock on Chromium
- In full screen with Keyboard Lock, the browser does not act on Ctrl+W, Ctrl+T, and Ctrl+N

## Acceptance
The browser asks before Ctrl+W, a tab close, or a reload unloads the desktop. Session > Full screen enters fullscreen and calls Keyboard Lock where Chromium has it, and leaving fullscreen unlocks the keyboard. In fullscreen, the browser does not act on Ctrl+W, Ctrl+T, and Ctrl+N. guard.test.ts covers the leave prompt, the lock and unlock, fullscreen with no Keyboard Lock, and Ctrl+W only in fullscreen.
