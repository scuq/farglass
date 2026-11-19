---
id: ADR-025
title: Apps are folders with a manifest and a module that loads on first use
status: accepted
date: 2026-11-19
tags: [shell, apps]
superseded_by:
files: [web/src/apps/]
---

## Context
F013 needs one small app interface: open a window, open a channel. The shell must stay small, and an app must load only when the user opens it. Later apps (terminal F015, settings F022, Linux apps F031) use the same interface. A tab can run an old shell after the host got a new one.

## Decision
- An app is a folder web/src/apps/<id>/. manifest.ts exports manifest with defineApp: id, title, description, keywords, icon, size, and start. start is a dynamic import of the app module. The id must be the name of the folder.
- The registry finds the manifests with import.meta.glob, eager. It wraps start with lazy of Solid, thus Vite puts each app module in its own chunk, and the browser loads it at the first render of a window. The launcher calls preload when the pointer enters an entry.
- AppHost runs the app of one window in a Loading boundary and an Errored boundary. A failed import gives a reload button, because the host has possibly a newer shell. Other errors give a restart of the app.
- AppContext is the whole app interface: windowId, user, openChannel, state (watch, set, key), setTitle, and openApp. The channels of an app close with its window.
- The apps share the state channel of the desktop. state.key(name) gives <app>.<name>.<window id>. Closing a window removes its keys. The end of the desktop at a reconnect removes nothing, because the windows come back.

## Consequences
A new app is a new folder, with no change to a list. The terminal (F015) is the next app. F031 adds Linux apps to the launcher through a second source of entries. A key of a closed window stays in the store if the shell was not open at the close, for example after a crash of the browser.
