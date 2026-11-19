---
id: F013
title: App interface and load on first use
status: done
phase: P03
tags: []
decisions: [ADR-001, ADR-022, ADR-023, ADR-025]
files: [web/src/apps/, web/src/wm/, web/src/panel/, spikes/e2e/wm.mjs]
---

## Goal
One small interface for all apps: open a window, open a channel.

## Requirements
- Each app is an ES module that loads on first use
- An app registers its name, icon, and start function
- The shell restores app windows after it connects again

## Acceptance
- Each app is an ES module in its own chunk (About 0.5 kB, Echo 2.1 kB, the shell 80 kB). Chromium confirmed that the About chunk loads only when its first window opens. A unit test confirms that the registry does not load a module.
- An app registers its name, icon, description, keywords, window size, and start function in manifest.ts. The registry finds each manifest, and refuses an id that is not the name of its folder. The launcher, the title bars, and the panel show the icons.
- The app interface is AppContext: open a channel, the state store with window keys, setTitle, openApp, the window id, and the user. Tests confirm each member, the close of the channels with the window, and the removal of the window keys on a close but not on a reconnect.
- The shell restores the app windows after it connects again: the desktop comes back from the state store, and each app reads its state through its keys. Chromium confirmed the windows and the echo log after a reload and after a break of the connection.
- A module that does not load shows a reload button. A test confirms it.
