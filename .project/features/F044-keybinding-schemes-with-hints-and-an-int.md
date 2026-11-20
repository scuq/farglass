---
id: F044
title: Keybinding schemes with hints and an interactive tour
status: done
phase: P03
tags: [shell]
decisions: [ADR-006]
files: [web/src/keys/, web/src/tour/, web/src/wm/keys.ts, web/src/wm/controller.ts, web/src/apps/settings/pages/Keyboard.tsx]
---

## Goal
The shortcuts follow the conventions that the user knows, and the shell teaches them.

## Requirements
- Schemes for Windows and Mac conventions, selected from the operating system of the client by default
- Hints on the screen show the keys of the active scheme
- A binding never takes a key that input methods, screen readers, or apps use
- When the browser keeps a shortcut, the scheme has a second binding
- An interactive tour of the shortcuts
- Shortcuts for a new terminal window, a new window of the focused app, and closing the focused window that the browser does not reserve. The browser keeps Cmd/Ctrl+T, Cmd/Ctrl+W, and Cmd/Ctrl+Shift+T (reopen the closed tab). Candidates: Option/Alt+Shift+T, N, and W on each client, or Ctrl+Shift+T, N, and W on a Mac client. With Keyboard Lock, Cmd/Ctrl+T and Cmd/Ctrl+W work too

## Acceptance
Two schemes, Windows and Linux and Mac, chosen from the client by default and in Settings > Keyboard; each action with a Keyboard Lock key has a second key that reaches the browser (Alt+Shift on a pc, Ctrl+Shift and Option+Tab on a Mac); a test checks each key against a list of browser, input method, screen reader, AltGr, Option character, and common app keys; holding Alt+Shift or Ctrl+Shift shows the hints, tooltips and menus show the keys; new terminal, new window of the focused app, and close the window have keys, and Ctrl/Cmd+T, N, and W act with Keyboard Lock in shell windows only; an interactive tour of ten steps continues on each pressed key. Tests: scheme.test.tsx, wm, menu, and settings tests, and the wm and term e2e tests pass.
