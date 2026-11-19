---
id: F044
title: Keybinding schemes with hints and an interactive tour
status: planned
phase: P03
tags: [shell]
decisions: [ADR-006]
files: [web/src/keys/, web/src/tour/]
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
